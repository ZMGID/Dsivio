"""Exercise the bundled MCP SDK error boundary, without calling ComfyUI."""
import sys
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'src-tauri/resources/video-runtime/python-packages'))
sys.path.insert(0, str(ROOT / 'src-tauri/resources/plugins/dsvideo-plugin/scripts'))
from comfy_errors import expose_expected_errors
from comfy_mcp.errors import ComfyCliError
from mcp.server.mcpserver import MCPServer
from mcp.server.mcpserver.exceptions import ToolError, UnexpectedToolError


class ErrorReportingTests(unittest.IsolatedAsyncioTestCase):
    async def test_expected_cli_error_survives_sdk_boundary_and_redacts_credentials(self):
        server = MCPServer('test')
        @server.tool()
        def upload_file(path: str) -> str:
            raise ComfyCliError('Cannot reach http://user:secret@host:8188/upload?token=hidden Bearer abc test-key')
        expose_expected_errors(server)
        with patch.dict('os.environ', {'COMFY_API_KEY': 'test-key'}):
            with self.assertRaises(ToolError) as caught:
                await server.call_tool('upload_file', {'path': '/test.jpg'})
        detail = str(caught.exception)
        self.assertNotIsInstance(caught.exception, UnexpectedToolError)
        self.assertIn('host:8188', detail)
        self.assertIn('Cannot reach', detail)
        for secret in ('secret', 'hidden', 'abc', 'test-key'):
            self.assertNotIn(secret, detail)

    async def test_cli_probe_uses_the_same_endpoint_as_mcp_submission(self):
        import json
        import os
        import threading
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        from mcp import ClientSession, StdioServerParameters
        from mcp.client.stdio import stdio_client
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                data = json.dumps({'system': {'os': 'endpoint-test', 'python_version': '3.12', 'embedded_python': False}, 'devices': []}).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                self.wfile.write(data)
            def log_message(self, *args):
                pass
        http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=http.serve_forever, daemon=True).start()
        runtime = ROOT / 'src-tauri/resources/video-runtime'
        env = {**os.environ, 'DSVIDEO_RUNTIME_ROOT': str(runtime),
               'PYTHONPATH': str(runtime / 'python-packages'), 'PYTHONHOME': '',
               'COMFYUI_URL': f'http://127.0.0.1:{http.server_port}', 'COMFY_LOCAL_URL': 'http://127.0.0.1:1'}
        params = StdioServerParameters(command=sys.executable,
            args=['-B', str(ROOT / 'src-tauri/resources/plugins/dsvideo-plugin/scripts/mcp_comfy.py')], env=env)
        try:
            async with stdio_client(params) as (read, write):
                async with ClientSession(read, write) as session:
                    await session.initialize()
                    result = await session.call_tool('system_stats', {})
                    self.assertFalse(result.is_error, str(result))
                    self.assertIn('endpoint-test', result.model_dump_json())
        finally:
            http.shutdown()
            http.server_close()

    async def test_unexpected_crash_stays_masked_and_success_is_unchanged(self):
        server = MCPServer('test')
        @server.tool()
        def echo(value: str) -> str:
            return value
        @server.tool()
        def crash() -> str:
            raise RuntimeError('internal secret')
        expose_expected_errors(server)
        result = await server.call_tool('echo', {'value': 'ok'})
        self.assertIn('ok', str(result))
        with self.assertRaises(UnexpectedToolError) as caught:
            await server.call_tool('crash', {})
        self.assertNotIn('internal secret', str(caught.exception))

if __name__ == "__main__":
    unittest.main()
