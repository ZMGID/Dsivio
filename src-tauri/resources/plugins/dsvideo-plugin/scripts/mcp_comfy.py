"""Use the same ComfyUI address in chat and the video workspace."""
import os
import sys
from dsvideo_config import get_provider
from runtime import comfy_command

command = comfy_command()
if not command:
    print('内置 Comfy MCP 不完整，请重新安装 Dsivio', file=sys.stderr)
    sys.exit(1)
os.environ['COMFYUI_URL'] = os.environ.get('COMFYUI_URL') or get_provider('comfy').get('base_url') or 'http://127.0.0.1:8188'
os.environ['COMFY_WHERE'] = 'local'
os.environ['COMFY_BIN'] = command
os.environ.setdefault('GIT_PYTHON_REFRESH', 'quiet')
from comfy_mcp.server import main
main()
