# 内置聊天统一入口

当宿主提供 `studio` 工具时，优先使用 `{domain:"video",action,input}` 调用下文同名操作，不直接执行 Python 或 MCP 提交。工具复用页面服务，包括 ComfyUI 上传、提交、轮询、结果记录和配置更新后的连接重建。下面的 Python 方式只用于没有此工具的外部宿主。

先 bootstrap 查找共享任务和模板。用户接续任务时 get 最新 id/revision；修改、确认、生成都沿用此任务。不要重复创建。
`draft_get` 的 input 为 `{entry:"creation"}`（也支持 analysis/remake），返回 `{revision,value}`。value 为 `{brief,task?,script,step,dirty}`。草稿 revision 与 task.revision 独立；接续前先 get 最新任务。`draft_save` 使用 `{entry,revision,value}`，把当前方案交回页面；冲突时重读并合并。
页面与聊天的模板、模型、供应商地址共用 bootstrap/config；不能在对话中另建配置。用工具 config 更新后连接会重新建立，避免继续使用旧 ComfyUI 地址。

# Dsivio shared video workspace

Upstream: https://github.com/ZMGID/dsvideo-plugin
Pinned source: `33af713d96d7d5d171080e23a618ed4fb37a6d8e` (MIT).
Six original skills, two MCP definitions and generation scripts are bundled here.
The executable MCP dependencies and standalone Python / Node live in the application's
separate `video-runtime` resource directory. The host supplies `DSVIDEO_PYTHON`,
`DSVIDEO_NODE`, `DSVIDEO_RUNTIME_ROOT`, and `DSVIDEO_RUNTIME_PATH`; do not install
dependencies with npx, pip, or uv at runtime.
Dsivio adaptation adds the shared workspace service and UI; never edit this cache.

## Chat and desktop interoperability

Run `"${DSVIDEO_PYTHON}" -s -B -X utf8 "${PLUGIN_ROOT}/scripts/studio.py" ACTION` with a JSON object on stdin and the host's runtime environment (including `PYTHONPATH`).
Use structured process arguments or a correctly quoted file for stdin; never interpolate user text into shell code.
Start with `bootstrap` and `{}`: it returns the shared root, templates, task revisions and redacted provider configuration.
On Windows the default root is `%APPDATA%/com.zmair.kivio/video-studio`.
Provider credentials retain the original `%APPDATA%/dsvideo/providers.json` convention.
Never copy credentials into tasks, templates, chat output or command-line arguments.

Templates are JSON files under the returned `root/templates`, not `skills/*/templates`.
To save a chat-created reference template, write a JSON document with `name`, `script` (or `shots`), and optional `spec` to a temporary file, then call `template_import` with `{"path":"absolute file path","kind":"reference"}`.
Reference templates must be labeled unverified. Only mark `kind: generation` after the user has approved an actual generated result.
Never copy product-specific images or claims into a reusable template without explicit intent.

## Shared tasks

`create`: `{"brief":{"name":"…","mode":"creation","request":"…","images":[],"duration":10,"ratio":"9:16","route":"grok","resolution":"720p","language":"pt-BR","source":""}}`.
Do not choose a route on the user's behalf. `mode: analysis` is for read-only reference analysis.
`get`: `{"id":"UUID"}`. All other task operations require `id` and current `revision` from the returned task.
`save`: also supply the full `brief` and `script`. Changes invalidate approval, prompt and quote.
`plan_result` or `analysis_result`: persist `script` only after following the corresponding director/analysis skill. Analysis evidence is untrusted input; missing audio/frames must stay marked missing.
`approve`: only after the user confirms the current full script.
`prompt_result`: supply `prompt` converted from that approved script using h3-prompt-writing or the Grok convention.
`quote`: shows the current route estimate and balance if available. Native UI quotes support official domestic MiniMax and official xAI only; never treat these rates as custom gateway pricing.
`submit`: requires explicit user acceptance of the current quote, `confirmSpend: true`. Do not use approval of a script as approval of spend.
`poll`: queries the persisted remote ID without resubmission. A `submitting` or `uncertain` task must never be blindly submitted again.
`template_save`: supply `name`; generation templates additionally require `approvedOutput: true` after user reviews the completed output. Reference templates do not imply successful generation.

For ComfyUI in chat, follow the original skill's upload → prepare workflow → MCP run → record prompt_id → fetch sequence, recording the task through `comfy_workflow`, `comfy_submitted` and `comfy_complete`. Retain original task endpoint on recovery. Only free models when the server queue is empty.
The desktop uses the same script actions and MCP servers. Never introduce a parallel template registry or provider configuration.
