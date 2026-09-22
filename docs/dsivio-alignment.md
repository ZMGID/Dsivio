# Dsivio 与 Kivio 对齐

以 Kivio 3.0.0 的代码为基础，Dsivio 保留独立版本号（当前为 1.0.1）和 ZMGID/Dsivio 发布仓库，保留 Dsivio 品牌、图片工作台、视频工作台、应用市场、紫鸟 CLI、葡萄牙语翻译。

图片和视频页面不再选择提示词助手或优化提示词。视频方案使用默认聊天模型。

视频插件及其必需依赖随应用内置：构建时准备固定版本的 Node、Python、ffmpeg、Comfy MCP 和视频分析 MCP，启动时使用应用资源内的绝对路径，不要求用户手动安装。模型分析仍使用混音器中的视频分析模型；没有配置时自动选择 MCP。

发布沿用 Kivio 的 macOS DMG、Windows NSIS 和便携包流程，产物使用 Dsivio 名称，更新源仍是 ZMGID/Dsivio。原数据目录标识 com.zmair.kivio 保留。

Dsivio 品牌特征必须保留：独立版本号与发布仓库、名称和图标、冷启动点阵 Logo、对话切换点阵扫光动画。同步上游功能和架构时，不得以 Kivio 默认表现替换这些特征；动画继续尊重减少动态效果设置。
