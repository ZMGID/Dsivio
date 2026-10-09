# 关于 Dsivio

这份文档只是告诉你 Dsivio 有什么，是否用、怎么用，由你根据任务和用户的意思决定。

## 生成图片、视频、语音与转写：用 `dsivio media`

插件通过 `dsivio media` 调用运行中的 Dsivio 媒体任务负责人。云端模型必须在「设置 > 媒体创作」中显式开启对应产品协议并加入模型池；已有聊天/视频 key 不表示已开通 TTS 或云转写。本地 WhisperX 的安装与服务进程也由宿主监督，插件不另起同模式服务。

- **不要向用户要 URL 和 API Key，也不要去读 Dsivio 的设置文件或密钥**。插件自己的配置里只写这条命令。
- **不需要按厂商协议自己对接服务**。模型选择、参数换算、任务轮询、结果下载都由 Dsivio 负责。
- `dsivio` 在 Dsivio 的对话终端里已经在 `PATH` 上。其他环境下是 `~/.kivio/bin/dsivio`（Windows 为 `%USERPROFILE%\.kivio\bin\dsivio.cmd`），Dsivio 每次启动都会更新它。
- Dsivio 必须处于运行状态。没运行时命令以退出码 6 结束，请提示用户打开 Dsivio。

### 命令

```sh
dsivio media models [--kind image|video|speech|transcribe|edit|matting] [--json]
dsivio media image --prompt-file prompt.txt [--model 供应商/模型] [--ref a.png]... \
    [--ratio 16:9] [--size 2K] [--quality high] [--n 1] [--out ./outputs]
dsivio media video --prompt-file prompt.txt [--model 供应商/模型] [--first-frame a.png] [--last-frame b.png] \
    [--ref r.png]... [--ref-video v.mp4]... [--ref-audio a.mp3]... \
    [--duration 5|auto] [--resolution 720p] [--ratio 9:16] [--audio|--no-audio] [--out ./outputs]
dsivio media speech --model 供应商/模型 --mode tts --text-file text.txt --voice 音色ID \
    [--instruction-file instruction.txt] [--output-format wav] [--out ./outputs]
dsivio media speech --model 供应商/模型 --mode clone --text-file text.txt \
    --voice-ref authorized.wav --consent-attestation consent.txt [--output-format wav]
dsivio media transcribe evidence.wav --language zh [--model local/whisperx-small] \
    [--sample-frames 正整数] [--timestamps word|segment] [--out ./outputs]
dsivio media subtitle video.mp4 --language zh [--burn] [--model local/ffmpeg-subtitle]
dsivio media edit --plan-file plan.json
dsivio media asr status [--json]
dsivio media asr install [--json]
dsivio media asr stop [--json]
dsivio media cancel <任务ID> [--timeout 秒] [--json]
dsivio media status <任务ID> [--resume]
dsivio media wait <任务ID> [--timeout 秒] [--out ./outputs]
```

