import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'src-tauri/resources/skills/dsimage/scripts/dsivio.py'
spec = importlib.util.spec_from_file_location('shared_image_config', SCRIPT)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class SharedConfigTests(unittest.TestCase):
    def test_reads_saved_changes_and_selected_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'image-studio').mkdir()
            config = root / 'image-studio/config.json'
            config.write_text(json.dumps({'providerId': 'p', 'model': 'first', 'protocol': 'async'}))
            settings = {'settings': {'providers': [{'id': 'p', 'baseUrl': 'https://example.test/v1',
                       'apiKeys': ['first-key', 'selected-key'], 'activeKeyIndex': 1}]}}
            (root / 'settings.json').write_text(json.dumps(settings))
            with patch.object(bridge, 'templates_dir', return_value=root / 'image-studio/templates'):
                values = bridge.image_config()
                self.assertEqual(values['IMG_API_KEY'], 'selected-key')
                self.assertEqual(values['IMG_API_MODE'], 'async')
                config.write_text(json.dumps({'providerId': 'p', 'model': 'updated'}))
                self.assertEqual(bridge.image_config()['IMG_MODEL'], 'updated')
                settings['settings']['providers'][0]['enabled'] = False
                (root / 'settings.json').write_text(json.dumps(settings))
                with self.assertRaisesRegex(ValueError, '停用'):
                    bridge.image_config()

    def test_explicit_credentials_do_not_mix_with_app(self):
        with patch.dict(os.environ, {'IMG_API_KEY': 'override'}, clear=True), patch.object(bridge, 'image_config') as read:
            self.assertFalse(bridge.use_image_config())
            read.assert_not_called()

    def test_default_preserves_explicit_model_override(self):
        with patch.dict(os.environ, {'IMG_MODEL': 'override'}, clear=True), patch.object(bridge, 'image_config', return_value={'IMG_MODEL': 'saved', 'IMG_API_KEY': 'key'}):
            self.assertTrue(bridge.use_image_config())
            self.assertEqual(os.environ['IMG_MODEL'], 'override')
            self.assertEqual(os.environ['IMG_API_KEY'], 'key')

if __name__ == '__main__':
    unittest.main()
