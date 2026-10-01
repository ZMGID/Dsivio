"""Loopback-only, single-inference WhisperX service (no runtime downloads)."""
import argparse
from dataclasses import replace
import importlib.metadata
import json
import hmac
import logging
import os
from pathlib import Path
import re
import select
import signal
import stat
import sys
import socket
import tempfile
import threading
import time
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PROTOCOL = "dsivio-video.asr/1"
SERVICE_VERSION = "0.2.0"
MAX_REQUEST = 64 * 1024
MAX_AUDIO = 512 * 1024 * 1024
SIMPLIFIED_CHINESE_PROMPT = "这是一段普通话语音，请用简体中文准确记录说话内容。"


class ServiceError(Exception):
    def __init__(self, code, message, status=400):
        super().__init__(message)
        self.code, self.status = code, status


def deny_download(*args, **kwargs):
    raise ServiceError("RESOURCE_NOT_PREPARED", "Resource is not cached; run dsivio-video setup asr", 503)


class Engine:
    def __init__(self, args):
        os.environ.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", HF_HUB_DISABLE_TELEMETRY="1")
        import nltk
        import numpy as np
        import torch
        import whisperx
        import torchaudio
        from whisperx.alignment import DEFAULT_ALIGN_MODELS_HF, DEFAULT_ALIGN_MODELS_TORCH
        self.args, self.np, self.whisperx = args, np, whisperx
        self.torch_models, self.hf_models = DEFAULT_ALIGN_MODELS_TORCH, DEFAULT_ALIGN_MODELS_HF
        self.cache = Path(args.cache).resolve()
        nltk.data.path.insert(0, str(self.cache / "nltk"))
        nltk.download = deny_download
        torch.hub.download_url_to_file = deny_download
        # torchaudio's imported download alias also needs to be blocked.
        import torchaudio._internal
        torchaudio._internal.download_url_to_file = deny_download
        self.aligners = {}
        # The pinned wheel's trusted legacy VAD checkpoint includes configuration
        # classes. Keep PyTorch's weights-only loading, with an explicit allow-list.
        from collections import defaultdict
        from typing import Any
        from omegaconf import ListConfig
        from omegaconf.base import ContainerMetadata, Metadata
        from omegaconf.nodes import AnyNode
        from pyannote.audio.core.model import Introspection
        from pyannote.audio.core.task import Problem, Resolution, Specifications
        from torch.torch_version import TorchVersion
        with torch.serialization.safe_globals([defaultdict, ListConfig, ContainerMetadata,
                                               Metadata, AnyNode, Introspection, Problem,
                                               Resolution, Specifications, TorchVersion,
                                               Any, list, dict, int]):
            self.model = whisperx.load_model(args.model, args.device, compute_type=args.compute,
                                            download_root=str(self.cache / "huggingface"), local_files_only=True)
        logging.info("ASR loaded: model=%s device=%s compute=%s", args.model, args.device, args.compute)

    def aligner(self, language):
        if language in self.aligners:
            return self.aligners[language]
        if language not in self.torch_models and language not in self.hf_models:
            raise ServiceError("INVALID_INPUT", f"No default alignment model for language {language}")
        directory = self.cache / ("torch" if language in self.torch_models else "huggingface")
        if language in self.torch_models:
            import torchaudio
            bundle = getattr(torchaudio.pipelines, self.torch_models[language])
            checkpoint = directory / Path(bundle._path).name
            if not checkpoint.is_file():
                raise ServiceError("RESOURCE_NOT_PREPARED", f"Alignment resources for {language} are not cached; run dsivio-video setup asr", 503)
        try:
            value = self.whisperx.load_align_model(language, self.args.device, model_dir=str(directory), model_cache_only=True)
        except (ValueError, OSError, LookupError) as error:
            raise ServiceError("RESOURCE_NOT_PREPARED", f"Alignment resources for {language} are not cached; run dsivio-video setup asr", 503) from error
        self.aligners[language] = value
        logging.info("Alignment model loaded: language=%s", language)
        return value

    def transcribe(self, pcm, language):
        if self.args.model.endswith(".en") and language not in (None, "en"):
            raise ServiceError("INVALID_INPUT", "English-only ASR model cannot transcribe this language")
        if language is not None:
            self.aligner(language)
        audio = self.np.frombuffer(pcm, dtype="<i2").astype(self.np.float32) / 32768.0
        started = time.monotonic()
        # WhisperX exposes initial_prompt through its immutable faster-whisper
        # options. The inference lock makes this request-scoped replacement safe.
        original_options = self.model.options
        self.model.options = replace(original_options, initial_prompt=SIMPLIFIED_CHINESE_PROMPT if language == "zh" else None)
        try:
            result = self.model.transcribe(audio, batch_size=self.args.batch_size, language=language)
        finally:
            self.model.options = original_options
        language = result["language"]
        if not result["segments"]:
            return {"language": language, "segments": []}
        model, metadata = self.aligner(language)
        aligned = self.whisperx.align(result["segments"], model, metadata, audio, self.args.device, return_char_alignments=False)
        segments = []
        for segment in aligned["segments"]:
            words = []
            for word in segment.get("words", []):
                words.append({"text": word.get("word", ""), **{key: word[key] for key in ("start", "end", "score") if key in word}})
            segments.append({"text": segment["text"], "words": words, **{key: segment[key] for key in ("start", "end") if key in segment}})
        logging.info("Transcription and alignment complete: language=%s seconds=%.3f", language, time.monotonic() - started)
        return {"language": language, "segments": segments}


