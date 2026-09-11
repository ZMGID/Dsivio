"""Dsivio shared configuration and templates; generation stays in dsimage."""
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


def image_config():
    """Read the same saved model/provider as the image page, without IPC."""
    import json
    root = templates_dir().parent
    path = root / "config.json"
    if not path.is_file():
        return None
    config = json.loads(path.read_text(encoding="utf-8"))
    if not config.get("providerId"):
        return None
    settings = json.loads((root.parent / "settings.json").read_text(encoding="utf-8"))
    settings = settings.get("settings", settings)
    provider = next((p for p in settings.get("providers", [])
                     if p.get("id") == config["providerId"]), None)
    if not provider or not provider.get("enabled", True):
        raise ValueError("图片供应商不存在或已停用，请在图片页面重新选择。")
    keys = provider.get("apiKeys") or [provider.get("apiKey") or ""]
    index = min(max(int(provider.get("activeKeyIndex", 0)), 0), len(keys) - 1)
    key = keys[index].strip() or next((k.strip() for k in keys if k.strip()), "")
    model = config.get("model", "").strip()
    url = provider.get("baseUrl", "").strip().rstrip("/")
    if not key or not model or not url:
        raise ValueError("图片模型、供应商地址或密钥未配置完整，请在应用设置中补全。")
    values = {"IMG_API_KEY": key, "IMG_MODEL": model,
              "IMG_BASE_URL": url, "IMG_PROVIDER": "custom"}
    mode = config.get("protocol", "")
    if mode in ("sync", "async", "grok", "gemini", "gemini-chat"):
        values["IMG_API_MODE"] = mode
    return values


def use_image_config():
    # Explicit dsimage endpoint/credentials form a separate configuration.
    if any(os.environ.get(k) for k in ("IMG_API_KEY", "IMG_BASE_URL", "IMG_PROVIDER")):
        return False
    values = image_config()
    if values is None:
        return False
    for key, value in values.items():
        os.environ.setdefault(key, value)
    return True