- `models` 列出当前可用的模型（供应商已启用、模型仍在其列表里的才会出现）。`id` 的格式是 `供应商/模型`，可以原样传给 `--model`。列表顺序就是「设置 > 媒体创作」里拖动排好的优先级，不传 `--model` 时用第一个（`default: true`）。
- 每个模型带版本化 `description`，它是参数事实的权威；**调用前先读 `arguments` / `constraints`，不要猜**。保留的 `capabilities` 是同一描述的兼容投影。`known: false` 的未知模型仅使用已声明的基本路线，不接受任意扩展参数。
- `description.arguments` 按规范参数名列出类型、允许值/范围、默认、长度/媒体数量/位置等事实及公共 transport。`factsComplete:false` / `unknownFacts` 表示事实尚不完整，不代表任意值都可以传；`billingInfo:null` 代表价格未知，不代表免费。`factsRevision` 是规范描述内容的 SHA256。
- 图像扩展参数如 `background` / `outputFormat` 仅在实现其编码的路线公布。图像参数规范名为 `aspectRatio`；`aspect_ratio` 只是旧输入 alias，两个拼法不能同时传。首帧/尾帧的规范值是单条 `mediaList`，条目为 `{"source":"/abs/a.png","attributes":{}}`；CLI 旧字符串 flags 仍可用。`--duration auto` 只在模型描述的 `specialValues` 真正允许时有效。
- 提示词可以用 `--prompt "..."` 直接传，也可以用 `--prompt-file 文件`，或 `--prompt-file -` 从标准输入读取。中文或较长的提示词建议用文件或标准输入。
- 本地路径可以是相对路径，会按当前目录解析。也可以传 `http(s)://` 链接。
- `image` / `video` 默认会等到生成结束才返回。加 `--no-wait` 则提交后立刻返回任务 ID，之后用 `status` 或 `wait` 查询。等待上限用 `--timeout` 设置，图片默认 600 秒，视频默认 1800 秒。
- 已拿到远程回执后，查询暂时查不到（404、408、429、5xx）时，任务保持 `running`，继续等同一个回执。`running` 只表示结果还没确认，`error` 里可能有查询诊断。
- 供应商受理后 10 分钟仍查不到回执，任务变成 `failed`，`canResume` 为 `true`，`remoteId` 保留回执编号。这通常是供应商那边把任务弄丢了，**可能已经扣费**：请把回执编号告诉用户，让用户找供应商核实，不要自己重新生成。
- 供应商明确失败、过期、取消，或者完成了却没有成片（多为内容审核）时，任务变成 `failed`，`canResume` 为 `false`，不会再查询。
- `status` 和 `wait` 只读取状态。只有 `failed` 且 `canResume` 为 `true` 时，才在用户确认后加 `--resume`：它用同一个回执再查一次，不会重新提交。不要因为查询失败重新发起付费生成。
- `--idempotency-key <key>`：同一个 key 只会提交一次，重复调用返回同一个任务。唯一的例外是服务明确拒绝了请求（退出码 3，没有扣费）：用同一个 key 再调用会重新提交，方便修正原因后重试。可能重试的调用方（例如按节点执行的流水线）都应该传这个参数，避免重复扣费。
- `--source <名称>`：记录调用来源，例如 `hypit`，便于在 Dsivio 的任务记录里区分。
- `--options-json '{...}'` / `--options-file 文件`：互斥地传入已声明的通用模型参数对象；不能与普通 flags 或其他 alias 重复。未知参数不是 vendor body 透传，会在任务创建/付费前拒绝。`false` 与 `0` 原样保留。
- `--description-revision <factsRevision>`：执行计划时传回模型描述版本。版本不匹配返回 `MODEL_DESCRIPTION_CHANGED`、退出 2，不提交；重新获取 models 并重新计划，不能忽略变化继续收费。
- `--out <目录>`：成功后把结果复制到这个目录。不传时，结果文件留在 Dsivio 的任务目录里，路径见输出。
- TTS 与 clone 都输出可验证的音频任务产物。clone 只在模型描述明确支持时可用，必须提供用户真实授权的样本与同意声明；不替用户编造声明，不把普通音色选择当作上传克隆。
- `transcribe` 输入必须是 16kHz/mono/PCM s16 WAV，data 长度与 `sampleFrames` 一致；任意 MP3/视频先由插件素材工具提取标准证据。返回标准 MediaTask，转写数据在 `result` 或 JSON output 中；没有对齐时间的词保留缺省，不估算补齐。
- `transcribe` 必须指定 `--language`，不做自动识别；语言必须已在「设置 > 媒体创作 > 转写」勾选（见 `asr status` 的 `settings.languages`），否则报 `ASR_LANGUAGE_NOT_INSTALLED`，此时请用户在设置里添加，不要猜语言重试。
- 本地 WhisperX「未安装」不等于不可用：`asr status` 的 `settings.autoInstall` 为 true 时，直接 `transcribe` 即由 App 自动安装，`note` 字段说明下一步。安装下载约 2–5 GB（约 1.5 GB 环境，加每种语言 0.4–1.3 GB），耗时较长，开始前告诉用户；中断或失败后重试会续传已下载的部分。
- `asr install` 会真的开始安装，按设置中的模型与语言执行；不要用它查看用法（用 `dsivio media --help`）。传入与设置不同的 `--model`/`--language` 会被拒绝。返回安装操作状态，退出 0 仅表示开始/复用，不表示 ready；继续查 `asr status`。手动安装与首次识别共用同一 owner，失败保留旧环境；本地失败不自动上传云端。`asr stop` 只停空闲服务，busy 时先明确取消对应 task。
- `cancel` 返回 `confirmed|requested|unsupported|too-late` 以及作用域/费用事实；退出 0 不等于远端已取消或退款。同步云请求已发出时通常无法确认取消；不删除回执、成功产物或供应商记录。
- `models --kind matting` 在当前延期范围返回空列表；导入透明素材不等于已实现云端抠像。

