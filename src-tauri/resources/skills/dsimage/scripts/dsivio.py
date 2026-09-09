"""Dsivio filesystem adaptation. No server, credentials export, or second template library."""
from __future__ import annotations

import json
import os
import shutil
import sys
import uuid
from pathlib import Path


def workspace() -> Path:
    override = os.environ.get("DSIVIO_IMAGE_STUDIO_DIR")
    if override:
        return Path(override).expanduser().resolve()
    if sys.platform == "win32":
        base = Path(os.environ["APPDATA"])
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    return base / "com.zmair.kivio" / "image-studio"


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def atomic_json(path: Path, data: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    try:
        temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def load_config() -> dict:
    """Read only the provider selected by Image Settings; secrets stay in memory."""
    root = workspace()
    try:
        config = read_json(root / "config.json")
        runtime = read_json(root / "runtime.json")
        settings = read_json(Path(runtime["settingsFile"]))["settings"]
    except (OSError, ValueError, KeyError):
        raise RuntimeError("请先启动 Dsivio，在「图片 → 图片设置」选择供应商和图片模型。") from None
    provider = next((p for p in settings.get("providers", []) if p["id"] == config.get("providerId")), None)
    if not provider or not provider.get("enabled", True) or not config.get("model"):
        raise RuntimeError("图片供应商或模型未配置，请在 Dsivio 的图片设置中选择。")
    keys = provider.get("apiKeys") or [provider.get("apiKey", "")]
    index = provider.get("activeKeyIndex", 0)
    key = keys[index if 0 <= index < len(keys) else 0]
    if not key:
        raise RuntimeError("该供应商没有可用于 Skill 的 API key，请在应用供应商设置中配置。")
    mode = {"openai": "sync", "grok": "grok", "gemini": "gemini", "gemini-chat": "gemini-chat", "async": "async"}.get(config.get("protocol"))
    if not mode:
        raise RuntimeError("图片接口协议未配置，请检查图片设置。")
    os.environ.update(IMG_PROVIDER="custom", IMG_BASE_URL=provider["baseUrl"], IMG_API_KEY=key,
                      IMG_MODEL=config["model"], IMG_API_MODE=mode)
    return provider


def main(argv: list[str]) -> int:
    import core
    if argv == ["paths"]:
        print(json.dumps({"workspace": str(workspace()), "templates": str(core.TEMPLATES_DIR),
                          "tasks": str(workspace() / "tasks")}, ensure_ascii=False))
        return 0
    if len(argv) == 3 and argv[0] == "copy-template":
        source = core.find_template(argv[1])
        name = argv[2].strip()
        if not name or name in (".", "..") or any(c in name for c in '/\\:'):
            raise RuntimeError("模板名称不能含路径分隔符。")
        destination = core.TEMPLATES_DIR / name
        if destination.exists():
            raise RuntimeError("同名模板已存在，请使用新名称。")
        shutil.copytree(source, destination, ignore=shutil.ignore_patterns("record.json"))
        data = read_json(destination / "template.json")
        data["name"] = name
        atomic_json(destination / "template.json", data)
        print(destination)
        return 0
    if argv == ["tasks"]:
        for file in sorted((workspace() / "tasks").glob("*.json")):
            try:
                task = read_json(file)
                print(json.dumps({"id": task["id"], "name": task["brief"]["name"],
                                  "status": task["status"], "path": str(file),
                                  "outputDirectory": task.get("outputDirectory")}, ensure_ascii=False))
            except (OSError, ValueError, KeyError):
                continue
        return 0
    raise RuntimeError("用法：studio paths | studio copy-template <模板名或路径> <新名称> | studio tasks")
