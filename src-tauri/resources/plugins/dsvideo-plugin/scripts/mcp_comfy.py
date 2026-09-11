"""Use the same ComfyUI address in chat and the video workspace."""
import os
import sys
from dsvideo_config import get_provider
from runtime import comfy_command

command = comfy_command()
if not command:
    print('内置 Comfy MCP 不完整，请重新安装 Dsivio', file=sys.stderr)
    sys.exit(1)
os.environ['COMFYUI_URL'] = os.environ.get('COMFYUI_URL') or get_provider('comfy').get('base_url') or 'http://192.168.1.171:8188'
# CLI probes (system_stats/free) use COMFY_LOCAL_URL rather than --host/--port.
os.environ['COMFY_LOCAL_URL'] = os.environ['COMFYUI_URL']
os.environ['COMFY_WHERE'] = 'local'
os.environ['COMFY_BIN'] = command
os.environ.setdefault('GIT_PYTHON_REFRESH', 'quiet')
from comfy_mcp.server import main, mcp
from comfy_errors import expose_expected_errors
expose_expected_errors(mcp)
main()
