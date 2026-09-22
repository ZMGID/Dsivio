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
