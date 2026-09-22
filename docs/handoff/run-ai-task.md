# 交接：`run_ai_task` —— 工作台的一次性 AI 调用入口

给实现者的完整说明。读完本文和 [统一工程规范](../engineering-standards.md) 即可开工，不需要再问背景。涉及工作台页面的部分遵守 [ADR 0007](../adr/0007-workbench-features-come-from-one-registry.md)。

## 1. 要解决什么

工作台（`src/chat/workbench/`）里很多页面需要 AI 帮忙：按产品图写一条生图提示词、润色文案、写视频脚本、分析参考视频再出方案、生成一篇图文并落盘。现在后端有两条能力，但没有一个工作台能调的入口：

- **Agent 循环** `run_agent_loop`（`src-tauri/src/chat/agent/loop_.rs:229`）。所有调用方都在聊天侧，会写侧栏会话。唯一"不进侧栏"的用法是自动化的 `run_builtin_agent_node`（`src-tauri/src/automation/application.rs:387`），但它绑死了 `automation_id / run_id`。
- **一问一答** `call_chat_completion_message_streamed`（`src-tauri/src/chat/agent/planning.rs`）。标题、提问优化、压缩、vision、视频分析、advisor 六处各自抄了一遍"取模型 → 查凭证 → 拒绝生图模型 → 组消息 → 调用"。

**目标**：一个 Tauri 命令 `run_ai_task`，同一条代码路径，用 `mode` 区分"一步"和"多步"，两种都可以带工具，都不写侧栏会话。

## 2. 契约（先定死，前后端照此实现）

```ts
// src/generated/aiTask.ts —— 由 ts-rs 从 Rust 导出，不手写
export type AiTaskMode = 'once' | 'agent'
export type AiTaskSlot = 'chat' | 'vision' | 'promptOptimize'

export type AiTaskRequest = {
  taskId: string            // 前端生成的 uuid；会话 id = `wb_${taskId}`，取消也靠它
  mode: AiTaskMode
  system: string | null     // 调用方自己的系统提示；见 §3.4 与工具说明的拼接规则
  prompt: string
  images: string[]          // data URL（`data:image/png;base64,...`），最多 4 张
  tools: string[]           // 工具白名单，空 = 无工具。匹配规则复用 chat/agent/filter.rs::entry_matches
  slot: AiTaskSlot          // 从 settings.default_models 取模型；带图且 slot='chat' 时自动改用 vision
  providerId: string | null // 与 model 同时给才生效，只给一个报错（照 resolve_kivio_model）
  model: string | null
  cwd: string | null        // 有文件类工具时的工作目录；null = settings.chat_tools.native_tools.working_directory
  timeoutSecs: number | null// null → once 60s / agent 600s
  stream: boolean           // true 时按 taskId 发 `ai-task-delta` 事件
}

export type AiTaskResult = {
  text: string
  toolCalls: AiTaskToolCall[]   // { name: string; status: string }，从 ToolCallRecord 压缩而来
  usage: AiTaskUsage | null     // { inputTokens: number; outputTokens: number }
}

// 事件 `ai-task-delta`：{ taskId: string; delta: string }
```

Rust 侧字段 snake_case，`#[serde(rename_all = "camelCase")]` + `#[derive(TS)]`，与 `media_generation.rs` 的 `MediaRequest` 同一套写法。

两种 `mode` 的差别只有三件事，其余全部共用：

| | `once` | `agent` |
|---|---|---|
| `effective_chat_tools.max_tool_rounds` | `Some(1)` | 取设置值（默认不限） |
| 默认超时 | 60s | 600s |
| 典型用途 | 写提示词、润色、单次带图分析 | 写图文并落盘、多步分析 |

## 3. 后端实现

### 3.1 文件

- 新建 `src-tauri/src/chat/ai_task.rs`：请求/结果类型、`run_ai_task`、`cancel_ai_task` 两个命令、内部 `run()`。
- 新建 `src-tauri/src/chat/agent/headless_host.rs`：把 `automation/application.rs` 里的 `WorkflowAgentHost`（173–254 行）和 `WorkflowToolExecutor`（256–300 行）**搬**过来改名 `HeadlessAgentHost` / `HeadlessToolExecutor`，加一个可选的流事件配置。自动化改为引用这里的实现，原地删除。不允许复制一份并存（规范 §4）。
- `src-tauri/src/lib.rs` 注册两个命令（在 `chat::commands::prompt_optimize::chat_optimize_prompt` 附近，约 661 行）。
- `src-tauri/src/bin/export_chat_protocol.rs` 追加导出到 `src/generated/aiTask.ts`（照 296–299 行 `generation_types` 的写法）。

### 3.2 `HeadlessAgentHost`

```rust
pub(crate) struct HeadlessAgentHost {
    app: AppHandle,
    text: Mutex<String>,
    /// Some((event_name, task_id)) 时每个 delta 额外 emit 一次
    stream: Option<(String, String)>,
}
```

