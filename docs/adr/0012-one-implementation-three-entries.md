# 每种能力一套实现、三个入口

**状态：接受（2026-10-02）。** 依据 [Workbench PRD](../prd/workbench-prd.md)。

## 背景

工作台 39 项功能需要 AI 写作分析、媒体生成与处理、电商平台、内容平台发布这几种能力。若每页各写提交、轮询、记录和平台调用，规则会散落在几十处。`run_ai_task`（ADR 0008）与 `media_generation` + `dsivio media`（ADR 0009）已证明「一种能力一套实现」可行。

## 决策

1. **一套实现**：每种能力的提交、回执、查询、恢复、防重复提交和任务状态只在 Rust 的一个模块里：AI → `chat/ai_task.rs`；媒体生成与本地处理 → `media_generation`；电商平台 → `workbench/commerce`（ADR 0011）；内容平台发布 → `workbench/publish`（ADR 0013）。
2. **三个入口只做转接**：
   - 工作台页面：Tauri 命令 → `src/api/tauri.ts` 的类型化方法 → 功能 hook。页面不直接 `invoke`。
   - 对话：Agent 工具（`mixer_*` 媒体工具、`mixer_process_video`、`commerce`、`publish`）。
   - 插件 / Skill / 外部 Agent：`dsivio <能力>` 命令行，经 `app_cli` 本机回环端口连到运行中的 App 执行；命令行进程不读密钥、不直连供应商或平台。
3. **入口范围**：工作台入口随每个功能必做。对话工具与命令行按能力做：本地处理、电商平台、内容发布两者都有；AI 只补 `dsivio ai`（对话本身就是 AI）。不要求每个功能页单独提供对话入口。
4. **统一结果分类**：成功 / 被拒（未扣费或未生效，可修改重试）/ 失败 / 结果不确定（只能查询，不能重交）/ 已取消。命令行退出码沿用 `dsivio media`：0 成功、1 内部错误、2 参数无效、3 被拒、4 失败、5 不确定、6 App 未运行、7 已取消、124 等待超时。
5. **传输共用**：`src-tauri/src/app_cli.rs` 持有回环服务、令牌鉴权与一行 JSON 协议，按 `service` 分发给各能力模块的 `handle`；连接信息仍写在 `<app_data>/run/media-cli.json`。

## 两种功能模板

生成类功能页只套用两种形状之一，不自创第三种流程：

- **一步**：填表 → 可选 `run_ai_task(once)` 帮写 → 一次提交到 `media_generation` → 结果进任务列表。样板：主图生成。
- **多步**：`run_ai_task(agent)` 出方案 → 用户确认/修改（随项目保存）→ 逐项提交媒体任务，部分失败只重做失败项。方案确认前不产生付费生成。样板：`ImageProjectWorkspace` / `VideoProjectWorkspace`；新的多步功能作为它们的新 feature / entry 接入。

## 后果

- 新能力先定 Rust 模块与统一类型，再接三个入口；页面只有表单、`build*Request` 纯函数和结果展示。
- 产物统一记在媒体任务里（含 `text` 记录），图片视频库只读这一个来源。
- 新增入口时若发现要复制提交/重试规则，说明规则放错了地方，回到能力模块修改。
