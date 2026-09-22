# 视频模型接入核对记录

核对日期：2026-09-21 至 2026-09-22。本文记录官方资料与本批实现范围，不替代统一工程规范。型号与取值以 `src/data/videoModelCatalog.json` 为机器可读权威；本文不另列一份型号参数表。

## 归属与调用路径

- 模型管理：供应商密钥、原生 Base URL、模型启用与 `videoProtocol`。聊天协议与视频协议独立。
- 模型分工：`defaultModels.imageGeneration` 和 `defaultModels.videoGeneration` 属于对话，独立于 Workbench。对话生图沿用已有工具，视频对话工具尚未接入。
- 媒体创作：独立设置页，`workbenchMedia.imageModels` 与 `workbenchMedia.videoModels` 分别保存多个供应商/模型引用；不复制密钥、不自动加入全部模型、不从对话默认槽位继承。Workbench 页面从池中显式选模，池成员删除后提示重选，不自动切换。
- 官方目录：`src/data/videoModelCatalog.json` 同时提供前端预设、模型元数据和 Rust 校验。新增版本修改此目录，不在页面猜测模型名称。
- 请求边界：`src-tauri/src/video_studio/providers.rs` 负责参数校验、原生 JSON、鉴权、单次提交、查询与结果解码。前端通过现有 `api` 调用。IPC 类型由此 Rust 定义经 `npm run protocol:generate` 生成到 `src/generated/videoGeneration.ts`；`protocol:check` 防止漂移。
- 本批没有改造旧 VideoStudio 的 Python 任务执行器，也没有将其改成新的默认模型消费者。工作台迁移时应直接消费此接口并移除对应旧供应商格式化代码，不能再复制一套新适配。Workbench 模型池为迁移后的执行器提供可选模型；目前工作台生成按钮仍是占位实现，选模不等于生成链路已接通。

统一输入描述提示词、时长、分辨率、比例、首尾帧、参考素材和声音开关。只映射已实现的模式；不支持的组合提前报错，不静默丢弃参考素材。自定义模型需明确指定协议；无法从自定义 ID 推导版本限制，由供应商执行最终校验。

`preview_video_model_request` 只生成带占位密钥的请求，不发送网络请求。`submit_video_model_request` 只提交一次并立即返回远端任务 ID。`query_video_model_request` 查询原任务，校验原连接未变化；Hailuo 自动完成 file_id 到下载 URL 的二次查询。标准结果包含任务 ID、运行/成功/失败/过期/取消状态、结果地址与下载鉴权标记。

生命周期由调用工作台负责：先持久化远端 ID、供应商、模型、协议及 Base URL，再查询；取得 URL 后先持久化再下载。传输中断或返回无效 JSON 不自动重发付费 POST。查询失败保留原任务，不作为新任务提交。临时链接过期应再次查询。Veo 下载需要原 API Key，不能把密钥附在前端链接里。

## 官方协议核对

