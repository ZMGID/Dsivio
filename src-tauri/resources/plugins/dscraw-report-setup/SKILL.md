---
name: dscraw-report-setup
description: Prepare the bundled Ziniao daily-report Skill, select the user's stores and data directory, and set up daily execution in Dsivio. Also use when dependencies or the first configuration are missing.
kivio-market-managed: true
---

# 设置紫鸟店铺日报

日报 Skill、采集脚本、模板和空白台账随 Dsivio 打包，市场安装到 `~/.kivio/skills/dscraw-report`。复用已有环境、用户配置和任务，不安装 Codex，不复制原作者的店铺或调度任务。

1. 加载 `dscraw-report`，读取它的 `references/migration.md`，检查并只补齐缺失依赖。紫鸟认证由用户在自己的本机客户端完成，不读取凭据。
2. 按 `references/daily-workflow.md` 的首次配置流程，读取可见店铺，让用户选择哪些店做日报、数据目录、模板及每天执行的时间。模板选择和默认组合见该工作流。用户已给出的选择直接采用；已登记配置直接复用。
3. 将配置存入用户选择的数据目录，用 `--config` 执行 plan，随后按原 Skill 完成首轮采集、归档、台账、渲染和审计。
4. 用户要求每日自动运行并已给出时间时，首轮跑通后直接调用 Dsivio 的 `schedule_list` / `schedule_create` / `schedule_update`，按工作流保存包含配置绝对路径的定时提示词，不要求用户再确认同一事项。

环境与配置检查通过后，只把已安装的 `~/.kivio/skills/dscraw-report-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`。这只记录首次设置已完成；实店采集是否完成仍以日报 audit 和缺项为准。
