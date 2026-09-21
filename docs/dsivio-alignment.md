# Dsivio 与 Kivio 对齐

以 Kivio 3.0.0 为基础，保留 Dsivio 品牌、图片工作台、视频工作台、应用市场、紫鸟 CLI、葡萄牙语翻译。

图片和视频页面不再选择提示词助手或优化提示词。视频方案使用默认聊天模型。

视频插件及其必需依赖随应用内置：构建时准备固定版本的 Node、Python、ffmpeg、Comfy MCP 和视频分析 MCP，启动时使用应用资源内的绝对路径，不要求用户手动安装。模型分析仍使用混音器中的视频分析模型；没有配置时自动选择 MCP。

发布沿用 Kivio 的 macOS DMG、Windows NSIS 和便携包流程，产物使用 Dsivio 名称，更新源仍是 ZMGID/Dsivio。原数据目录标识 com.zmair.kivio 保留。
