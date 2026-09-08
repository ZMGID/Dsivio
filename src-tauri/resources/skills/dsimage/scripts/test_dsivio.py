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


class SharedWorkspaceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        env = patch.dict(os.environ, {"DSIVIO_IMAGE_STUDIO_DIR": str(self.root)})
        env.start()
        self.addCleanup(env.stop)
        templates = patch.object(core, "TEMPLATES_DIR", self.root / "templates")
        templates.start()
        self.addCleanup(templates.stop)

    def test_chat_created_template_is_standard_and_page_edits_are_read_next_time(self):
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(dsimage.main(["template", "init", "聊天模板", "--blank", "--mode", "smart", "--slots", "3"]), 0)
        path = core.find_template("聊天模板")
        data = dsivio.read_json(path / "template.json")
        self.assertEqual(len(data["slots"]), 3)
        data["style"] = "页面刚保存的风格"
        dsivio.atomic_json(path / "template.json", data)
        self.assertEqual(core.load_template(path)["style"], "页面刚保存的风格")
        self.assertFalse((path / "record.json").exists())

    def test_copy_preserves_assets_but_gets_new_ui_identity(self):
        folder = self.root / "templates" / "builtin-test"
        dsivio.atomic_json(folder / "template.json", {"name": "原模板", "mode": "smart", "slots": [{"id": "h1"}]})
        dsivio.atomic_json(folder / "record.json", {"id": "old"})
        (folder / "h1.png").write_bytes(b"reference")
        with contextlib.redirect_stdout(io.StringIO()):
            dsivio.main(["copy-template", "原模板", "副本"])
        copy = self.root / "templates" / "副本"
        self.assertEqual((copy / "h1.png").read_bytes(), b"reference")
        self.assertFalse((copy / "record.json").exists())
        self.assertEqual(dsivio.read_json(copy / "template.json")["name"], "副本")

    def test_shared_config_uses_selected_key_and_protocol_without_writing_secrets(self):
        settings = self.root / "app-settings.json"
        dsivio.atomic_json(settings, {"settings": {"providers": [{"id": "p", "baseUrl": "https://example.invalid/v1", "apiKeys": ["test-first", "test-selected"], "activeKeyIndex": 1}]}})
        dsivio.atomic_json(self.root / "runtime.json", {"settingsFile": str(settings)})
        dsivio.atomic_json(self.root / "config.json", {"providerId": "p", "model": "shared-model", "protocol": "grok"})
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            gen_image.load_env_file(None)
        self.assertEqual(os.environ["IMG_API_KEY"], "test-selected")
        self.assertEqual(os.environ["IMG_MODEL"], "shared-model")
        self.assertEqual(os.environ["IMG_API_MODE"], "grok")
        self.assertEqual(output.getvalue(), "")
        self.assertFalse(list(self.root.rglob(".env")))

    def test_setup_and_update_do_not_replace_bundled_skill_or_write_config(self):
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(dsimage.main(["setup", "env"]), 0)
            self.assertEqual(dsimage.main(["update"]), 0)
        self.assertFalse(list(self.root.iterdir()))


if __name__ == "__main__":
    unittest.main()
