import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import core
import dsimage
import dsivio
import gen_image


MODELS = [
    {"id": "p/gpt-image-2", "model": "gpt-image-2", "default": True, "description": {"factsRevision": "sha256:p", "arguments": {
        "aspectRatio": {"allowed": ["1:1", "4:5"]}, "size": {"allowed": ["1K", "2K", "4K"]},
        "quality": {"allowed": ["low", "medium", "high"]}, "n": {"maximum": 1}}}},
    {"id": "q/seedream", "model": "seedream", "default": False, "description": {"factsRevision": "sha256:q", "arguments": {
        "aspectRatio": {"allowed": ["1:1", "4:5"]}, "size": {"allowed": ["1K", "2K"]},
        "quality": {"allowed": ["low", "medium"]}, "n": {"maximum": 4}}}},
]


def job(**overrides):
    import argparse
    values = dict(n=1, image=None, format="png", resolution="1k", size="1:1", quality=None, timeout=None, poll_interval=1)
    return argparse.Namespace(**(values | overrides))


def flag(argv, name):
    return argv[argv.index(name) + 1]


class TemplateSharingTests(unittest.TestCase):
    def test_no_standalone_config_uses_dsivio_media_pool_without_reading_app_keys(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True), \
             patch.object(gen_image, "find_default_env_file", return_value=None), \
             patch.object(gen_image, "_dsivio", return_value=(0, MODELS, "")) as cli:
            base_url, api_key, model, mode = gen_image.resolve_backend(None, None, None)
        self.assertEqual((mode, model, api_key), ("dsivio", "p/gpt-image-2", ""))
        self.assertEqual(cli.call_args.args[0], ["models", "--kind", "image", "--json"])
        self.assertFalse(hasattr(dsivio, "image_config"))

    def test_template_pin_outside_pool_falls_back_but_explicit_model_fails(self):
        with patch.object(gen_image, "_dsivio", return_value=(0, MODELS, "")), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(gen_image.dsivio_model("grok-imagine-image-2.0", strict=False)["id"], "p/gpt-image-2")
            self.assertEqual(gen_image.dsivio_model("seedream", strict=True)["id"], "q/seedream")
            with self.assertRaises(gen_image.GenError):
                gen_image.dsivio_model("grok-imagine-image-2.0", strict=True)

    def test_dsivio_generation_maps_declared_arguments_and_names_outputs_by_slot(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            ref = tmp / "front.png"
            ref.write_bytes(b"ref")
            produced = tmp / "task" / "output.png"
            produced.parent.mkdir()
            produced.write_bytes(b"img")
            task = {"id": "t1", "status": "succeeded", "outputs": [{"path": str(produced), "mime": "image/png"}]}
            with patch.object(gen_image, "_dsivio", side_effect=[(0, MODELS, ""), (0, task, "")]) as cli:
                model = gen_image.dsivio_model("seedream", strict=True)["id"]
                paths = gen_image.generate_one("dsivio", "", model, "dsivio", job(image=[str(ref)], size="4:5", quality="high"),
                                               "白底主图", tmp / "out", "h1", "h1")
            argv = cli.call_args.args[0]
            self.assertEqual(cli.call_args.kwargs["stdin"], "白底主图")
            self.assertEqual(argv[:3], ["image", "--prompt-file", "-"])
            self.assertEqual(flag(argv, "--model"), "q/seedream")
            self.assertEqual(flag(argv, "--ref"), str(ref.resolve()))
            self.assertEqual(flag(argv, "--ratio"), "4:5")
            self.assertEqual(flag(argv, "--size"), "1K")
            self.assertEqual(flag(argv, "--source"), "dsimage")
            self.assertEqual(flag(argv, "--description-revision"), "sha256:q")
            self.assertNotIn("--quality", argv)  # 模型只允许 low/medium，不把模板默认 high 送去被拒
            self.assertEqual([p.name for p in paths], ["h1.png"])
            self.assertEqual(paths[0].read_bytes(), b"img")

    def test_uncertain_submission_keeps_key_and_rerun_never_submits_a_new_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            produced = tmp / "output.png"
            produced.write_bytes(b"img")
            done = (0, {"id": "t1", "status": "succeeded", "outputs": [{"path": str(produced), "mime": "image/png"}]}, "")
            with patch.object(gen_image, "_dsivio", side_effect=[(0, MODELS, ""), (5, None, "uncertain"), done, done]) as cli, \
                 contextlib.redirect_stderr(io.StringIO()):
                model = gen_image.dsivio_model(None, strict=False)["id"]
                run = lambda: gen_image.generate_with_retry("dsivio", "", model, "dsivio", job(), "cat", tmp, "h1", "h1")
                with self.assertRaises(gen_image.GenError) as error:
                    run()
                self.assertFalse(gen_image.is_backoff_error(str(error.exception)))
                run()
                keys = [flag(call.args[0], "--idempotency-key") for call in cli.call_args_list[1:3]]
                self.assertEqual(keys[0], keys[1])
                run()  # 成功后再出（--redo）是新的提交
                self.assertNotEqual(flag(cli.call_args_list[3].args[0], "--idempotency-key"), keys[0])
            self.assertEqual(cli.call_count, 4)

    def test_app_not_running_asks_to_open_dsivio_without_retrying(self):
        with patch.object(gen_image, "_dsivio", return_value=(6, None, "")):
            with self.assertRaises(gen_image.GenError) as error:
                gen_image.dsivio_model(None, strict=False)
        self.assertIn("打开 Dsivio", str(error.exception))
        self.assertFalse(gen_image.is_backoff_error(str(error.exception)))

    def test_disconnected_submission_is_not_automatically_resubmitted(self):
        import http.client
        import urllib.request
        with patch.object(gen_image.urllib.request, "urlopen", side_effect=http.client.RemoteDisconnected()):
            with self.assertRaises(gen_image.GenError) as error:
                gen_image._post_json(urllib.request.Request("https://example.com", data=b"{}"), 30, "接口")
        self.assertIn("RemoteDisconnected", str(error.exception))
        self.assertIn("30s", str(error.exception))
        self.assertFalse(gen_image.is_backoff_error(str(error.exception)))

    def test_chat_template_uses_page_library_without_task_or_config(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            with patch.dict(os.environ, {"DSIVIO_IMAGE_STUDIO_DIR": tmp}), patch.object(core, "TEMPLATES_DIR", root / "templates"):
                self.assertEqual(dsivio.templates_dir(), root / "templates")
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(dsimage.main(["template", "init", "聊天模板", "--blank", "--mode", "smart", "--slots", "3"]), 0)
                folder = core.find_template("聊天模板")
                path = folder / "template.json"
                data = json.loads(path.read_text())
                self.assertEqual(len(data["slots"]), 3)
                data["style"] = "页面编辑后的风格"
                path.write_text(json.dumps(data))
                self.assertEqual(core.load_template(folder)["style"], data["style"])
                self.assertFalse((root / "tasks").exists())
                self.assertFalse((root / "config.json").exists())

    def test_generation_reads_own_env_file(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True):
            env = Path(tmp) / ".env"
            env.write_text("IMG_MODEL=skill-model\nIMG_API_MODE=grok\n")
            gen_image.load_env_file(env)
            self.assertEqual(os.environ["IMG_MODEL"], "skill-model")
            self.assertEqual(os.environ["IMG_API_MODE"], "grok")

if __name__ == "__main__":
    unittest.main()