def read_audio(path, roots, sample_frames=None):
    if not isinstance(path, str) or not Path(path).is_absolute():
        raise ServiceError("INVALID_AUDIO_PATH", "audio_path must be an absolute path")
    try:
        resolved = Path(path).resolve(strict=True)
        if not any(resolved.is_relative_to(root) for root in roots) or not resolved.is_file():
            raise ServiceError("INVALID_AUDIO_PATH", "Audio must be a regular file inside an allowed root")
    except (OSError, RuntimeError) as error:
        raise ServiceError("INVALID_AUDIO_PATH", f"Cannot resolve audio_path: {error}") from error
    try:
        if resolved.stat().st_size > MAX_AUDIO:
            raise ServiceError("INVALID_INPUT", "Audio exceeds the 512 MiB limit")
        with wave.open(str(resolved), "rb") as wav:
            if (wav.getframerate(), wav.getnchannels(), wav.getsampwidth(), wav.getcomptype()) != (16000, 1, 2, "NONE"):
                raise ServiceError("INVALID_INPUT", "Audio must be 16 kHz mono 16-bit PCM WAV")
            count = wav.getnframes()
            pcm = wav.readframes(count)
            if count <= 0 or len(pcm) != count * 2 or wav._data_chunk.chunksize != count * 2:
                raise ServiceError("INVALID_INPUT", "WAV has empty or truncated PCM data")
            if sample_frames is not None and (type(sample_frames) is not int or sample_frames <= 0 or sample_frames > 9007199254740991 or sample_frames != count):
                raise ServiceError("INVALID_INPUT", "sample_frames must equal the exact WAV PCM frame count")
            return pcm
    except (OSError, wave.Error, EOFError) as error:
        raise ServiceError("INVALID_INPUT", f"Cannot read WAV: {error}") from error