行为与现有 `WorkflowAgentHost` 完全一致：累积 delta、工具审批和会话 consent 一律 `true`、`request_user_response` 返回 `phase: "cancelled"`、`is_generation_active` 查 `chat_runtime()`。只在 `emit_stream_delta` 里多一步：`stream` 为 `Some` 就 `app.emit(event, json!({ "taskId", "delta" }))`。

### 3.3 `run()` 步骤（照 `run_builtin_agent_node` 387–563 行的顺序）

1. **模型**：`providerId/model` 同时给用之；否则按 `slot`：`chat` → `settings.effective_chat_model()`；`vision` → `effective_vision_model()`；`promptOptimize` → `effective_prompt_optimize_model_for_session(None)`。`images` 非空且 slot 为 `chat` 时改走 vision。然后 `get_provider` + `enabled && has_credentials()`，再 `model_can_generate_images_directly` 为真则报错（照 `prompt_optimize.rs` 176–196 行，错误文案用 `resolve_chat_language` 分中英）。
2. **会话**：`conversation_id = format!("wb_{task_id}")`。`chat_interactions().grant_session_consent(&id)`、`chat_runtime().begin_generation(&id)`。结束时无论成败 `end_generation`。**不**调用 `repository`，不创建会话。
3. **工具**：`tools` 为空 → `Vec::new()`，跳过目录加载。否则 `mcp::registry::list_enabled_tool_catalog` → 按白名单 `retain`（`filter::entry_matches`）→ 去掉 memory_* / automation_* / 子代理工具（把 `application.rs` 的 `is_workflow_forbidden_tool` 一起搬到 `headless_host.rs`，两边共用）。白名单非空但结果为空 → 报错"所选工具当前不可用"。
4. **系统提示**：
   - `tools` 为空：`runtime_messages[0]` 就是调用方的 `system`（为 `None` 时用一句固定的中/英"你是工作台助手，只输出结果，不要解释"）。不调 `build_chat_system_prompt`，不扫 skills。
   - `tools` 非空：`build_chat_system_prompt(...)`（参数照 464–487 行，`registry` 用 `skills::build_registry_in` 的结果，`active_skill_*` 传 `None`），然后把调用方的 `system` 追加在末尾 `"\n\n"` 分隔。
5. **用户消息**：无图 `{ role: "user", content: prompt }`；有图用内容块数组：`[{type:"text", text}, {type:"image_url", image_url:{url: dataUrl}}...]`（`vision.rs:747` 同款；provider 适配由 `chat/model/` 负责，这里只出 OpenAI 形状）。
6. **配置**：`AgentRunConfig` 照 516–542 行填。差异：`effective_chat_tools.approval_policy = "auto"`；`max_tool_rounds` 按 `mode`；`skill_project_cwd` = `cwd` 或设置默认；`run_id = format!("wb-run-{task_id}")`、`message_id = format!("wb-msg-{task_id}")`。
7. **执行**：`tokio::time::timeout(dur, run_agent_loop(config, &host, &executor))`。超时 → `chat_runtime().cancel_conversation(&conversation_id)`，返回错误"AI 任务超时"。`Err("cancelled")` 原样透传。
8. **结果**：`text` = `result.content`，空则用 `host.text`；两者都空报错。`toolCalls` 从 `result.tool_records` 映射 name/status；`usage` 从 `result.usage`。不做代码围栏/前缀清理，交前端（§4.3）。

### 3.4 `cancel_ai_task(task_id)`

`chat_runtime().cancel_conversation(&format!("wb_{task_id}"))`。循环内 `is_generation_active` 变 false 后自行退出。不需要任务表。

## 4. 前端实现

### 4.1 Adapter

`src/api/tauri.ts` 的 `api` 对象里加：

```ts
runAiTask: (request: AiTaskRequest) => invoke<AiTaskResult>('run_ai_task', { request }),
cancelAiTask: (taskId: string) => invoke<void>('cancel_ai_task', { taskId }),
```

### 4.2 Hook `src/chat/workbench/useAiTask.ts`

```ts
type Options = Omit<AiTaskRequest, 'taskId'> & { onDelta?: (delta: string) => void }
export function useAiTask(): {
  run: (options: Options) => Promise<string | null>  // 返回 text；被取消/迟到返回 null
  cancel: () => void
  busy: boolean
  error: string | null
  partial: string   // stream=true 时累积的流式文本
}
```

要点：`taskId` 每次 `run` 用 `crypto.randomUUID()`；`cancel` 调 `api.cancelAiTask` 并把当前 taskId 标记作废，作废后到达的结果**丢弃**，不写 state；组件卸载时取消订阅 `ai-task-delta`（`@tauri-apps/api/event` 的 `listen`，只处理 `taskId` 匹配的事件）。默认值：`images: []`、`tools: []`、`slot: 'chat'`、`providerId/model/cwd/timeoutSecs: null`、`stream: false`。

