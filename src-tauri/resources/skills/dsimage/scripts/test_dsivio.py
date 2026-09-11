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
