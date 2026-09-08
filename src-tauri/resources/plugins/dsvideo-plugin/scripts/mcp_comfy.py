"""Use the same ComfyUI address in chat and the video workspace."""
import os
import shutil
import subprocess
import sys
from dsvideo_config import get_provider
from runtime import comfy_command, bin_dir

env = dict(os.environ)
env['COMFYUI_URL'] = os.environ.get('COMFYUI_URL') or get_provider('comfy').get('base_url') or 'http://127.0.0.1:8188'
env['PATH'] = str(bin_dir()) + os.pathsep + env.get('PATH', '')
env['COMFY_WHERE'] = 'local'
command = comfy_command()
if not command:
    print('缺少 comfy-mcp，请安装 comfy-mcp==0.10.0 和 comfy-cli>=1.14', file=sys.stderr)
    sys.exit(1)
sys.exit(subprocess.call([command], env=env))
