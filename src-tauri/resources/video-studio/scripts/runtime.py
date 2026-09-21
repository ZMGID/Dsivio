"""Detect host dependencies; installation remains an explicit user action."""
import shutil
import sys

def comfy_command():
    return shutil.which('comfy')

def status():
    return {'python': sys.version.split()[0], 'comfy': bool(comfy_command()),
            'node': bool(shutil.which('node')), 'ffmpeg': bool(shutil.which('ffmpeg')),
            'analyzer': bool(shutil.which('mcp-video-analyzer')), 'bundled': False}

def install():
    if not comfy_command():
        raise ValueError('请先在本机安装 comfy-mcp，并确保 comfy 命令可用')
    return {'installed': True}