### 输出和退出码

stdout 只输出一行 JSON，进度和错误说明写在 stderr。
参数描述错误 stdout 为 `{code,argumentPath,ruleId,actual,expected,message,exitCode:2}`，例如 `MODEL_ARGUMENT_UNSUPPORTED`；stderr 仍提供人类诊断。`--json` 显式声明 JSON 输出，不改变结果外形。


```json
{"id":"…","kind":"video","status":"succeeded","model":"…","providerId":"…","remoteId":"…",
 "outputs":[{"path":"/…/output.mp4","mime":"video/mp4"}],"error":null,"submissionState":null}
```

`status` 的取值为 `running`、`succeeded`、`failed`、`cancelled`。speech/transcribe 默认等待 600 秒；超时仍查询原 task。转写 `result` 内联有大小上限，完整证据始终可通过 JSON output 读取。

| 退出码 | 含义 | 怎么办 |
| --- | --- | --- |
| 0 | 成功（加了 `--no-wait` 时表示已提交） | 取 `outputs[].path` |
| 2 | 参数错误或模型未开启，**没有提交** | 修正参数后重试 |
| 3 | 服务拒绝了请求，**没有扣费** | 可以重试或换模型 |
| 4 | 已提交，但生成失败 | 查看 `error`；`canResume` 为 `true` 时是回执丢失，可能已扣费，先找供应商核实 |
| 5 | **提交结果不确定** | **不要重新提交**，用 `status` 查询，或请用户确认 |
| 6 | Dsivio 没在运行 | 请用户打开 Dsivio |
| 7 | 查询/等待的任务已确认取消 | 不消费产物，不把取消当作可重提失败 |
| 124 | 等待超时，任务仍在运行 | 用 `wait <任务ID>` 继续等待 |

生成是付费调用。遇到 5 和 124 时不要重新提交同一个请求。

## 内置程序路径：`dsivio tools`

插件需要本地媒体工具时，运行 `dsivio tools --json`，获取内置运行时中实际存在的程序的绝对路径。这个命令不需要 Dsivio 处于运行状态，也不会启动 App。

```json
{"ffmpeg":"/…/ffmpeg","ffprobe":"/…/ffprobe","yt-dlp":"/…/yt-dlp","python":"/…/python3","node":"/…/node","npm":"/…/npm-cli.js"}
```

缺失的程序不输出对应字段；`ffmpeg` 指向内置 `ffmpeg-static` 二进制。不加 `--json` 时，每行输出 `名称<TAB>路径`。退出码 0 表示成功，2 表示参数错误，1 表示无法定位资源或输出失败。获取路径后直接调用对应程序，不依赖它们在 `PATH` 上。

`npm` 是脚本路径，调用时必须使用同一 `tools.node`：`node npm-cli.js ci --omit=dev --no-audit --no-fund`。不切到 system npm，不重写 release lock；失败不标记 setup 成功，也不替换上一个可用 runtime。

## 终端里的 `node`、`npm`、`npx`、`python`、`ffmpeg`

Dsivio 把内置运行时以 `node`、`npm`、`npx`、`python`、`python3`、`ffmpeg`、`ffprobe` 这些命令名放在它启动的终端（bash、Git Bash、PowerShell）PATH 的**末尾**。没有装系统 Node、Python 或 ffmpeg 的电脑上，插件文档里的这些命令可以直接运行；用户自己装的版本排在前面，仍然优先。唯一例外：macOS 未装命令行工具时 `/usr/bin/python3` 只会弹出安装框，Windows 的应用商店 `python.exe` 别名只会打开商店，这两种情况下内置 `python`/`python3` 排在它们前面。

