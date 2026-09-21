# Dsivio 与 Kivio 对齐

以 Kivio 3.0.0 为基础，保留 Dsivio 品牌、图片工作台、视频工作台、应用市场、紫鸟 CLI、葡萄牙语翻译。

图片和视频页面不再选择提示词助手或优化提示词。视频方案使用默认聊天模型。

不再打包 Node、Python、ffmpeg。视频工作台使用本机 `python3`（Windows 为 `python`，可通过 `DSVIDEO_PYTHON` 指定），处理媒体时使用本机 ffmpeg。ComfyUI 功能需要安装 comfy-cli 与 comfy-mcp；MCP 分析需要本机 mcp-video-analyzer。模型分析仍使用混音器中的视频分析模型；没有配置时自动选择 MCP。配置界面会显示依赖检测结果，缺失环境不会自动下载安装。

发布沿用 Kivio 的 macOS DMG、Windows NSIS 和便携包流程，产物使用 Dsivio 名称，更新源仍是 ZMGID/Dsivio。原数据目录标识 com.zmair.kivio 保留。
