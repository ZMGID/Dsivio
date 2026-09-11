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


class TemplateSharingTests(unittest.TestCase):
    def test_repairs_stale_async_page_config_without_changing_credentials(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True):
            root = Path(tmp) / "image-studio"
            root.mkdir()
            (root / "config.json").write_text(json.dumps({"providerId": "p", "model": "gpt-image-2", "protocol": "async"}))
            settings = {"settings": {"providers": [{"id": "p", "baseUrl": "https://img.hezu.ink/v1", "apiKeys": ["test-key"]}]}}
            (root.parent / "settings.json").write_text(json.dumps(settings))
            with patch.dict(os.environ, {"DSIVIO_IMAGE_STUDIO_DIR": str(root)}):
                values = dsivio.image_config()
                self.assertEqual(values["IMG_API_MODE"], "sync")
                self.assertEqual(values["IMG_API_KEY"], "test-key")
                settings["settings"]["providers"][0]["baseUrl"] = "https://api.apimart.ai/v1"
                (root.parent / "settings.json").write_text(json.dumps(settings))
                self.assertEqual(dsivio.image_config()["IMG_API_MODE"], "async")

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
