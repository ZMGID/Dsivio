# 关于 Dsivio

这份文档只是告诉你 Dsivio 有什么，是否用、怎么用，由你根据任务和用户的意思决定。

## 生成图片、视频：用 `dsivio media` 命令

插件需要生图或生视频时，调用 `dsivio media` 命令即可。它使用用户在「设置 > 媒体创作」里开启的模型，由正在运行的 Dsivio 完成请求、等待和下载。

- **不要向用户要 URL 和 API Key，也不要去读 Dsivio 的设置文件或密钥**。插件自己的配置里只写这条命令。
- **不需要按厂商协议自己对接服务**。模型选择、参数换算、任务轮询、结果下载都由 Dsivio 负责。
- `dsivio` 在 Dsivio 的对话终端里已经在 `PATH` 上。其他环境下是 `~/.kivio/bin/dsivio`（Windows 为 `%USERPROFILE%\.kivio\bin\dsivio.cmd`），Dsivio 每次启动都会更新它。
- Dsivio 必须处于运行状态。没运行时命令以退出码 6 结束，请提示用户打开 Dsivio。

### 命令

```sh
dsivio media models [--kind image|video]
dsivio media image --prompt-file prompt.txt [--model 供应商/模型] [--ref a.png]... \
    [--ratio 16:9] [--size 2K] [--quality high] [--n 1] [--out ./outputs]
dsivio media video --prompt-file prompt.txt [--model 供应商/模型] [--first-frame a.png] [--last-frame b.png] \
    [--ref r.png]... [--ref-video v.mp4]... [--ref-audio a.mp3]... \
    [--duration 5] [--resolution 720p] [--ratio 9:16] [--audio] [--out ./outputs]
dsivio media status <任务ID> [--resume]
dsivio media wait <任务ID> [--timeout 秒] [--out ./outputs]
```

- `models` 列出当前可用的模型（供应商已启用、模型仍在其列表里的才会出现）。`id` 的格式是 `供应商/模型`，可以原样传给 `--model`。列表顺序就是「设置 > 媒体创作」里拖动排好的优先级，不传 `--model` 时用第一个（`default: true`）。
- 每个模型带 `capabilities`，说明它能做什么，**调用前先看它，不要猜**。`known: false` 表示 Dsivio 没有这个模型的资料（`capabilities` 为 `null`），这时只能只传提示词。
  - 视频：`modes`、`durations`、`resolutions`、`ratios`、`audioToggle`（能否开关声音）、`firstFrame` / `lastFrame`（首帧、尾帧，尾帧必须同时给首帧）、`maxReferenceImages` / `maxReferenceVideos` / `maxReferenceAudios`（参考素材上限，为 0 表示不支持）、`referenceAudioNeedsVisual`（参考音频必须同时有参考图或视频）、`framesExcludeReferences`（首尾帧和参考素材不能同时用）、`localReferenceMedia`（参考视频和音频能否用本地文件，否则必须是 http(s) 链接）、`maxPromptLength`、`defaults`。
  - 图片：`maxReferenceImages`、`sizes`（`--size` 可用的档位）、`ratios`、`maxCount`（`--n` 上限）。
- 提示词可以用 `--prompt "..."` 直接传，也可以用 `--prompt-file 文件`，或 `--prompt-file -` 从标准输入读取。中文或较长的提示词建议用文件或标准输入。
- 本地路径可以是相对路径，会按当前目录解析。也可以传 `http(s)://` 链接。
- `image` / `video` 默认会等到生成结束才返回。加 `--no-wait` 则提交后立刻返回任务 ID，之后用 `status` 或 `wait` 查询。等待上限用 `--timeout` 设置，图片默认 600 秒，视频默认 1800 秒。
- 已拿到远程回执后，查询中的临时 404 / 408 / 429 / 5xx 保持 `running`，继续等待同一任务。`running` 只表示结果尚未确认，不证明服务端已经恢复；`error` 可能包含查询诊断。后台等待有上限，查询中断时会保留 `running`、`canResume: true`，下次 `status` / `wait` 自动继续查询，不会重新提交。
- `status` 平时不需要 `--resume`。旧记录或下载失败若停在 `failed` 且 `canResume` 为 `true`，才加它；它会用同一个远程回执继续查询，不会重新提交。不要因为查询失败重新发起付费生成。
- `--idempotency-key <key>`：同一个 key 只会提交一次，重复调用返回同一个任务。可能重试的调用方（例如按节点执行的流水线）都应该传这个参数，避免重复扣费。
- `--source <名称>`：记录调用来源，例如 `hypit`，便于在 Dsivio 的任务记录里区分。
- `--options-json '{...}'`：额外的模型参数，原样合并进请求。不能和上面的参数重复。
- `--out <目录>`：成功后把结果复制到这个目录。不传时，结果文件留在 Dsivio 的任务目录里，路径见输出。

### 输出和退出码

stdout 只输出一行 JSON，进度和错误说明写在 stderr。

```json
{"id":"…","kind":"video","status":"succeeded","model":"…","providerId":"…","remoteId":"…",
 "outputs":[{"path":"/…/output.mp4","mime":"video/mp4"}],"error":null,"submissionState":null}
```

`status` 的取值为 `running`、`succeeded`、`failed`。

| 退出码 | 含义 | 怎么办 |
| --- | --- | --- |
| 0 | 成功（加了 `--no-wait` 时表示已提交） | 取 `outputs[].path` |
| 2 | 参数错误或模型未开启，**没有提交** | 修正参数后重试 |
| 3 | 服务拒绝了请求，**没有扣费** | 可以重试或换模型 |
| 4 | 已提交，但生成失败 | 查看 `error` |
| 5 | **提交结果不确定** | **不要重新提交**，用 `status` 查询，或请用户确认 |
| 6 | Dsivio 没在运行 | 请用户打开 Dsivio |
| 124 | 等待超时，任务仍在运行 | 用 `wait <任务ID>` 继续等待 |

生成是付费调用。遇到 5 和 124 时不要重新提交同一个请求。

## Dsivio 自带的工具

在 Dsivio 对话里，也可以直接使用下面这些工具：

| 工具 | 作用 | 出现的条件 |
| --- | --- | --- |
| `mixer_video_analysis` | 分析会话里的视频附件，结果会复用 | 「设置 > 模型分工 > 视频分析模型」有可用模型 |
| `mixer_generate_image` | 生成或编辑图片，可传本地图片路径或之前结果的 `art_` ID 作参考 | 「设置 > 模型分工 > 生图模型」已选 |
| `mixer_generate_video` | 生成视频，立即返回任务 ID | 「设置 > 模型分工 > 对话视频生成模型」已选 |
| `mixer_media_task` | 用任务 ID 等待或查询同一个任务，拿到本地文件 | 有生成工具时 |

插件自己的程序、脚本以及外部 Agent 调用不到这些工具，这些场景请使用 `dsivio media` 命令。
