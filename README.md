# Dsivio

Dsivio 是 macOS 与 Windows 上的桌面应用。同一个窗口里有两种形态，从侧栏左上角点一下切换：

- **Dsivio**：和 Dsivio Agent 对话。会话按最近、集、项目组织；Agent 可以使用工具、子代理、Skills、MCP 和知识库。
- **Workbench**：把电商内容生产放在同一套导航里，从选品、店铺、图文、图片、视频，做到发布和素材库。

窗口之外，热键还能翻译正在输入或选中的文字、对屏幕区域做 OCR，并用 Lens 对画面提问。模型密钥由你自己配置，对话记录留在本机。

## 对话

Dsivio Agent 是当前唯一的内置运行时，负责对话、工具执行和子代理。一条对话属于一个集，或属于一个项目，二者取其一。项目带工作目录，Agent 默认在这个目录里读写文件；集则用来放人设和默认助手，不绑定目录。

右侧可以打开文件、Git、终端和任务。设置里配置模型、Skills、MCP、知识库，以及本机工具。

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

首次使用 Dsvideo 会选择或创建项目目录，准备 Runtime 并生成 `DSVIDEO_STATE.md`，不重复安装插件，也不执行付费生成测试。
支持多个目录；Dsvideo 插件的 `data/PROJECTS.md` 登记路径和当前项目（查询命令返回文档绝对路径，插件更新保留数据），项目内状态文档维护创作进度。Agent 可通过
`dsivio dsvideo projects list` 查询，`projects init --path <绝对目录> --name <名称>` 登记新项目，`projects use --path <绝对目录>` 切换。

开发构建在 `npm run build:video-runtime` 后执行 `npm run build:dsvideo`。独立 dsvideo 项目更新后，运行
`npm run build:dsvideo -- --source /path/to/dsvideo` 更新发布快照；日常构建只消费仓库中的快照和锁文件，不依赖相邻 checkout。

内置市场保留 Hypit 等视频制作插件；旧 Dsivio Video 插件的打包资源和注册入口已删除。升级会清理旧内置安装，保留用户数据；工作台继续使用 App 自有的模板和视频分析工具。

宿主的媒体生成、语音、本地 ASR、通用模型参数和取消能力独立于插件市场入口，继续由 Dsivio 持有凭证与任务；移除 Dsivio Video 不移除这些共用能力。

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
