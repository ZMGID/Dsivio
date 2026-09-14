# 同步 Kivio 更新

Dsivio 是 Kivio 的定制版。公共功能主要在 Kivio 开发；本仓库保留图片、视频工作台和 Dsivio 品牌、打包及发布配置。

- `origin`：`https://github.com/ZMGID/Dsivio.git`
- `upstream`：`https://github.com/ZMGID/kivio.git`

## 首次同步

2026-09-14 同步到 Kivio `4e3141a2373a42047c4feeefa4ac5dfe61b8f610`。
Dsivio 初始提交 `1839b26407ae4ffaedd37c85b272ea1b98ddc7ba` 与 Kivio
`00c984fa686be826da4a42ffc5bc3fb514647b7b` 的程序代码一致，仅三个 README 文件不同。
首次同步先用保留当前树的合并提交接回该基准历史，再正常合并上游更新。
后续同步已有共同祖先，不要重复使用 `ours` 策略或 `--allow-unrelated-histories`。

## 后续同步

在干净工作区执行 `git fetch upstream`，检查 `git log HEAD..upstream/main`，
再执行 `git merge upstream/main`。Git 远端配置是本机配置；新克隆需要先添加 `upstream`。

冲突处理以 Kivio 的公共功能为主，但需要同时保留：

- 图片、视频工作台及其 Tauri 接口、模板、插件、运行环境和任务恢复功能。
- 文件夹附件与视频附件；模型的视频生成能力与视频输入能力是两个独立字段。
- 工作台使用的媒体计价数据，以及 Dsivio 的名称、版本和更新地址。

自动合并成功不代表接口兼容。同步后运行协议与 TypeScript 检查、相关前后端测试、
视频 Python 测试、UI 构建和桌面二进制构建。实际生图、视频供应商调用与发布另行验证。
