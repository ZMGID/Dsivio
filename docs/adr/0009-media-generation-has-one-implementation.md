# 生图、生视频只有一套实现，CLI 是运行中 App 的客户端

生图、生视频的协议对接、提交、等待、存结果只在 `src-tauri/src/media_generation` 维护一份。模型只从「设置 > 媒体创作」开启的模型池取，密钥只留在 App 里。

调用方按自己的形态接入同一套实现，入口只做转接：

| 调用方 | 入口 |
| --- | --- |
| 工作台、生成工作流（App 内 Rust） | 直接调 `media_generation` |
| 前端页面 | `start_media_generation` 等 tauri command |
| 对话里的 Agent | `mixer_generate_image` / `mixer_generate_video` / `mixer_media_task` 工具（产物登记、免审批、中断恢复依赖这些工具名，不删） |
| 插件自己的程序、skill、外部 Agent | `dsivio media ...` 命令行 |

`dsivio media` 不自己生成，也不读 `settings.json`：它通过本机回环端口连到正在运行的 App，由 App 执行。连接信息和一次性令牌写在 `<app_data>/run/media-cli.json`（仅当前用户可读）。App 没运行时命令直接失败（退出码 6），不另起一套生成。这样任务记录只有一份，CLI 发起的任务在 App 的任务列表里可见，来源记为 `cli/<source>`。

付费提交不能重复：`--idempotency-key` 相同的请求返回同一个任务；提交结果不确定时退出码 5，调用方只能查询，不能重交。

插件不再读取密钥、不再按协议表自己对接服务；`resources/plugins/_shared/dsivio.md` 只描述命令。

## 模块归属

- `media_generation.rs`：任务的提交、回执、查询、下载、恢复；任务状态只在这里改。
- `media_generation/image_providers.rs`：图片协议。走哪条路由（Gemini 原生 / chat / images API / 异步回执）只由 `resolve_image_route` 决定；异步回执网关在 `image_providers/async_task.rs`。
- `media_generation/video_providers.rs`：视频协议。各协议能不能收参考素材、尾帧、声音开关，下载要不要密钥，写在 `src/data/videoModelCatalog.json` 的 `protocols.*.capabilities`，代码不再写死协议名单；只有请求体形状按协议写在 `prepare` 里。
- `media_generation/artifacts.rs`：所有结果下载。按供应商代理设置，手动跟随跳转，密钥不出原服务，大小有上限。
- `chat/image_generation.rs` 只收集对话里的参考图并调用上面这些。

## 模型只在媒体创作配置

对话生图/生视频、工作台和 `dsivio media` 都从「设置 > 媒体创作」的模型池取模型，池内第一个可用模型是默认。「模型分工」不再有对话生成分组；旧的 `defaultModels.imageGeneration` / `videoGeneration` 在加载时追加到对应池末尾（能生成的才加入），之后不再写回。对话工具跳过 ComfyUI 工作流，因为它的输入是用户绑定的节点参数，不是对话工具的提示词/尺寸参数。

## 任务终态

- 供应商明确给出失败、过期、取消，或者完成了却没有成片（多为内容审核），或者返回无法识别的状态：任务失败，不会再自动查询，也不能恢复。
- 供应商受理后查不到回执（404，或暂时不可用）：先继续等；受理超过 10 分钟仍然查不到，任务失败，但保留回执，可以恢复。提示用户凭回执去找供应商核实，因为可能已经扣费。实测部分中转网关会丢回执，不是我们的配置问题。
- 只有用户主动恢复时才会重新查询这个回执；自动轮询和重新打开页面都只读取状态。
- 提交失败会分类：请求在生成之前被明确拒绝（4xx，408/429 除外）记为「已拒绝」，可以修改后重交；其余情况（传输错误、5xx、无法解析）记为「结果不确定」，不自动重交。

## 本地处理并入媒体任务

本地字幕和剪辑不是第二套生成实现。它们走同一个 `media_generation::start`：供应商 `local`，模型 `ffmpeg-subtitle` / `ffmpeg-edit`，协议 `ffmpeg_subtitle` / `ffmpeg_edit`。ffmpeg 只从随应用打包的 `media_runtime` 解析；找不到二进制就失败，不改用 PATH。烧字幕用 libass 的 `subtitles` 滤镜。本机打包的 ffmpeg 带这个滤镜。任务被打断后恢复为失败，提交状态是已拒绝，可以重做，不会去云端重交。

`dsivio media subtitle` 和 `dsivio media edit` 仍是运行中 App 的客户端。回环协议增加 `service` 字段，监听改到 `app_cli`，端点文件仍是 `<app_data>/run/media-cli.json`。`media`、`ai`、`commerce`、`publish` 共用这一条连接。

## 产物记录

文案也记成媒体任务：模型 `record`，类型 `text`，状态直接是成功，正文在 `output.md`，`result.title` 是标题。来源沿用调用方，例如 `workbench/posts`。`delete_media_task` 删除记录目录；状态仍是运行中的任务拒绝删除。

## 暂不在本次范围

- 工作台图片项目（`workbench/image_projects`）仍有自己的轮询循环，负责兼容旧回执；新任务都通过 `media_generation` 提交。
- 「允许外部程序调用媒体生成」开关。
