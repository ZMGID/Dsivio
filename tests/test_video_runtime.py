"""The desktop runtime must not install dependencies or use system tools."""
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src-tauri/resources/plugins/dsvideo-plugin/scripts'))
import runtime


class BundledRuntimeTests(unittest.TestCase):
    def test_missing_bundle_is_not_satisfied_by_system_tools(self):
        with patch.dict(os.environ, {'DSVIDEO_RUNTIME_ROOT': ''}), patch('shutil.which', return_value='/system/comfy-mcp') as which:
            self.assertIsNone(runtime.comfy_command())
            self.assertFalse(runtime.status()['bundled'])
            with self.assertRaisesRegex(ValueError, '重新安装 Dsivio'):
                runtime.install()
            which.assert_not_called()

    def test_legacy_install_only_checks_local_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            files = ['runtime.json', 'analyzer/node_modules/mcp-video-analyzer/dist/index.js']
            files += ['bin/comfy.exe', 'node/node.exe', 'analyzer/node_modules/ffmpeg-static/ffmpeg.exe'] if os.name == 'nt' else ['bin/comfy', 'node/bin/node', 'analyzer/node_modules/ffmpeg-static/ffmpeg']
            for file in files:
                path = root / file
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('fixture')
            with patch.dict(os.environ, {'DSVIDEO_RUNTIME_ROOT': directory}), patch('importlib.util.find_spec', return_value=object()), patch('subprocess.run') as run:
                self.assertEqual(runtime.install(), {'installed': True})
                run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
