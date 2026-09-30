---
name: jianying-editor-setup
description: Check Dsivio integration and the Jianying editor Skill runtime before the first draft. Also load it when this integration fails, a dependency or authorization is missing, or Dsivio adaptation needs checking.
kivio-market-managed: true
---

# 设置剪映剪辑

市场把 [luoluoluo22/jianying-editor-skill](https://github.com/luoluoluo22/jianying-editor-skill) 装到 `~/.kivio/skills/jianying-editor`。技能名是 `jianying-editor`。首次使用或修复时先检查接入，再按官方 `rules/setup.md` 和 `requirements.txt` 补齐环境。不要把技能复制到 `~/.agents/skills`。本插件没有 MCP 组件。剪辑脚本写在用户项目里，不要写进技能目录。

1. 确认当前对话能加载 `jianying-editor` 和 `jianying-editor-setup`。确认 `~/.kivio/skills/jianying-editor/SKILL.md`、`requirements.txt`、`scripts/jy_wrapper.py` 可读。缺的是市场安装的文件时，用市场的安装或修复入口，不要手抄。
2. 在 Dsivio 对话实际使用的环境里检查依赖。缺少时先展示 `pip install -r requirements.txt`（在技能目录执行）并取得同意，再安装。然后用 `python3 -c "import sys; sys.path.insert(0, 'scripts'); import jy_wrapper"` 确认包装可以导入。不要猜测剪映草稿目录；按 `rules/setup.md` 探测，或让用户用 `JY_SKILL_ROOT` 指向这个技能目录。
3. 分别报告组件是否就位，以及导入是否成功。市场“已安装”只表示文件就位。

接入和导入检查都成功后，只把已安装的 `~/.kivio/skills/jianying-editor-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`，其余内容不变。未完成不要标记。用户要求重跑或依赖变化时可以再跑，不重复追加。