### 4.3 `src/chat/workbench/aiText.ts`

`stripFences(text): string`——去掉首尾 ``` 围栏和引号。逻辑照 `prompt_optimize.rs::sanitize_optimized_prompt`（95–132 行），纯函数，配一个测试。

### 4.4 第一个消费者：短视频页「帮我写」

`src/chat/workbench/video/ShortsPage.tsx` 的 `helpWrite`（20 行）目前只弹"即将上线"。改为：

```ts
const ai = useAiTask()
const helpWrite = async () => {
  if (!brief.trim()) return setNotice(t.workbenchShortsNeedBrief)
  const text = await ai.run({
    mode: 'once',
    slot: 'vision',
    system: t.workbenchShortsPromptSystem,     // 新 i18n 键，中英各一段
    prompt: buildShortsAssistPrompt(brief, duration, ratio),  // 新纯函数 video/shortsAssistPrompt.ts
    images: await Promise.all(files.slice(0, 4).map(dataUrl)),  // localMedia.ts 已有 dataUrl
  })
  if (text) setPrompt(stripFences(text))
}
```

按钮在 `ai.busy` 时禁用并显示"生成中"；`ai.error` 显示到现有 `notice`。`AvatarPage` / `DramaPage` 的同名按钮**这次不接**，留给后续按同样形状复制。

## 5. 测试（必须有，规范 §7）

Rust（`ai_task.rs` 的 `#[cfg(test)]`，SSE mock 直接搬 `prompt_optimize.rs` 430–488 行的 `start_sse_mock`）：

1. `once` 无工具：请求体 `"stream":true`、不含 `reasoning_effort: none`、system 就是调用方给的那一句；返回文本正确。
2. `once` 带图：请求体里 user content 是数组且含 `image_url`；slot `chat` 被改成 vision 模型（两个 provider 区分）。
3. 不写会话：跑完后 `repository(&app).list(...)` 不含 `wb_` 开头的 id（或断言 `end_generation` 后 `has_active_generation("wb_x") == false` 且 repository 目录无新文件）。
4. 白名单：给 `["memory_save", "read"]`，结果工具集不含 `memory_*`；给全不存在的名字报错。
5. `once` 的 `max_tool_rounds == Some(1)`，`agent` 保持设置值——对 `AgentRunConfig` 的构造函数单测即可，不必跑循环。
6. 超时：mock 不回包，`timeoutSecs: 1`，返回超时错误且 `has_active_generation` 为 false。
7. 自动化回归：`automation` 现有测试全部保持绿（host 搬家后行为不变）。

TS（vitest）：

1. `useAiTask`：run 期间 `busy` 为 true；后端 reject → `error` 有值；`cancel` 后迟到的 resolve 不改 state。
2. `stripFences` 三个用例（围栏、前缀、空）。
3. `ShortsPage.test.tsx`：mock `api.runAiTask`，点「帮我写」→ 断言调用参数 `mode: 'once'`、`slot: 'vision'`、`images.length === files.length`；resolve 后提示词框里是去围栏的文本。无 brief 时不调用。

## 6. 验收

```sh
npm run protocol:generate   # 生成 src/generated/aiTask.ts，随代码一起提交
npm run typecheck && npm run lint && npm run architecture:check && npm run test
cd src-tauri && cargo test
```

实机：`npm run dev` → 设置里配一个带 vision 的聊天模型 → 工作台 → 短视频生成 → 传一张产品图、写一句 brief → 点「帮我写」→ 提示词框被填充；期间点取消，结果不再写入；侧栏**没有**新会话。

## 7. 明确不做

- 不迁移 `title.rs` / `prompt_optimize.rs` / `vision.rs` 到新入口。入口设计已经容得下它们，迁移是后面的独立任务。
- 不加设置页 UI，不加新的 `default_models` 槽位。
- 不在 Rust 里放任何业务提示词。提示词内容属于页面的纯函数（`build*Prompt`），和 `image/mainImagePrompt.ts` 一样。
- 不返回模型原生生成的图片（`AgentRunResult.images`）。工作台生图统一走 `media_generation`。
- 不做任务持久化/历史列表。`once` 结果落在页面 state；`agent` 产物由工具（`present_artifacts`）自己落盘，页面后续用 `chatArtifactsList` 读。

## 8. 提交时附一段 ADR

新建 `docs/adr/0008-workbench-ai-calls-go-through-one-headless-entry.md`，五六行即可：决定（一个入口、`mode` 只改轮次/超时/流）、为什么不是两个入口（一步也要工具，两条路径会重复前置检查）、后果（标题/优化/vision 后续可收敛进来；`wb_` 会话不落盘）。
