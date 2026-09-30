---
name: daihuo-fanpai-setup
description: Check Dsivio integration and run the daihuo-fanpai doctor before the first commerce-video remake. Also load it when this integration fails, a dependency or authorization is missing, or Dsivio adaptation needs checking.
kivio-market-managed: true
---

# 设置带货仿拍

市场把 [wangcanyu/daihuo-fanpai](https://github.com/wangcanyu/daihuo-fanpai) 的技能装到 `~/.kivio/skills/daihuo-fanpai`。首次使用或修复时先检查接入，再按官方说明补齐依赖。不要把技能复制到 `~/.agents/skills`。本插件没有 MCP 组件。

1. 确认当前对话能加载 `daihuo-fanpai` 和 `daihuo-fanpai-setup`。确认 `~/.kivio/skills/daihuo-fanpai/SKILL.md`、`doctor.py`、`route.py`、`deliver.py` 可读。缺的是市场安装的文件时，用市场的安装或修复入口，不要手抄。
2. 在 Dsivio 对话实际使用的环境里，进入该技能目录，运行 `python3 doctor.py`。按脚本的分级处理：ffmpeg 可以按官方说明安装；即梦 CLI 和 Seed key 只提示用户自己登录或提供，不要反复试凭据；CosyVoice 这类重型依赖不默认安装，把官方降级选项交给用户选。
3. 分别报告组件是否就位，以及体检结果。市场“已安装”只表示文件就位。

Dsivio 接入和 `python3 doctor.py` 都成功后，只把已安装的 `~/.kivio/skills/daihuo-fanpai-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`，其余内容不变。未完成不要标记。用户要求重跑或依赖变化时可以再跑，不重复追加。
