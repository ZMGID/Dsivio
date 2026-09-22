"""Application-bundled dependencies. No installer or system fallback."""
import importlib.util
import os
from pathlib import Path
import sys


def runtime_dir():
    value = os.environ.get('DSVIDEO_RUNTIME_ROOT')
    return Path(value) if value else None


def bin_dir():
    root = runtime_dir()
    return root / 'bin' if root else None


def bundled_file(relative):
    root = runtime_dir()
    path = root / relative if root else None
    return path if path and path.is_file() else None


def comfy_command():
    command = bundled_file('bin/comfy.exe' if os.name == 'nt' else 'bin/comfy')
    return str(command) if command and importlib.util.find_spec('comfy_mcp') else None


def status():
    return {
        'python': sys.version.split()[0],
        'comfy': bool(comfy_command()),
        'node': bool(bundled_file('node/node.exe' if os.name == 'nt' else 'node/bin/node')),
        'ffmpeg': bool(bundled_file('analyzer/node_modules/ffmpeg-static/ffmpeg.exe' if os.name == 'nt' else 'analyzer/node_modules/ffmpeg-static/ffmpeg')),
        'analyzer': bool(bundled_file('analyzer/node_modules/mcp-video-analyzer/dist/index.js')),
        'bundled': bool(bundled_file('runtime.json')),
    }


def install():
    # Compatibility for older UI callers: only verify, never download or mutate.
    current = status()
    if not all(current[key] for key in ('bundled', 'comfy', 'node', 'ffmpeg', 'analyzer')):
        raise ValueError('内置视频运行环境不完整，请重新安装 Dsivio')
    return {'installed': True}
