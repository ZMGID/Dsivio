# Dsivio 项目文档

核对日期：2026-09-10。当前代码包含尚未提交的工作台改动；本文是开发导航，不是发布说明。项目使用 Tauri v2、Rust、React 18、TypeScript、Vite 和 Tailwind CSS v4，面向 macOS / Windows。

## 从这里开始

| 目的 | 文档 |
| --- | --- |
| 理解架构、代码约定和关键约束 | [CLAUDE.md](../CLAUDE.md) |
| 统一产品用语 | [CONTEXT.md](../CONTEXT.md) |
| 理解聊天模型协议 | [模型适配器契约](../src-tauri/src/chat/model/README.md) |
| 修改图片功能 | [图片工作台实现](prd/image-studio-implementation.md)、[保存目录规范](prd/image-studio-folder-convention.md) |
| 修改视频功能与打包运行环境 | [视频工作台](video-studio.md) |
| 修改共享草稿、任务库和文件删除 | [聊天与图片、视频页面互通](studio-chat-sync.md) |
| 验证聊天实际请求或渲染性能 | [聊天探针](chat-probe.md)、[性能基线](perf/chat-rendering-baseline.md) |
| 准备发布 | [打包流程](RELEASE_PACKAGING.md)、[版本说明](releases/) |

显示名称使用 Dsivio；Rust crate / GUI 二进制 `kivio`、应用标识 `com.zmair.kivio` 和历史兼容字段保持原值。版本号以 `package.json`、Cargo 与 Tauri 配置为准。

## 本地开发与检查

在仓库根目录执行：

```bash
npm ci
npm run dev
```

完整桌面开发命令准备 Swift 辅助程序，Tauri hook 准备视频运行环境并启动 Vite。视频运行环境构建需要 uv；细节见视频工作台文档。`npm run dev:ui` 仅启动端口 5713 的界面预览，真实 IPC、媒体生成和本地文件操作需要桌面应用。

按修改范围选用检查：

```bash
npm run typecheck
npm run lint
npm test
cargo test --manifest-path src-tauri/Cargo.toml
python3 -B -X utf8 -m unittest discover -s tests -p test_video_studio.py
```

`typecheck` 会先核对生成协议；修改 Rust 协议后用 `npm run protocol:generate` 更新生成物。Windows 的 Rust 测试使用 `scripts/win-cargo-test.ps1`。视频依赖打包检查使用 `npm run verify:video-runtime`；构建前需已准备该运行环境。

纯文档修改检查路径、链接和差异即可；没有运行的测试不要标为通过，也不要把历史测试数量当作当前验证结果。

## 决策、设计与实测证据

- [ADR](adr/)：CLI 导入、原生会话所有权和侧栏顺序等已接受决策。
- [聊天架构](CHAT_ARCHITECTURE.md)、[Agent 运行时 PRD](CHAT_AGENT_RUNTIME_PRD.md)、[产品设计](prd/)和[研究记录](research/)保留设计背景；带日期的内容只描述当时方案。
- [Grok 成片规格校验](audits/grok-specs-2026-09-10/README.md)：实际尺寸、时长和音量证据，以及自动检测范围。
- [Grok 提示词长度调查](audits/grok-prompt-length-2026-09-10/README.md)：特定网关的字节边界，不能外推为通用模型上限。
- [媒体流程审计](audits/media-2026-09-10/README.md)：当时的媒体问题与验证快照。

更新文档时以当前代码、配置和可复查证据为准。区分“已实现”“测试覆盖”“真实供应商验证”和“已发布”；审计目录保留原始记录，不把旧证据改写成新结论。
