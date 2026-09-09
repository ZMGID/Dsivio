"""Contract between desktop settings and the bundled dsimage client."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src-tauri/resources/skills/dsimage/scripts'))
import dsivio


class ImageContracts(unittest.TestCase):
    def test_desktop_and_plugin_use_same_api_root_and_selected_key(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'config.json').write_text(json.dumps({'providerId': 'p', 'model': 'image', 'protocol': 'openai'}))
            (root / 'runtime.json').write_text(json.dumps({'settingsFile': str(root / 'settings.json')}))
            for base, expected in [('https://example.test', 'https://example.test/v1'),
                                   ('https://example.test/v1/', 'https://example.test/v1'),
                                   ('https://example.test/v1beta', 'https://example.test/v1beta'),
                                   ('https://example.test/proxy/v1?unused=1', 'https://example.test/proxy/v1')]:
                with self.subTest(base=base), patch.dict(os.environ, {'DSIVIO_IMAGE_STUDIO_DIR': directory}):
                    (root / 'settings.json').write_text(json.dumps({'settings': {'providers': [
                        {'id': 'p', 'baseUrl': base, 'apiKeys': ['first', 'selected'], 'activeKeyIndex': 1}
                    ]}}))
                    dsivio.load_config()
                    self.assertEqual(os.environ['IMG_BASE_URL'], expected)
                    self.assertEqual(os.environ['IMG_API_KEY'], 'selected')
                    self.assertEqual(os.environ['IMG_API_MODE'], 'sync')
