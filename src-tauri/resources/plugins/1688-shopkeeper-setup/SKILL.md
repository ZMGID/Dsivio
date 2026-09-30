---
name: 1688-shopkeeper-setup
description: Check Dsivio's 1688 AI 店长 installation, Python runtime, AK, and store binding before use. Also load it when this integration fails, a dependency or authorization is missing, or Dsivio adaptation needs checking.
kivio-market-managed: true
---

# 设置 1688 AI 店长

市场从 [next-1688/1688-shopkeeper](https://github.com/next-1688/1688-shopkeeper) 的固定提交安装技能到 `~/.kivio/skills/1688-shopkeeper`。首次使用或修复时检查：

1. 确认当前对话能加载 `1688-shopkeeper` 和 `1688-shopkeeper-setup`，且技能目录中有 `SKILL.md`、`cli.py`、`requirements.txt`。缺少市场安装的文件时使用市场的安装或修复入口。
2. 检查 `python3 --version` 和 `python3 -c 'import requests; print(requests.__version__)'`。缺少 requests 时，按 `requirements.txt` 在该 Python 环境安装依赖；不要自动安装到全局环境或重复安装。
3. `cli.py` 从运行 Dsivio 的进程环境变量 `ALI_1688_AK` 读取密钥。让用户在 1688 AI版 APP 获取 AK，并在本机安全地配置该环境变量后重启 Dsivio；不要要求用户把 AK 发到聊天中，也不要在聊天、日志或命令输出里回显它。上游 `configure` 会写入 `~/.openclaw/openclaw.json`，在 Dsivio 中不要调用此命令，除非用户明确要求改 OpenClaw 配置。
4. 在与 Dsivio 相同的环境里执行 `python3 ~/.kivio/skills/1688-shopkeeper/cli.py check`。它会检查 AK 并查询已绑定的店铺；只报告脱敏状态。市场“已安装”表示技能文件就位，不代表 AK 或店铺授权已就绪。不要把现有 Workbench 的 1688 找货配置视作本技能的 AK。

执行选品、商机、趋势和日报时遵循上游 `SKILL.md` 与相应 `references/capabilities/` 文档。`publish` 会实际向下游店铺铺货：先按上游文档执行 `--dry-run`，核对商品与唯一目标店铺，再按用户已授权的目标执行。不要把试运行当成已经铺货。

Dsivio 接入、Python 依赖和 `check` 都成功后，只把已安装的 `~/.kivio/skills/1688-shopkeeper-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`，其余内容不变。未完成不要标记。
