"""Managed Comfy MCP dependencies, isolated from the user's Python packages."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import venv


def runtime_dir():
    base = Path(os.environ.get('APPDATA') or (Path.home() / 'Library/Application Support' if sys.platform == 'darwin' else os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share'))
    return base / 'com.zmair.kivio' / 'video-studio' / 'runtime'


def bin_dir():
    return runtime_dir() / ('Scripts' if os.name == 'nt' else 'bin')


def comfy_command():
    managed = bin_dir() / ('comfy-mcp.exe' if os.name == 'nt' else 'comfy-mcp')
    return str(managed) if managed.is_file() else shutil.which('comfy-mcp')


def install():
    root = runtime_dir()
    venv.EnvBuilder(with_pip=True).create(root)
    python = bin_dir() / ('python.exe' if os.name == 'nt' else 'python')
    result = subprocess.run([str(python), '-m', 'pip', 'install', '--disable-pip-version-check',
                             'comfy-mcp==0.10.0', 'comfy-cli==1.15.0'],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=540)
    if result.returncode:
        raise ValueError('依赖安装失败，请检查 Python 版本和软件源网络后重试')
    return {'installed': bool(comfy_command())}
