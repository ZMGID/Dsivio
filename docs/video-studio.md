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
- ComfyUI 客户端依赖可从设置安装到应用专属虚拟环境；不会启动或安装 ComfyUI 服务端。

## 生成与恢复

路线无默认值；禁止失败后自动切换。编辑任务会清除剧本确认、提示词和报价。
报价十分钟有效，配置变更会使报价失效。提交前先持久化状态，获得远程编号后立即保存。
提交超时且无可靠回执时进入待核查状态，不能重复提交；用户可从供应商控制台核实后补录编号。
查询只访问原任务地址和编号。API 成片通过上游合同检查后下载；ComfyUI 在成功或失败后尝试释放空闲模型，队列有任务时跳过。

当前 UI 的 API 报价支持 MiniMax 国内官方 API（CNY）和 xAI 官方 Grok 模型（USD）；自定义网关费用未知时不使用官方价格放行。原插件的高级 API 参数仍可在聊天使用。
API Key 不返回前端或写入任务和模板。结果大于 80 MB 时使用本地播放器打开。
浏览器环境只预览布局，真实执行需桌面应用、已配置的视觉聊天模型以及所选路线依赖。

## 验证

- `python -B -X utf8 -m unittest discover -s tests -p test_video_studio.py`
- `./scripts/win-cargo-test.ps1 --lib video_studio::tests`
- `npx vitest run src/chat/chatRoutes.test.ts`
- `npx tsc --noEmit --pretty false`
- `npm run build:ui`

这轮未调用付费视频生成服务；真实供应商、模型输出质量和 ComfyUI 服务端节点需要配置后联调。
