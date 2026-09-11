"""Run MCP SDK checks in its bundled Python ABI, isolated from system tests."""
from pathlib import Path
import subprocess
import unittest


class BundledComfyErrorsTests(unittest.TestCase):
    def test_bundled_mcp_error_boundary(self):
        root = Path(__file__).resolve().parents[1]
        runtime = root / 'src-tauri/resources/video-runtime'
        python = runtime / ('python/python.exe' if __import__('os').name == 'nt' else 'python/bin/python3')
        if not python.exists():
            self.skipTest('Bundled video runtime is not installed')
        result = subprocess.run([str(python), '-B', str(root / 'tests/runtime_checks/comfy_errors.py')], capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