class Handler(BaseHTTPRequestHandler):
    def reply(self, status, value):
        body = json.dumps(value, ensure_ascii=False, allow_nan=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.wfile.flush()

    def authorize(self):
        hosts = self.headers.get_all("Host") or []
        expected = {f"127.0.0.1:{self.server.server_port}", f"localhost:{self.server.server_port}"}
        if len(hosts) != 1 or hosts[0] not in expected or self.headers.get_all("Origin"):
            raise ServiceError("FORBIDDEN", "Only non-browser local requests are accepted", 403)
        values = self.headers.get_all("Authorization") or []
        if len(values) != 1 or not hmac.compare_digest(values[0], "Bearer " + self.server.token):
            raise ServiceError("UNAUTHORIZED", "ASR session authentication failed", 401)

    def do_GET(self):
        try:
            self.authorize()
            if self.path != "/health":
                raise ServiceError("NOT_FOUND", "Unknown endpoint", 404)
            args = self.server.engine.args
            self.reply(200, {"ok": True, "protocol": PROTOCOL, "serviceVersion": SERVICE_VERSION,
                             "whisperxVersion": importlib.metadata.version("whisperx"), "model": args.model,
                             "device": args.device, "compute": args.compute, "batchSize": args.batch_size,
                             "busy": self.server.inference_lock.locked(), "activeTaskId": self.server.active_task_id})
        except ServiceError as error:
            self.reply(error.status, {"error": {"code": error.code, "message": str(error)}})

    def disconnected(self, finished):
        # A synchronous inference has exactly one live owner. Losing that socket
        # must stop CPU work, not leave an anonymous inference behind.
        while not finished.wait(0.1):
            try:
                readable, _, _ = select.select([self.connection], [], [], 0)
                if readable and self.connection.recv(1, socket.MSG_PEEK) == b"":
                    logging.info("Inference owner disconnected; stopping service")
                    os._exit(0)
            except OSError:
                if not finished.is_set():
                    os._exit(0)

    def do_POST(self):
        acquired = False
        finished = threading.Event()
        try:
            self.authorize()
            if self.path not in ("/transcribe", "/shutdown", "/cancel"):
                raise ServiceError("NOT_FOUND", "Unknown endpoint", 404)
            if self.headers.get_content_type() != "application/json":
                raise ServiceError("UNSUPPORTED_MEDIA_TYPE", "Expected application/json", 415)
            lengths = self.headers.get_all("Content-Length")
            if not lengths:
                raise ServiceError("LENGTH_REQUIRED", "Content-Length is required", 411)
            if len(lengths) != 1 or not re.fullmatch(r"[0-9]+", lengths[0]) or self.headers.get("Transfer-Encoding"):
                raise ServiceError("INVALID_LENGTH", "Invalid Content-Length")
            length = int(lengths[0])
            if length > MAX_REQUEST:
                raise ServiceError("REQUEST_TOO_LARGE", "Request exceeds 64 KiB", 413)
            self.connection.settimeout(10)
            try:
                body = self.rfile.read(length)
            except socket.timeout as error:
                raise ServiceError("TRUNCATED_REQUEST", "Request body is incomplete") from error
            if len(body) != length:
                raise ServiceError("TRUNCATED_REQUEST", "Request body is incomplete")
            try:
                request = json.loads(body, parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))
            except (ValueError, UnicodeError) as error:
                raise ServiceError("INVALID_JSON", "Request is not valid JSON") from error
            if self.path == "/cancel":
                if not isinstance(request, dict) or set(request) != {"task_id"} or not isinstance(request["task_id"], str):
                    raise ServiceError("INVALID_REQUEST", "Cancel requires task_id")
                if self.server.active_task_id != request["task_id"]:
                    self.reply(200, {"outcome": "too-late", "taskId": request["task_id"]})
                    return
                self.reply(200, {"outcome": "requested", "taskId": request["task_id"]})
                # WhisperX has no safe per-inference interrupt; the supervisor
                # confirms this owned child exits before publishing cancelled.
                os._exit(0)
            if self.path == "/shutdown":
                if request != {}:
                    raise ServiceError("INVALID_REQUEST", "Shutdown requires an empty JSON object")
                acquired = self.server.inference_lock.acquire(blocking=False)
                if not acquired:
                    raise ServiceError("ASR_BUSY", "Cannot stop ASR while an inference is running", 409)
                self.reply(200, {"ok": True})
                self.server.stop_requested = True
                return
            if not isinstance(request, dict) or not {"audio_path", "language"}.issubset(request):
                raise ServiceError("INVALID_REQUEST", "Request must contain audio_path and language")
            if set(request) - {"audio_path", "language", "task_id", "sample_frames", "timestamps"}:
                raise ServiceError("UNKNOWN_FIELD", "Unknown transcription request field")
            language = request["language"]
            if not isinstance(language, str) or not re.fullmatch(r"[a-z]{2,3}", language) or language in ("auto", "und"):
                raise ServiceError("INVALID_INPUT", "language must be a lowercase two- or three-letter code")
            if "sample_frames" in request and type(request["sample_frames"]) is not int:
                raise ServiceError("INVALID_INPUT", "sample_frames must be a positive safe integer")
            timestamps = request.get("timestamps", "word")
            task_id = request.get("task_id", "standalone")
            if timestamps not in ("word", "segment") or not isinstance(task_id, str) or not task_id or len(task_id) > 256:
                raise ServiceError("INVALID_INPUT", "Invalid timestamps or task_id")
            acquired = self.server.inference_lock.acquire(blocking=False)
            if not acquired:
                raise ServiceError("ASR_BUSY", "An inference is already running", 409)
            pcm = read_audio(request["audio_path"], self.server.roots, request.get("sample_frames"))
            self.server.active_task_id = task_id
            threading.Thread(target=self.disconnected, args=(finished,), daemon=True).start()
            result = self.server.engine.transcribe(pcm, language)
            if timestamps == "segment":
                for segment in result["segments"]:
                    segment.pop("words", None)
            result.update(schema="dsivio.media.transcript/1", sampleRate=16000, sampleFrames=len(pcm) // 2,
                          engine={"backend": "local", "model": self.server.engine.args.model,
                                  "protocol": PROTOCOL, "serviceVersion": SERVICE_VERSION,
                                  "whisperxVersion": importlib.metadata.version("whisperx")})
            finished.set()
            self.reply(200, result)
        except ServiceError as error:
            self.reply(error.status, {"error": {"code": error.code, "message": str(error)}})
        except LookupError:
            logging.exception("Missing cached inference resource")
            self.reply(503, {"error": {"code": "RESOURCE_NOT_PREPARED", "message": "Resource is not cached; run dsivio-video setup asr"}})
        except (BrokenPipeError, ConnectionResetError):
            if acquired:
                os._exit(0)
        except Exception:
            logging.exception("Inference failed")
            self.reply(500, {"error": {"code": "INFERENCE_FAILED", "message": "Inference failed; see the ASR service log"}})
        finally:
            finished.set()
            if acquired and not self.server.stop_requested:
                self.server.active_task_id = None
                self.server.last_activity = time.monotonic()
                self.server.inference_lock.release()

    def log_message(self, format, *args):
        logging.info(format, *args)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--model", default="small")
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--compute", default="int8")
    parser.add_argument("--batch-size", type=int, default=8)
    parser.add_argument("--cache", required=True)
    parser.add_argument("--allow-root", action="append", default=[])
    parser.add_argument("--token-file", required=True)
    parser.add_argument("--parent-pid", type=int, default=os.getppid())
    parser.add_argument("--supervised-stdin", action="store_true")
    parser.add_argument("--idle-seconds", type=float, default=300)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    token_path = Path(args.token_file)
    metadata = token_path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or (os.name != "nt" and (metadata.st_mode & 0o077 or metadata.st_uid != os.getuid())):
        raise ValueError("token-file must be a private regular file owned by the current user")
    token = token_path.read_text().strip()
    if not re.fullmatch(r"[A-Za-z0-9_-]{32,256}", token):
        raise ValueError("token-file must contain a random session nonce")
    if args.parent_pid <= 1 or args.parent_pid != os.getppid():
        raise ValueError("parent-pid must identify the actual live supervisor")

    def watch_parent():
        while True:
            time.sleep(0.2)
            if os.getppid() != args.parent_pid:
                os._exit(0)

    def watch_stdin():
        while sys.stdin.buffer.read(1):
            pass
        os._exit(0)

    threading.Thread(target=watch_parent, daemon=True).start()
    if args.supervised_stdin:
        threading.Thread(target=watch_stdin, daemon=True).start()
    signal.signal(signal.SIGTERM, lambda *_: os._exit(0))
    signal.signal(signal.SIGINT, lambda *_: os._exit(0))
    engine = Engine(args)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.daemon_threads = True
    server.engine = engine
    server.roots = [Path(root).resolve() for root in args.allow_root or [tempfile.gettempdir()]]
    server.token = token
    server.active_task_id = None
    server.inference_lock = threading.Lock()
    server.last_activity = time.monotonic()
    server.stop_requested = False
    server.timeout = 1
    logging.info("Listening on 127.0.0.1:%s", server.server_port)
    print(json.dumps({"port": server.server_port, "pid": os.getpid(), "protocol": PROTOCOL,
                      "serviceVersion": SERVICE_VERSION}), flush=True)
    try:
        while not server.stop_requested and (server.inference_lock.locked() or time.monotonic() - server.last_activity < args.idle_seconds):
            server.handle_request()
    finally:
        server.server_close()
        logging.info("Idle shutdown")


if __name__ == "__main__":
    main()
