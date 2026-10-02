#!/usr/bin/env python3
"""套图 Schema 与 Rust `validate_template` 读同一份 `docs/schemas`。

校验器用视频运行时里已经带上的 jsonschema 4.26（Draft 2020-12），
包在 `src-tauri/resources/video-runtime/python-packages`。不另拷一份，
也不手写半套校验。那里的 `rpds` 是 CPython 3.12 扩展，系统 Python 加载不了，
所以当前解释器如果不是视频运行时的 `python3`，就改用它重新执行本文件。

跑：`src-tauri/resources/video-runtime/python/bin/python3 scripts/test_image_set_schema.py`
（在 dsimage 技能目录下），或直接用系统 python 调用本文件，它会自己换解释器。
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[5]
BUNDLED_PYTHON = REPO / "src-tauri/resources/video-runtime/python/bin/python3"
BUNDLED_PACKAGES = REPO / "src-tauri/resources/video-runtime/python-packages"


def _ensure_bundled_runtime() -> None:
    if not BUNDLED_PYTHON.is_file():
        raise SystemExit(f"缺少视频运行时 Python：{BUNDLED_PYTHON}")
    if Path(sys.executable).resolve() != BUNDLED_PYTHON.resolve():
        os.execv(str(BUNDLED_PYTHON), [str(BUNDLED_PYTHON), *sys.argv])
    if str(BUNDLED_PACKAGES) not in sys.path:
        sys.path.insert(0, str(BUNDLED_PACKAGES))


_ensure_bundled_runtime()

import json  # noqa: E402
import unittest  # noqa: E402

import jsonschema  # noqa: E402

TEMPLATE_SCHEMA = REPO / "docs/schemas/image-set-template.v1.schema.json"
CLIENT_SCHEMA = REPO / "docs/schemas/image-set-client.v1.schema.json"
EXAMPLE = REPO / "docs/schemas/examples/image-set-v1"
SKILL_TEMPLATES = REPO / "src-tauri/resources/skills/dsimage/templates"
STUDIO = REPO / "src-tauri/resources/image-studio"


def _load(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def _collect(root: Path, name: str) -> list[Path]:
    return sorted(path for path in root.rglob(name) if path.is_file())


class ImageSetSchemaTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.template = jsonschema.Draft202012Validator(_load(TEMPLATE_SCHEMA))
        cls.client = jsonschema.Draft202012Validator(_load(CLIENT_SCHEMA))
        cls.template.check_schema(cls.template.schema)
        cls.client.check_schema(cls.client.schema)

    def _assert_valid(self, validator: jsonschema.Draft202012Validator, path: Path) -> None:
        errors = sorted(validator.iter_errors(_load(path)), key=lambda error: list(error.absolute_path))
        self.assertEqual(errors, [], f"{path}: {errors[0].message if errors else ''}")

    def test_shared_example_builtins_and_skill_templates(self) -> None:
        templates = _collect(EXAMPLE, "template.json") + _collect(SKILL_TEMPLATES, "template.json")
        templates += [
            STUDIO / "default.json",
            STUDIO / "kids.json",
            STUDIO / "mens-backpack/template.json",
            STUDIO / "womens-backpack/template.json",
        ]
        self.assertGreater(len(templates), 4)
        for path in templates:
            self._assert_valid(self.template, path)
        clients = _collect(EXAMPLE, "要求.json") + _collect(SKILL_TEMPLATES, "要求.json")
        self.assertTrue(clients)
        for path in clients:
            self._assert_valid(self.client, path)

    def test_replace_slot_without_prompt_is_rejected(self) -> None:
        document = {
            "name": "x",
            "mode": "replace",
            "slots": [{"id": "h1", "example": "h1.png"}],
        }
        with self.assertRaises(jsonschema.ValidationError):
            self.template.validate(document)


if __name__ == "__main__":
    unittest.main()
