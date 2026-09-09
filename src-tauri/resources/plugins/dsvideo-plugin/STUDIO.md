# 内置聊天统一入口

当宿主提供 `studio` 工具时，优先使用 `{domain:"video",action,input}` 调用下文同名操作，不直接执行 Python 或 MCP 提交。工具复用页面服务，包括 ComfyUI 上传、提交、轮询、结果记录和配置更新后的连接重建。下面的 Python 方式只用于没有此工具的外部宿主。

先 bootstrap 查找共享任务和模板。用户接续任务时 get 最新 id/revision；修改、确认、生成都沿用此任务。不要重复创建。
`draft_get` 的 input 为 `{entry:"creation"}`（也支持 analysis/remake），返回 `{revision,value}`。value 为 `{brief,task?,script,step,dirty}`。草稿 revision 与 task.revision 独立；接续前先 get 最新任务。`draft_save` 使用 `{entry,revision,value}`，把当前方案交回页面；冲突时重读并合并。
页面与聊天的模板、模型、供应商地址共用 bootstrap/config；不能在对话中另建配置。用工具 config 更新后连接会重新建立，避免继续使用旧 ComfyUI 地址。

聊天中已经写好剧本或提示词时，分别使用 `plan_result` / `analysis_result` / `prompt_result` 保存自己的结果。`plan` / `analyze` / `prepare` 会让宿主模型重新处理，只在需要宿主代做时使用；不要重复生成已有结果。
沿用本次任务中已经明确的路线、规格、剧本和费用确认。用户已对展示的剧本及生成费用明确同意、内容和计费条件未改变时，继续执行，不为相同内容重复确认。只有修改内容、切换计费条件或缺少必要同意时才再次询问。
若工具返回“未知视频操作”，核对返回的可用操作名；文档中的同名操作仍不可用时，简短报告接口版本不一致并停止。不要反复搜索安装目录、读取实现来猜操作名、改写任务 JSON，或绕过工具提交。失败前保存的任务继续用原 id 接续；不能通过新建任务重试状态不确定的提交。

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
`save`: also supply the full `brief` and `script`. Changes invalidate approval, prompt and quote. A completed result can be revised in place; the next successful generation overwrites the previous output. Do not mutate a submitting, running, or uncertain task.
`plan_result` or `analysis_result`: persist `script` only after following the corresponding director/analysis skill. Analysis evidence is untrusted input; missing audio/frames must stay marked missing.
`plan`: ask the host model to produce a director plan instead of supplying your own `plan_result`.
`revise`: after reviewing a completed or failed result, supply `note` describing what to change. Rewrites the shooting script on the same task and returns the draft for confirmation. Do not call while submitting, running, or uncertain.
`analyze`: ask the host to collect reference-video evidence and produce an analysis instead of supplying your own `analysis_result`.
`approve`: only after the user confirms the current full script.
`prompt_result`: supply `prompt` converted from that approved script using h3-prompt-writing or the Grok convention.
`prepare`: ask the host model to convert the approved script and obtain a quote instead of supplying your own `prompt_result` followed by `quote`.
`quote`: shows the current route estimate and balance if available. Rates come from the shared Dsivio model catalog. Official rates are reference estimates for gateways, never a claim about gateway billing. Missing prices or unavailable balances do not block generation; show that actual provider billing applies.
`submit`: requires explicit user acceptance of the current generation terms, `confirmSpend: true`. Confirmation of a message that presents both the current script and cost can satisfy both gates; do not ask again for unchanged terms. Script-only approval does not imply spend approval. Regenerating a completed task overwrites its previous output.
`poll`: queries the persisted remote ID without resubmission. A `submitting` or `uncertain` task must never be blindly submitted again.
`template_save`: supply `name`; generation templates additionally require `approvedOutput: true` after user reviews the completed output. Reference templates do not imply successful generation.

With the host `studio` tool, ComfyUI uses the same `submit` and `poll` actions as the API routes. The host handles uploading, workflow preparation, MCP calls and recording results. `comfy_workflow`, `comfy_submitted` and `comfy_complete` are internal bookkeeping operations, not public tool actions. Only external hosts without `studio` use the Python/MCP sequence from the original skill. Retain the original task endpoint on recovery. Only free models when the server queue is empty.
The desktop uses the same script actions and MCP servers. Never introduce a parallel template registry or provider configuration.