| 家族 | 官方资料 | 本批关键处理 |
| --- | --- | --- |
| MiniMax H3 | [创建 V2](https://platform.minimaxi.com/docs/api-reference/video-generation-v2-create)、[查询 V2](https://platform.minimaxi.com/docs/api-reference/video-generation-v2-query) | `api.minimax.cn`，`content` 数组；时长、分辨率必填；首尾帧与多模态参考互斥。与 Hailuo 分预设。 |
| MiniMax Hailuo | [文生视频](https://platform.minimax.io/docs/api-reference/video-generation-t2v) | V1 `prompt` 请求；成功任务的 `file_id` 还需 `/files/retrieve`。1080P 只能使用 6 秒。 |
| Seedance | [创建任务](https://docs.volcengine.com/docs/ark/create-video-generation-task-api?lang=zh)、[官方 SDK](https://github.com/volcengine/volcengine-python-sdk/blob/master/volcenginesdkarkruntime/resources/content_generation/tasks.py) | 方舟 `/contents/generations/tasks`；各版本分别维护范围；2.5 首尾帧只允许 adaptive；参考生成显式声明任务类型。当前不暴露视频编辑、延长、自动时长。 |
| Grok Imagine | [视频生成](https://docs.x.ai/developers/model-capabilities/video/generation) | `/videos/generations` 返回 `request_id`，`/videos/{id}` 查询；本批为文生、单首帧模式，1.5 支持声音开关。编辑、延长、多参考待独立适配。 |
| Google Veo | [Gemini API Veo](https://ai.google.dev/gemini-api/docs/veo) | `predictLongRunning`，`x-goog-api-key`；首帧转换为 base64 对象；按 operation name 查询；下载也需鉴权。 |
| Wan | [通义万相文生视频](https://help.aliyun.com/zh/model-studio/text-to-video-api-reference) | 本批 Wan 2.7 原生异步接口，`X-DashScope-Async: enable`，新版本用 resolution/ratio，不沿用旧版 size。地域与密钥需匹配。 |
| Runway | [API reference](https://docs.dev.runwayml.com/api.md) | `X-Runway-Version`；文生与图生分路径；ratio 使用像素比例；Gen4 Turbo 需要首帧。 |
| Vidu | [文生视频](https://platform.vidu.com/docs/text-to-video) | `Authorization: Token`；本批预设只启用已核对的文生模式；任务通过 creations 获取结果。 |
| Luma Ray | [生成](https://docs.agents.lumalabs.ai/guides/videos/generation/)、[迁移](https://docs.agents.lumalabs.ai/guides/videos/migration/) | Ray 3.2 使用 Agents API 和新密钥；视频参数放在 video 对象中；首尾帧仅 5 秒；内联图片使用 data/media_type。 |
| Kling | [3.0 文生](https://kling.ai/document-api/api/video/3-0-omni/text-to-video)、[Turbo 图生](https://kling.ai/document-api/api/video/3-0-turbo/image-to-video)、[鉴权](https://kling.ai/document-api/api/get-started/authentication) | 新版 API Key Bearer；模型在路径里；配置在 settings；Turbo 使用 contents；`/tasks?task_ids=` 查询。旧 AK/SK JWT 不适用此预设。 |

供应商预设仅填连接配置并提供官方型号建议，不代表当前账号已开通、余额充足或接口实测成功。视频专用供应商不调用聊天 `/models`，不执行聊天短测。混合供应商保留远端目录，并补充官方视频建议。生成模型不进入普通聊天选项。

## 已研究但不作为已接入发布

- [Sora 官方说明](https://developers.openai.com/api/docs/guides/video-generation)：已宣布 Videos API 于 2026-09-24 关闭，本批不新增推荐预设。
- [Gemini Omni](https://ai.google.dev/gemini-api/docs/omni)：使用 Interactions API，不能套用 Veo 长任务结构；目录标记为单独协议，尚未实现。
- 厂商官方支持的其他模式、上传素材接口、回调、批处理、视频编辑/延长不因型号出现在目录中就自动视为已支持。当前范围由每个型号的 modes 与原生适配共同限定。

## 验证边界

自动化验证覆盖目录一致性、原生请求格式、鉴权、模型约束、任务回执/终态解码、配置序列化与删除清理、视频模型选择。请求预览不产生费用。未使用用户密钥逐家发起付费生成，不能将这些测试表述为各供应商生产端到端验证。

本批检查结果：101 个前端测试、53 个相关 Rust 测试通过（包含本地 TCP HTTP 提交/查询与错误不重发测试）；TypeScript、所改文件 ESLint、协议生成一致性、架构边界、Vite 构建通过。浏览器组件实测确认视频筛选、默认值选择、原生协议展示、预览错误反馈，以及 1100px 中文/720px 英文下抽屉无横向溢出。浏览器截图接口失败，未完成截图视觉验收。原生进程与 Vite 正常运行，但原生窗口未被当前 UI 工具识别，因此没有宣称桌面 IPC 界面端到端验证完成。

2026-09-22 配置边界更正：媒体创作改为 Workbench 专用模型池；对话生成配置恢复在模型分工。设置通过现有编辑器保存，Rust 规范化负责去重和清理已删除引用。临时禁用供应商保留池成员，Workbench 将其标为不可用；移出模型池不删除供应商模型，不改对话配置。
