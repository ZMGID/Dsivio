# ComfyUI 自托管工作流接入

核对日期：2026-09-22。

## 配置和使用

模型管理 → 添加预设「ComfyUI 本地」→ 填写服务地址 → 测试连接 → 导入 ComfyUI 导出的 API 格式 JSON。

为工作流命名，选择图片/视频和保存输出的节点；把希望在 Workbench 中填写的节点输入映射为文字、数字或参考图。未暴露的输入保留导入值。普通画布 JSON 包含节点坐标和控件布局，不能直接提交，导入时明确拒绝。

媒体创作中的两个池按工作流名称展示；勾选后，在 Workbench 选择此工作流即可填写参数、生成、查看任务和打开结果文件。工作流仅用于 Workbench，不参与对话模型分工。当前适配无鉴权的本机/局域网自托管服务；不使用云端 API Key，也不自动安装自定义节点或下载模型。

## 单一归属

- `ModelProvider.request.comfy.workflows` 持有工作流图、名称、媒体类型和输入输出映射；供应商 `baseUrl` 持有服务地址。
- `enabledModels` / `availableModels` 中的工作流 ID 由后端规范化派生，媒体池只保存供应商和工作流 ID。
- `src-tauri/src/comfyui.rs` 负责校验、上传、提交、查询和任务持久化；`src/generated/comfyui.ts` 由 Rust 类型生成。
- 设置编辑走既有 settings editor；Workbench 通过现有 API adapter 调用原生命令，不读写设置文件。
- 旧视频工作流和对话 MCP 保留原有行为，此次没有迁移其历史任务。

## 官方协议

依据 [服务器路由](https://docs.comfy.org/development/comfyui-server/comms_routes) 和 [官方 HTTP/WebSocket 示例](https://github.com/Comfy-Org/ComfyUI/blob/master/script_examples/websockets_api_example.py)：

| 操作 | 请求与处理 |
|---|---|
| 连接测试 | `GET /system_stats`，读取系统和设备信息 |
| 检查节点 | `GET /object_info`，比对工作流 `class_type`，缺少节点明确列出 |
| 上传参考图 | `POST /upload/image`，multipart `image` + `type=input`；把返回的 `subfolder/name` 写入已绑定输入 |
| 提交 | `POST /prompt`，JSON `{prompt, client_id, extra_data}`；记录返回的 `prompt_id` |
| 状态 | `GET /history/{prompt_id}`，未完成时查询 `GET /queue`；当前采用轮询显示排队/生成/完成状态，不声称提供节点级百分比 |
| 结果 | 读取所选输出节点的 `images` / `gifs` / `videos` 文件描述；`GET /view?filename=...&subfolder=...&type=...` |

扩展节点必须返回可读取的文件描述，且与声明的图片/视频类型匹配；否则任务显示输出配置错误。连接测试只检查服务和节点存在，不能保证模型权重、插件版本和所有节点输入可运行；最终由 `/prompt` 和执行状态验证。

## 任务与失败语义

提交前持久化任务记录，收到回执后先保存编号再查询。每个任务绑定提交时的地址、输出节点和媒体类型；改配置不会改变历史任务。超时、5xx 或缺少回执显示“提交待核查”，不自动重复 POST。明确的 4xx 拒绝显示失败。

结果生成与本地下载分开记录；下载失败保留远程描述，刷新只恢复下载。结果文件流式下载并限制 512 MB，保存在应用数据目录 `comfy-tasks/<id>/`。Asset 协议仅开放该目录用于预览。关闭页面不取消服务端运行，重开后读取本地记录恢复查询。

## 验证范围

覆盖 API 格式拒绝、节点映射、图片 multipart 上传、单次提交、历史/队列解析、工作流配置持久化、下载失败恢复、视频输出识别、多模型池隔离及前端重复提交防护。HTTP 契约测试使用真实 localhost HTTP 服务模拟 ComfyUI。

## 2026-09-22 内置生成收敛

对话与 Workbench 均进入 `media_generation::start`，共用提交、任务记录、查询与结果保存。现有图片请求适配、视频供应商协议和 ComfyUI 工作流执行继续复用；ComfyUI 的原任务记录作为其协议回执保留，不复制到另一份任务记录。前端只使用 `start_media_generation`、`get_media_task` 和 `list_media_tasks`，只显示生成中、完成和失败。

dsvideo 插件及其导演、参考视频分析、提示词编写、剧本确认、报价流程已撤除；不再自动安装或装载其 MCP，也不再打包对应 Python/Node/Comfy CLI 依赖。应用本身已有的通用对话视频理解工具不属于 dsvideo，保持原用途。

旧供应商配置一次性导入模型管理，不覆盖现有选择；旧任务按原编号和地址导入查询记录，原文件、模板和作品保留。旧 ComfyUI 的固定 H3 工作流不自动作为新工作流注册，需要使用者在模型管理中导入自己的 API 工作流。提交未取得回执不自动重新生成，恢复仅查询原任务或继续下载。

验证以原生 localhost HTTP 契约测试、前端交互测试及类型、协议、构建检查为准；不代表已逐家进行付费生成。按用户要求，本轮不使用浏览器控制。
