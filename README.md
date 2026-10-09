# Dsivio

Dsivio 是 macOS 与 Windows 上的桌面应用。同一个窗口里有两种形态，从侧栏左上角点一下切换：

- **Dsivio**：和 Dsivio Agent 对话。会话按最近、集、项目组织；Agent 可以使用工具、子代理、Skills、MCP 和知识库。
- **Workbench**：把电商内容生产放在同一套导航里，从选品、店铺、图文、图片、视频，做到发布和素材库。

窗口之外，热键还能翻译正在输入或选中的文字、对屏幕区域做 OCR，并用 Lens 对画面提问。模型密钥由你自己配置，对话记录留在本机。

## 对话

Dsivio Agent 是当前唯一的内置运行时，负责对话、工具执行和子代理。一条对话属于一个集，或属于一个项目，二者取其一。项目带工作目录，Agent 默认在这个目录里读写文件；集则用来放人设和默认助手，不绑定目录。

右侧可以打开文件、Git、终端和任务。设置里配置模型、Skills、MCP、知识库，以及本机工具。

## 即时通讯

「设置 → 即时通讯」默认只显示飞书 / Lark 和企业微信智能机器人的**扫码连接**入口：用手机扫描本地生成的二维码并确认授权，Dsivio 保存对应机器人身份和密钥后启用长连接。连接状态以原生连接结果为准，不把“授权成功”显示成“已连接”。手动配置、访问配对及企业微信自建应用收在折叠的「高级设置」中。实现参考 [Hermes Agent](https://github.com/NousResearch/hermes-agent)，运行时不依赖 Hermes 或 Python。

- **飞书 / Lark**：扫码授权使用长连接，不需要公网回调地址。高级设置可填写 App ID / App Secret；手动创建的应用需要启用机器人、配置 `im.message.receive_v1` 事件和相应的消息 / 资源权限，并发布应用。Webhook 模式另外填写 Verification Token / Encrypt Key，将配置的本机监听地址通过 HTTPS 反向代理暴露给飞书。
- **企业微信智能机器人**：扫码授权或在高级设置填写 Bot ID / Secret，使用官方 WebSocket 长连接；不是只支持发消息的群 Webhook 机器人。
- **企业微信自建应用**：填写企业 ID、Agent ID、Secret、回调 Token 和 EncodingAESKey；将本机回调服务通过 HTTPS 暴露并配置到企业微信。此模式接收文本，附件消息会说明不支持，不会伪装成已处理。

三个入口共用助手、模型和工作目录；清空助手或模型选择后，会话重新跟随当前桌面对话默认配置。密钥按机器人身份存入系统凭证库，不写入普通设置或状态返回值；修改机器人 ID 后，需要重新保存与该 ID 对应的密钥。扫码结果先暂存，保存身份并提交授权时才写密钥；取消、身份改变或设置保存失败不会覆盖旧机器人的凭证，保存失败可重试。

私聊默认使用**访问配对**：发送消息后，在本机此设置页批准配对码。群聊默认只响应允许列表中的群，可以再限制群内用户；飞书默认还要求 @ 机器人。群内不同用户默认分开会话。IM 会话保存在本机，能在桌面对话列表中继续查看；`/new`、`/reset` 开新会话并保留旧记录，`/stop` 停止当前回复和该会话此前已排队的消息，`/help` 查看说明。临时断线只重连传输，不销毁会话队列或正在处理的回合；重连后保留群聊、话题和消息去重信息。

**IM 发起的工具调用及子代理会自动批准，不发送审批卡片；普通桌面对话原有审批策略不变。只授权可信用户。** 配对批准控制谁能访问，不是工具审批。应用需保持运行；不要让 Hermes 和 Dsivio 同时使用同一个机器人，以免抢占连接或分流消息。

可按平台设置通知频道，让定时任务结果使用当前连接发送；连接未就绪时不补发。企业微信机器人群聊只能在有效消息回复窗口内被动回复，通知频道建议填写私聊目标；自建应用通知目标为企业微信用户 ID。


## 工作台

工作台换的是左侧导航，不是另一套地址。图片、视频这些页面在两种形态下都能打开。

| 分组 | 做什么 |
| --- | --- |
| 电商自动化 | 店铺绑定、店铺概览、商品档案、自动化上架、上架检查、工作流 |
| 智能选品 | 同款找货、选品库 |
| 图文创作 | 图文带货、种草文章 |
| 图片创作 | 主图、详情页、海报、精修、换装、套图和自由生图 |
| 视频创作 | 短视频、口播、短剧、复刻、剪辑、字幕和拆解 |
| 视频发布 | 发布、账号授权、发布记录和数据 |
| 内容管理 | 图片模板、视频模板、角色库、素材库 |
| 统计管理 | 使用记录 |

## 视频制作插件

Dsvideo 随 App 打包并在启动时注册，默认启用；可在「插件」页停用。它包含视频编排、Studio、渲染和 Dsivio 媒体适配器。
使用 `dsivio dsvideo --help` 查看命令；无需另外安装 Node 或公开 npm 包。图片、视频生成调用运行中的 App，模型在「设置 > 媒体创作」配置。
系统本地配音使用 `dsivio media speech --model local/system-tts --text-file <文件>` 输出 WAV，实际可用声音见 `media models --kind speech`。Dsvideo 的 WhisperX alignment 调用 App 的 `media transcribe`，安装、模型准备和取消均由 App 管理。

首次使用 Dsvideo 会选择或创建项目目录，准备 Runtime 并生成 `DSVIDEO_STATE.md`；没有 `AGENTS.md` / `CLAUDE.md` 时还会生成 `AGENTS.md`，该项目里的新对话会自动加载它，知道要用 Dsvideo 制作。不重复安装插件，也不执行付费生成测试。
支持多个目录；Dsvideo 插件的 `data/PROJECTS.md` 登记路径和当前项目（查询命令返回文档绝对路径，插件更新保留数据），项目内状态文档维护创作进度。Agent 可通过
`dsivio dsvideo projects list` 查询，`projects init --path <绝对目录> --name <名称>` 登记新项目，`projects use --path <绝对目录>` 切换。

开发构建在 `npm run build:video-runtime` 后执行 `npm run build:dsvideo`。独立 dsvideo 项目更新后，运行
`npm run build:dsvideo -- --source /path/to/dsvideo` 更新发布快照；日常构建只消费仓库中的快照和锁文件，不依赖相邻 checkout。

内置市场保留 Hypit 等视频制作插件；旧 Dsivio Video 插件的打包资源和注册入口已删除。升级会清理旧内置安装，保留用户数据；工作台继续使用 App 自有的模板和视频分析工具。

宿主的媒体生成、语音、本地 ASR、通用模型参数和取消能力独立于插件市场入口，继续由 Dsivio 持有凭证与任务；移除 Dsivio Video 不移除这些共用能力。

内置插件市场的「电商运营」提供「紫鸟店铺日报」。它复用随 App 打包的 `dscraw-report` Skill 和现有定时任务；首次使用由 Agent 检查依赖，让用户选择店铺、数据目录和每日时间，完成首轮采集后按约定设置定时执行。配置、日报和累计台账保存到用户选择的目录，后续直接复用。当前采集范围为 Shopee、TikTok Shop、SHEIN 巴西站，紫鸟登录与必要的 Python/Node 依赖在本机准备。

## 开发

需要 Node.js、npm、Rust，以及 [Tauri 2](https://tauri.app/) 在当前系统上的依赖。macOS 上 OCR 辅助程序还需要 Swift。

```bash
npm install
npm run dev          # 桌面窗口。macOS 会先编 Swift sidecar
npm run dev:ui       # 只起界面，http://localhost:5713
npm run lint
npm run typecheck
npm test
```

Rust 测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Windows 上用 `powershell -File scripts/win-cargo-test.ps1`。聊天协议的 TypeScript 类型由 Rust 导出，改协议后运行 `npm run protocol:generate`，并用 `npm run protocol:check` 核对。

工程约定见 [docs/engineering-standards.md](docs/engineering-standards.md)。领域用词见 [CONTEXT.md](CONTEXT.md)。

## 许可证

[GPL-3.0-or-later](LICENSE)

IM 协议适配保留 [Hermes Agent 的 MIT 授权](docs/licenses/hermes-agent-MIT.txt)；飞书帧与加密实现参考 [lark-oapi 1.6.8 的 MIT 授权](docs/licenses/lark-oapi-MIT.txt)。

### 内置电脑控制工具

安装包固定提供 Cua Driver 0.34.0、Playwright CLI 0.1.22 和 OfficeCLI 1.0.155，以及对应官方技能。版本与下载校验值统一维护在 `scripts/computer-control/versions.json`，Playwright 的传递依赖由同目录锁文件固定。构建使用 `npm run build:computer-control`；验证使用 `npm run verify:computer-control`。

首次打开 Dsivio 会从本地资源完成接入、启用 CUA 和 OfficeCLI，并提供 Playwright 命令与技能，不需要再执行在线安装器。后续重启保留关闭状态和自定义驱动路径。macOS 保留官方 CuaDriver 的签名与应用身份；用户仍需授权辅助功能和屏幕录制。已有不同版本或签名异常的 CuaDriver 不会被静默覆盖，设置页会报告该安装问题。Playwright 连接用户现有浏览器时仍需对应浏览器和扩展授权。ego lite 不随包安装。
