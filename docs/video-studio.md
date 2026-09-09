# 视频工作台

入口：`#chat/videos`。沿用全局主题、Button / IconButton、Select、输入框与滚动条。

视频创作：素材与要求 → Agent 导演剧本 → 用户确认当前剧本 → 转换模型提示词 → 费用确认 → 提交 → 查询与本地成片。
参考分析：单条视频链接或本地视频 → video-analyzer MCP → Agent 根据关键帧、转写和时间线拆解 → 用户确认并保存参考模板。
模板库：区别成片验证模板和未验证的参考拆解模板。内置上游已有的 15 秒卧室 UGC 模板，不伪造示例视频。

## 共用能力

- `src-tauri/resources/plugins/dsvideo-plugin` 保留原插件六个 Skill、两个 MCP 和生成脚本。来源版本记录于 `STUDIO.md`。
- 启动时注册为内置包，沿用现有插件发现和启用机制。保留用户启用状态，应用更新覆盖包内容，用户数据独立保存。
- `scripts/studio.py` 是聊天和 UI 共用的工作区服务。使用 JSON stdin/stdout、跨进程锁、原子写入和乐观版本检查。
- 任务、素材、模板、结果：应用数据目录的 `video-studio`。API 配置仍使用原插件的 `dsvideo/providers.json`。
- 导演和提示词转换复用应用 Agent loop，采用无外部工具的专门步骤；生成通过原 Python 客户端及现有 MCP 连接池执行。
- Comfy MCP、视频分析 MCP、独立 Python / Node、FFmpeg / ffprobe 和 yt-dlp 随应用打包，启动使用应用资源绝对路径，不运行 npx 或现场安装依赖。ComfyUI 服务端与 H3 工作流节点仍由用户部署。

## 内置运行环境构建

构建机需要 Node.js、Rust 和 uv（发布流水线自动准备）。`npm run build:video-runtime` 按 `scripts/video-runtime` 中的版本、npm lockfile 和带哈希的 Python 依赖锁文件生成 `src-tauri/resources/video-runtime`。生成目录不提交到 Git，Tauri 开发启动及打包前自动准备。

支持 macOS Apple Silicon 和 Windows x64，须在目标平台构建。Node 下载校验固定 SHA-256；独立 Python 由 uv 下载。构建时安装依赖，用户机器只运行已打包文件。发行包保留各依赖的许可证和包元数据。

`npm run verify:video-runtime` 在隔离的 PATH 和空 npm 缓存下检查 Python 工作区、Comfy CLI、yt-dlp、两个 MCP 的握手以及本地视频元数据读取。构建流程另会移动整个运行目录到带空格的新路径，验证可搬移性。生成或读取本地测试视频不调用付费 API 或 ComfyUI 服务端。

应用启动时刷新内置 MCP 的启动路径，迁移旧 npx / 系统 Python 配置，并保留用户的服务启用开关和工具选择。MCP 所属包停用时仍由统一资格检查拦截。运行环境作为独立资源打包，不复制进通用插件导入目录。

## 生成与恢复

路线无默认值；禁止失败后自动切换。编辑任务会清除剧本确认、提示词和报价。
报价十分钟有效，配置变更会使报价失效。提交前先持久化状态，获得远程编号后立即保存。
提交超时且无可靠回执时进入待核查状态，不能重复提交；用户可从供应商控制台核实后补录编号。
查询只访问原任务地址和编号。API 成片通过上游合同检查后下载；ComfyUI 在成功或失败后尝试释放空闲模型，队列有任务时跳过。

当前 UI 的 API 报价支持 MiniMax 国内官方 API（CNY）和 xAI 官方 Grok 模型（USD）；自定义网关费用未知时不使用官方价格放行。原插件的高级 API 参数仍可在聊天使用。
API Key 不返回前端或写入任务和模板。结果大于 80 MB 时使用本地播放器打开。
浏览器环境只预览布局，真实执行需桌面应用、已配置的视觉聊天模型以及所选路线依赖。

## 验证

### 参数补齐（2026-09-08）

画幅按路线展开：ComfyUI 8 种，MiniMax 7 种（含自适应），Grok 7 种。
口播语言改为常用语言选择和自定义；可选口播、仅环境音与音乐、静音，并填写固定对白和配乐要求。
Grok 区分单图首帧与最多 7 张参考图；参考模式上限 720p，支持最多 3 个预设音色。静音直接映射 `generate_audio=false`。
MiniMax 增加首尾帧、参考视频、参考音频；各类数量及总数在提交前校验，参考音视频导入时通过 ffprobe 检查时长，并将参考视频时长计入报价。
聊天内置脚本同步支持 `--reference-image` 与 `--voice-id`，保持共享工作区协议。
ComfyUI/MiniMax 声音要求由提示词表达，不能将提示词静音要求当作硬性音轨开关。

核对来源：[xAI 参考生成](https://docs.x.ai/developers/model-capabilities/video/reference-to-video)、[生成规格](https://docs.x.ai/developers/model-capabilities/video/generation)、[定价](https://docs.x.ai/developers/pricing)，及内置 MiniMax 客户端与 ComfyUI 工作流支持表。

- `python -B -X utf8 -m unittest discover -s tests -p test_video_studio.py`
- `./scripts/win-cargo-test.ps1 --lib video_studio::tests`
- `npx vitest run src/chat/chatRoutes.test.ts`
- `npx tsc --noEmit --pretty false`
- `npm run build:ui`

这轮未调用付费视频生成服务；真实供应商、模型输出质量和 ComfyUI 服务端节点需要配置后联调。
