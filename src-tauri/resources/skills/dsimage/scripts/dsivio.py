"""Dsivio template directory only. Generation and configuration belong to dsimage."""
import os
import sys
from pathlib import Path


def templates_dir():
    override = os.environ.get("DSIVIO_IMAGE_STUDIO_DIR")
    if override:
        return Path(override).expanduser().resolve() / "templates"
    if sys.platform == "win32":
        base = Path(os.environ["APPDATA"])
    elif sys.platform == "darwin":
        base = Path.home() / "Library/Application Support"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    return base / "com.zmair.kivio/image-studio/templates"