- 内置 `npm`/`npx` 是 Node.js 22 自带的 npm。未指定 prefix 时，`npm install -g` 装到 Dsivio 私有目录 `~/.kivio/npm-global`（已在 PATH 上），不写系统目录。未指定 registry 时，下载先走 npmmirror 镜像；依赖安装失败时可改用官方源重试一次，`npx`、`npm exec` 和 `npm x` 执行的命令不会自动重跑，镜像缺包时可临时设 `npm_config_registry=https://registry.npmjs.org/`。参数、环境变量和 `.npmrc` 中的 registry/prefix 配置均保留。
- 内置 `python`/`python3` 是 Python 3.12，自带 openpyxl、Pillow、lxml、requests、jsonschema、pypdf、python-docx。它拒绝把包 `pip install` 进 Dsivio 自身；需要其他包时先建虚拟环境：`python -m venv <目录>`，再用该环境的 python 安装和运行。
- 要确定用的是内置 Python（不受用户 PATH 影响），写 `dsivio python …`；内置运行时缺失时它以退出码 127 结束，这时才改用系统 Python 3。

## Dsivio 自带的工具

在 Dsivio 对话里，也可以直接使用下面这些工具：

| 工具 | 作用 | 出现的条件 |
| --- | --- | --- |
| `mixer_video_analysis` | 分析会话里的视频附件，结果会复用 | 「设置 > 模型分工 > 视频分析模型」有可用模型 |
| `mixer_generate_image` | 生成或编辑图片，可传本地图片路径或之前结果的 `art_` ID 作参考 | 「设置 > 媒体创作」图片模型池有可用模型（用排在最前的） |
| `mixer_generate_video` | 生成视频，立即返回任务 ID | 「设置 > 媒体创作」视频模型池有可用模型（用排在最前的） |
| `mixer_media_task` | 用任务 ID 等待或查询同一个任务，拿到本地文件 | 有生成工具时 |
| `mixer_process_video` | 本地 ffmpeg 字幕或剪辑，立即返回任务 ID | 始终可用，不走云模型 |

`dsivio ai run` 用正在运行的 App 做一次工作台 AI 调用，不另开会话。App 没运行时退出码 6。

```sh
dsivio ai run (--prompt <文本> | --prompt-file <文件|->) [--mode once|agent] [--system <文本>] \
    [--image <图片>]... [--video <视频>] [--slot chat|vision|promptOptimize|videoAnalysis] \
    [--model <供应商/模型>] [--timeout <秒>]
```

`--kind edit` 列出 `local/ffmpeg-subtitle` 和 `local/ffmpeg-edit`。字幕命令烧录需要打包的 ffmpeg 带 libass `subtitles` 滤镜。剪辑计划是 JSON 对象，放在 `--plan-file` 里。

插件自己的程序、脚本以及外部 Agent 调用不到这些工具，这些场景请使用 `dsivio media` 命令。

### 提交响应丢失时按 key 找回（只读）

```sh
dsivio media status --source dsvideo --idempotency-key <原提交key>
```

此入口只读取 App 的任务记录，不提交生成，也不恢复供应商查询；不能与任务 ID 或 `--resume` 混用。返回原任务 JSON，退出码沿用任务状态。没有记录时退出 2，诊断包含 `MEDIA_TASK_NOT_FOUND`；这不证明厂商未受理，调用方继续查询或提示核实，不能自动重交。

## 发布到 TikTok / YouTube：用 `dsivio publish`

插件通过 `dsivio publish` 调用正在运行的 Dsivio。密钥留在 App 的凭据库里，命令行不读取 client secret 或访问令牌。账号要先在工作台或 `begin` / `complete` 里用用户自己的应用凭证完成授权。

```sh
dsivio publish accounts
dsivio publish begin --platform tiktok|youtube --client-id <id> --client-secret <secret> [--redirect <url>]
dsivio publish complete --request <id> [--callback <url>]
dsivio publish publish --account <id> --file <视频路径> --title <标题> [--description <文本>] [--privacy public|private|unlisted|friends] [--tag <标签>]...
dsivio publish records [--account <id>]
dsivio publish status <记录ID>
dsivio publish stats <记录ID>
dsivio publish retry --record <记录ID>
dsivio publish unbind --account <id>
```

`publish` 与 `submit` 是同一次提交。YouTube 的 `--redirect` 留空时，App 在本机回环上接收授权。TikTok 必须粘贴回调。已发布、上传中、处理中或结果不确定的记录不会再次上传；`status` 只查询。平台没有返回的指标在统计里是不支持，不是 0。退出码：0 成功，2 参数错误，3 被拒，4 失败，5 结果不确定，6 Dsivio 未运行。
