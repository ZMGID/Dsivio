---
name: jianying-editor-setup
description: Check Dsivio integration and the Jianying editor Skill runtime before the first draft. Also load it when this integration fails, a dependency or authorization is missing, or Dsivio adaptation needs checking.
kivio-market-managed: true
---

# 设置剪映剪辑

市场把 [luoluoluo22/jianying-editor-skill](https://github.com/luoluoluo22/jianying-editor-skill) 装到 `~/.kivio/skills/jianying-editor`。技能名是 `jianying-editor`。首次使用或修复时先检查接入，再按官方 `rules/setup.md` 和 `requirements.txt` 补齐环境。不要把技能复制到 `~/.agents/skills`。本插件没有 MCP 组件。剪辑脚本写在用户项目里，不要写进技能目录。

1. 确认当前对话能加载 `jianying-editor` 和 `jianying-editor-setup`。确认 `~/.kivio/skills/jianying-editor/SKILL.md`、`requirements.txt`、`scripts/jy_wrapper.py` 可读。缺的是市场安装的文件时，用市场的安装或修复入口，不要手抄。
2. 依赖装在技能专用的虚拟环境 `~/.kivio/jianying-editor/venv`，由 Dsivio 内置 Python 3.12 创建，不需要系统 Python，也不装进系统或 Dsivio 自身的 Python。该环境的解释器：Windows 是 `~/.kivio/jianying-editor/venv/Scripts/python.exe`，macOS 是 `~/.kivio/jianying-editor/venv/bin/python`，下文记作 `<venv-python>`。环境不存在时运行 `dsivio python -m venv "$HOME/.kivio/jianying-editor/venv"`。缺依赖时先展示 `<venv-python> -m pip install -r requirements.txt`（在技能目录执行）并取得同意，再调用 shell 工具按其契约传 `allow_host_python_package_install: true` 安装。然后用 `<venv-python> -c "import sys; sys.path.insert(0, 'scripts'); import jy_wrapper"` 确认包装可以导入；之后运行本技能的 Python 脚本都用 `<venv-python>`。只有 `dsivio python` 报告内置运行时缺失（退出码 127）时，才用系统 Python 3 按同样步骤建环境。不要猜测剪映草稿目录；按 `rules/setup.md` 探测，或让用户用 `JY_SKILL_ROOT` 指向这个技能目录。
3. 分别报告组件是否就位，以及导入是否成功。市场“已安装”只表示文件就位。

接入和导入检查都成功后，只把已安装的 `~/.kivio/skills/jianying-editor-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`，其余内容不变。未完成不要标记。用户要求重跑或依赖变化时可以再跑，不重复追加。
