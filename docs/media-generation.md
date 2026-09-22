# 图片与视频生成接口

生成流程的后端归属是 `src-tauri/src/media_generation.rs`。Dsivio 与 Workbench 复用它；功能页仅提供素材、生成要求、模型选择和 `origin`，不自行发送供应商 HTTP、不实现任务恢复。现有页面的拆分、接线另行进行，本次接口准备不改变页面布局。

## 调用契约

通过 `src/api/tauri.ts` 的三个现有方法调用：

- `startMediaGeneration(request)`：校验模型池、类型和输入，提交一次生成，返回任务编号和快照。云端生成在后台继续；ComfyUI 返回前会上传素材并取得提交回执。
- `getMediaTask(id, resume?)`：读取任务。运行中的任务自动继续查询；`resume: true` 只恢复有回执的查询或下载，不重新生成。
- `listMediaTasks({ origin?, providerId?, model? })`：筛选持久记录。WorkBench 历史按 `origin: "workbench/<feature-id>"` 归属，换模型不丢历史。

Rust 导出的 `src/generated/mediaGeneration.ts` 是请求、任务、图片参数类型的来源。视频参数读取 `src/generated/videoGeneration.ts` 的 `VideoGenerationInput`（`prompt` 放在请求顶层）；ComfyUI 输入映射读取 `src/generated/comfyui.ts`。不要另造每个功能的网络请求格式。

图片请求示例：

```ts
const request: MediaRequest = {
  providerId: selected.providerId,
  model: selected.model,
  kind: 'image',
  origin: 'workbench/main',
  prompt: '使用提供的商品图生成白底主图',
  images: ['/absolute/path/product.png'],
  options: { size: '1024x1024', n: 1 },
}
```

图片 `options` 支持 `size`、`aspect_ratio`、`quality`、`n`（1–4）；`aspectRatio` 为兼容别名。不接受拼错的键或错误类型；尺寸、比例和质量继续由现有图片适配器校验。图片素材支持本地路径和 Base64 data URL。

视频请求仍用相同顶层字段，`kind: 'video'`、`images: []`。视频素材角色必须明确：`options.firstFrame`、`lastFrame`、`referenceImages`，支持本地图片路径、data URL 或供应商支持的 HTTP(S) URL；后端统一读取本地图片。时长、分辨率、比例、音频及参考素材组合由既有模型目录和协议适配器校验。`referenceVideos`、`referenceAudios` 仍须使用对应协议支持的 URL，不隐式上传本地音视频。

功能页通过现有 `useMediaGeneration` 发起与观察任务；供应商协议和轮询规则不进入页面。

## ComfyUI 输入绑定

API 工作流的 `inputs` 可以把通用字段映射到具体节点，配置示例：

```json
[
  { "nodeId": "6", "input": "text", "label": "正向提示词", "kind": "text", "source": { "type": "prompt" } },
  { "nodeId": "10", "input": "image", "label": "商品图", "kind": "image", "source": { "type": "image", "index": 0 } },
  { "nodeId": "12", "input": "seconds", "label": "时长", "kind": "number", "source": { "type": "parameter", "name": "duration" } }
]
```

`parameter` 从通用 `options` 读取参数；例如 `name: "firstFrame"` 绑定首帧，`name: "referenceImages", index: 0` 绑定第一张参考图。转换归工作流适配器，功能页无需了解节点编号。

`source` 是可选的后端配置字段，现有设置页尚未增加它的编辑控件。旧工作流的 `options["节点ID:输入名"]` 原样兼容；没有配置通用绑定时继续使用节点参数。传入未绑定的通用提示词/图片、未声明的节点参数或两处重复输入会明确报错，不猜测正向/负向提示词，也不静默丢弃。绑定后的图片上传到 ComfyUI，再将服务端文件名注入工作流；工作流原图保持不变。任务保存原始提示词和来源。

## 任务与恢复语义

- `running`：正在提交、等待供应商或下载；`succeeded`：输出已保存到本地；`failed`：请求、生成或下载失败，查看 `error`。
- `canResume: true`：已有远端回执，可恢复查询/下载。网络失败不自动重放付费 POST；提交结果不明确且没有回执时不自动重试。
- 每个任务只有一个后台执行者。恢复操作先取得任务所有权，再读取和修改状态；同时打开多个入口不会覆盖完成状态或重复恢复。
- 请求校验后固定本地图片内容。视频提交使用同一份供应商快照；提交前发现协议或地址变化会拒绝。恢复使用原回执与原连接，防止串到新供应商。
- 云端图片、视频与 ComfyUI 共用媒体落盘校验；视频与 ComfyUI 共用原子下载：512 MB 上限、媒体签名检查、失败清理 `.part`，按实际文件类型保存扩展名。鉴权下载不向其他源发送密钥。签名检查不等于完整解码或播放验收。多张图片中后续输出失败，已保存的图片仍保留在任务中。
- 云端任务存于 `media-tasks`，已有 ComfyUI 任务存于 `comfy-tasks`；对调用方均返回同一个 `MediaTask`，密钥不写进任务。旧记录缺新增字段仍能读取。

本地 HTTP 测试覆盖图片请求及落盘、视频回执/查询/失败下载恢复、ComfyUI 上传/输入绑定/回执/下载、不确定提交不重放、旧数据读取与来源过滤。它们验证公共生成流程与现有适配器的组合，不代表所有供应商都已完成真实付费出图出片验收。
