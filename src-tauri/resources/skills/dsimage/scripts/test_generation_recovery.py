import argparse
import collections
import http.client
import urllib.error
import io
import json
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch

import gen_image as gen


def args(**overrides):
    values = dict(n=1, image=None, format="png", resolution="1k", size="1:1",
                  quality=None, timeout=None, poll_interval=1)
    return argparse.Namespace(**(values | overrides))


class GenerationRecoveryTests(unittest.TestCase):
    def test_explicit_server_error_can_be_retried(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = json.dumps({"data": [{"b64_json": "aGVsbG8="}]}).encode()
            error = urllib.error.HTTPError("https://api.example", 503, "busy", {}, io.BytesIO(b"busy"))
            with patch.object(gen.urllib.request, "urlopen", side_effect=[error, io.BytesIO(response)]) as post, patch.object(gen.time, "sleep"):
                result = gen.generate_with_retry("https://api.example", "k", "gpt-image-2", "sync", args(), "cat", Path(tmp), "test", "h1")
            self.assertEqual(post.call_count, 2)
            self.assertEqual(result[0].read_bytes(), b"hello")

    def test_async_multiple_images_keep_distinct_names(self):
        with patch.object(gen, "run_async", side_effect=lambda *a: [Path(a[3]) / (a[-1] + ".png")]) as submit:
            result = gen.run_async_adapter("https://api.example", "k", args(n=2), "cat", "m", Path("/tmp"), "png", "test", "h1")
        self.assertEqual(submit.call_count, 2)
        self.assertEqual([p.name for p in result], ["h1.png", "h1-2.png"])

    def test_ybw_gpt_image_uses_openai_async_contract(self):
        with tempfile.TemporaryDirectory() as tmp, \
             patch.object(gen, "http_post", return_value={"task_id": "imgtask_1"}) as post, \
             patch.object(gen, "_poll_task", return_value={
                 "status": "completed", "result": {"data": [{"b64_json": "aGVsbG8="}]}
             }) as poll:
            result = gen.run_async_adapter(
                "https://ybw-ai.com", "k", args(size="16:9", resolution="4k"),
                "cat", "gpt-image-2", Path(tmp), "png", "test", "h1",
            )
            saved = result[0].read_bytes()
        self.assertEqual(post.call_args.args[0], "https://ybw-ai.com/v1/images/generations/async")
        self.assertEqual(post.call_args.args[2]["size"], "3840x2160")
        self.assertNotIn("resolution", post.call_args.args[2])
        self.assertEqual(poll.call_args.kwargs["task_path"], "images/tasks")
        self.assertEqual(saved, b"hello")

    def test_ybw_gpt_edit_uses_multipart_async_endpoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "source.png"
            source.write_bytes(b"image")
            with patch.object(gen, "http_post_multipart", return_value={"id": "imgtask_2"}) as post, \
                 patch.object(gen, "_poll_task", return_value={
                     "status": "completed", "result": {"data": [{"b64_json": "aGVsbG8="}]}
                 }):
                gen.run_async_adapter(
                    "https://ybw-ai.com", "k", args(image=[str(source)]),
                    "edit", "gpt-image-2", Path(tmp), "png", "test", "h1",
                )
        self.assertEqual(post.call_args.args[0], "https://ybw-ai.com/v1/images/edits/async")
        self.assertEqual(post.call_args.args[3][0][0], "image[]")

    def test_sync_4k_uses_extended_timeout(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(gen, "http_post", return_value={"data": [{"b64_json": "aGVsbG8="}]}) as post:
            gen.run_sync("https://api.example", "k", args(resolution="4k"), "cat", "gpt-image-2", Path(tmp), "png")
            self.assertEqual(post.call_args.kwargs["timeout"], 480)

    def test_download_retry_reuses_response_without_posting_again(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            wire = json.dumps({"data": [{"url": "https://cdn.example/image.png"}]}).encode()
            with patch.object(gen.urllib.request, "urlopen", return_value=io.BytesIO(wire)) as post:
                with patch.object(gen, "download_to_path", side_effect=gen.GenError("下载图片超时。")):
                    with self.assertRaises(gen.GenError):
                        gen.generate_one("https://api.example/v1", "secret", "gpt-image-2", "sync", args(), "cat", root, "test", "h1")
                records = list(root.glob(".dsimage-recovery/*/*.json"))
                self.assertEqual(len(records), 1)
                self.assertNotIn("secret", records[0].read_text())
                with patch.object(gen, "download_to_path", side_effect=lambda url, path: path.write_bytes(b"image")):
                    result = gen.generate_one("https://api.example/v1", "secret", "gpt-image-2", "sync", args(), "cat", root, "test", "h1")
                self.assertEqual(post.call_count, 1)
                self.assertEqual(result[0].read_bytes(), b"image")
                self.assertEqual(list(root.glob(".dsimage-recovery/*/*.json")), [])

    def test_unknown_submission_is_retained_for_manual_reconciliation(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.object(gen.urllib.request, "urlopen", side_effect=http.client.RemoteDisconnected()) as post:
                for _ in range(2):
                    with self.assertRaisesRegex(gen.GenError, "提交结果未知"):
                        gen.generate_one("https://api.example/v1", "k", "gpt-image-2", "sync", args(), "cat", Path(tmp), "test", "h1")
                self.assertEqual(post.call_count, 1)

    def test_async_poll_retry_uses_the_same_task(self):
        with tempfile.TemporaryDirectory() as tmp:
            wire = json.dumps({"data": [{"task_id": "task-123"}]}).encode()
            with patch.object(gen.urllib.request, "urlopen", return_value=io.BytesIO(wire)) as post, \
                 patch.object(gen.time, "sleep"), \
                 patch.object(gen, "_poll_task", side_effect=[gen.GenError("查询接口连接失败或超时。"), {"result": {}}]) as poll, \
                 patch.object(gen, "_save_async_images", return_value=[Path(tmp) / "h1.png"]):
                with self.assertRaises(gen.GenError):
                    gen.generate_one("https://api.example/v1", "k", "gpt-image-2", "async", args(), "cat", Path(tmp), "test", "h1")
                gen.generate_one("https://api.example/v1", "k", "gpt-image-2", "async", args(), "cat", Path(tmp), "test", "h1")
                self.assertEqual(post.call_count, 1)
                self.assertEqual([call.args[2] for call in poll.call_args_list], ["task-123", "task-123"])

    def test_pool_preserves_parallelism_and_retries_only_failed_slots(self):
        with tempfile.TemporaryDirectory() as tmp:
            jobs = [dict(slot=f"h{i}", prompt=str(i), args=args(), output_dir=Path(tmp)) for i in range(6)]
            barrier = threading.Barrier(3)
            lock = threading.Lock()
            calls = collections.Counter()
            active = peak = 0

            def generate(base, key, model, mode, options, prompt, output, label, prefix):
                nonlocal active, peak
                with lock:
                    calls[prompt] += 1
                    active += 1
                    peak = max(peak, active)
                    attempt = calls[prompt]
                if attempt == 1:
                    barrier.wait(timeout=3)
                with lock:
                    active -= 1
                if prompt == "0" and attempt == 1:
                    raise gen.GenError("接口返回 HTTP 429：rate limit")
                return [output / f"{prefix}.png"]

            with patch.object(gen, "generate_one", side_effect=generate), patch.object(gen.time, "sleep"):
                result = gen.run_job_pool(jobs, concurrency=3, skip_existing=False, base_url="u", api_key="k", model="m", mode="sync")
            self.assertEqual(peak, 3)
            self.assertTrue(all(status == "ok" for status, _ in result.values()))
            self.assertEqual(calls["0"], 2)
            self.assertTrue(all(calls[str(i)] == 1 for i in range(1, 6)))

    def test_pool_retries_a_rate_limit_even_at_concurrency_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            job = dict(slot="h1", prompt="cat", args=args(), output_dir=Path(tmp))
            with patch.object(gen, "generate_one", side_effect=[gen.GenError("HTTP 429"), [Path(tmp) / "h1.png"]]) as generate, patch.object(gen.time, "sleep") as sleep:
                result = gen.run_job_pool([job], concurrency=1, skip_existing=False, base_url="u", api_key="k", model="m", mode="sync")
            self.assertEqual(result["h1"][0], "ok")
            self.assertEqual(generate.call_count, 2)
            sleep.assert_called_once_with(15)

    def test_pool_does_not_skip_a_partially_saved_multi_image_slot(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "h1.png").write_bytes(b"first")
            job = dict(slot="h1", prompt="cat", args=args(n=2), output_dir=Path(tmp))
            with patch.object(gen, "generate_one", return_value=[]) as generate:
                gen.run_job_pool([job], concurrency=2, skip_existing=True, base_url="u", api_key="k", model="m", mode="sync")
            generate.assert_called_once()
            with self.assertRaisesRegex(gen.GenError, "重复"):
                gen.run_job_pool([job, job], concurrency=2, skip_existing=True, base_url="u", api_key="k", model="m", mode="sync")

    def test_private_imagen_compat_uses_predict_and_keeps_sample_count(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(gen, "http_post", return_value={"predictions": [{"bytesBase64Encoded": "aGVsbG8="}]}) as post:
            gen.run_gemini("https://gateway.example/v1beta", "k", args(n=2, resolution="2k"), "cat", "models/imagen-4.0-generate-001", Path(tmp), "png")
            self.assertTrue(post.call_args.args[0].endswith(":predict"))
            self.assertEqual(post.call_args.args[2]["parameters"]["sampleCount"], 2)
            self.assertEqual(post.call_args.args[2]["parameters"]["imageSize"], "2K")

    def test_official_imagen_is_retired_without_request(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(gen, "http_post") as post:
            with self.assertRaisesRegex(gen.GenError, "2026-08-17"):
                gen.run_gemini(
                    "https://generativelanguage.googleapis.com/v1beta", "k", args(),
                    "cat", "models/imagen-4.0-generate-001", Path(tmp), "png",
                )
            post.assert_not_called()


if __name__ == "__main__":
    unittest.main()
