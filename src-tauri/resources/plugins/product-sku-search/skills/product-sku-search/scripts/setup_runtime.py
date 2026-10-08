"""Prepare one private Python environment; no system-wide installs or disk searches."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

PROBE = 'import json,openpyxl,PIL,lxml; print(json.dumps(dict(openpyxl=openpyxl.__version__,Pillow=PIL.__version__,lxml=lxml.__version__)))'


def probe(python):
    if not python.is_file():
        return None
    result = subprocess.run([str(python), '-c', PROBE], capture_output=True, text=True, timeout=15)
    return json.loads(result.stdout) if result.returncode == 0 else None


def setup(root, check=False):
    started = time.perf_counter()
    python = root/'venv'/('Scripts/python.exe' if os.name == 'nt' else 'bin/python')
    versions = probe(python)
    if versions is None and check:
        return dict(ready=False, python=str(python))
    if versions is None:
        root.mkdir(parents=True, exist_ok=True)
        if not python.is_file():
            subprocess.run([sys.executable, '-m', 'venv', str(root/'venv')], check=True, timeout=60)
        subprocess.run([str(python), '-m', 'pip', 'install', '--disable-pip-version-check', '--only-binary=:all:',
                        '--timeout', '20', '--retries', '1', '-r', str(Path(__file__).with_name('requirements.txt'))],
                       check=True, timeout=180, stdout=sys.stderr)
        versions = probe(python)
        if versions is None:
            raise RuntimeError('Runtime dependency check failed')
    result = dict(ready=True, python=str(python), versions=versions, seconds=time.perf_counter()-started)
    if not check:
        root.mkdir(parents=True, exist_ok=True)
        fd, name = tempfile.mkstemp(dir=root, suffix='.json')
        try:
            with os.fdopen(fd, 'w', encoding='utf-8') as stream:
                json.dump(result, stream, indent=2)
            os.replace(name, root/'runtime.json')
        finally:
            if os.path.exists(name):
                os.unlink(name)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path.home()/'.kivio/shopee-research/runtime')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    try:
        print(json.dumps(setup(args.root.resolve(), args.check)))
    except Exception as error:
        parser.exit(1, f'Runtime setup failed: {error}\n')
