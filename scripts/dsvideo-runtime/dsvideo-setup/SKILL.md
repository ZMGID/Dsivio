---
name: dsvideo-setup
description: 初始化、登记、切换或修复 Dsivio 内置 Dsvideo 的项目目录。用户首次使用、要求新建视频项目或更换创作目录时使用；无需安装插件和供应商适配器。
---

# Dsvideo 项目初始化

Dsvideo 已随 App 安装。只准备用户的项目，不安装 npm 包、不复制 Provider、不要求供应商 URL 或密钥、不执行付费测试。

1. 运行 `dsivio dsvideo projects list`，读取项目登记、当前项目以及返回的 `document` 项目索引文档。登记不代表目录仍可用，检查 `available`。
2. 用户可以继续任意已登记项目，也可以指定新的目录。没有明确目录时询问，不默认使用首次创建的目录，也不扫描无关文件夹寻找项目。
3. 新目录：`dsivio dsvideo projects init --path <绝对路径> --name <项目名称>`。该命令创建目录、准备缺失的 Runtime、创建 `DSVIDEO_STATE.md` 并登记。已有 Profile 和创作记录不会被覆盖。登记失败时先解决错误，不声称完成。
4. 已登记目录：`dsivio dsvideo projects use --path <绝对路径>`。目录移走或 Runtime 损坏时说明实际问题；用户指定新位置后再初始化登记，不能悄悄创建一个替代目录。
5. 在选定目录读取 `DSVIDEO_STATE.md`，通过 `--workspace <目录>` 操作对应项目。项目索引是插件内的多项目登记，不能据此把已打开的其他项目误认为当前对话的工作目录；使用 App「Dsvideo > 使用」可以选择项目并绑定对话。

项目索引文档放在 Dsvideo 插件自己的 `data/PROJECTS.md` 中，同目录的 `projects.json` 保存登记数据。先运行 `dsivio dsvideo projects list`，按返回的 `document` 绝对路径读取文档，不猜测安装位置。这两个文件由上述命令维护，不直接编辑；插件更新保留 data 目录。项目内 `DSVIDEO_STATE.md` 由 Agent 持续记录创作目标、进度、素材与产物路径、待办和用户确认的决定；每次制作或交接后更新，不写密钥，也不声称缺失的产物已经生成。

这只证明项目准备完成。模型连接、浏览器及其他本地工具按实际任务检查，需要安装或付费时沿用用户授权。
