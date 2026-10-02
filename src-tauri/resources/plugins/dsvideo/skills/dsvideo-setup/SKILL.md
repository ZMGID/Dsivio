---
name: dsvideo-setup
description: 初始化、登记、切换或修复 Dsivio 内置 Dsvideo 的项目目录。用户首次使用、要求新建视频项目或更换创作目录时使用；无需安装插件和供应商适配器。
---

# Dsvideo 项目初始化

Dsvideo 已随 App 安装。只准备用户的项目，不安装 npm 包、不复制 Provider、不要求供应商 URL 或密钥、不执行付费测试。

1. 运行 `dsivio dsvideo projects list`，读取项目登记、当前项目以及返回的 `document` 项目索引文档。登记不代表目录仍可用，检查 `available`。
2. 用户可以继续任意已登记项目，也可以指定新的目录。没有明确目录时询问，不默认使用首次创建的目录，也不扫描无关文件夹寻找项目。
3. 新目录：`dsivio dsvideo projects init --path <绝对路径> --name <项目名称>`。该命令创建目录、准备缺失的 Runtime、创建 `DSVIDEO_STATE.md`，项目根目录没有 `AGENTS.md` / `CLAUDE.md` 时生成 `AGENTS.md`（让该目录的新对话自动知道用 Dsvideo 制作），并登记。已有 Profile、创作记录和用户自己的说明文件不会被覆盖。登记失败时先解决错误，不声称完成。
4. 已登记目录：`dsivio dsvideo projects use --path <绝对路径>`。目录移走或 Runtime 损坏时说明实际问题；用户指定新位置后再初始化登记，不能悄悄创建一个替代目录。
5. 已有当前对话绑定目录时，优先使用它；目录已登记且 Runtime 可用时，不要求用户再次选目录或重新初始化。没有明确目录时才询问。
6. 在选定目录读取 `DSVIDEO_STATE.md`，通过 `--workspace <目录>` 操作对应项目。项目索引是插件内的多项目登记，不能据此把已打开的其他项目误认为当前对话的工作目录；使用 App「Dsvideo > 使用」可以选择项目并绑定对话。

项目索引文档放在 Dsvideo 插件自己的 `data/PROJECTS.md` 中，同目录的 `projects.json` 保存登记数据。先运行 `dsivio dsvideo projects list`，按返回的 `document` 绝对路径读取文档，不猜测安装位置。这两个文件由上述命令维护，不直接编辑；插件更新保留 data 目录。项目内 `DSVIDEO_STATE.md` 由 Agent 持续记录创作目标、进度、素材与产物路径、待办和用户确认的决定；每次制作或交接后更新，不写密钥，也不声称缺失的产物已经生成。

这只证明项目准备完成。模型连接、浏览器及其他本地工具按实际任务检查，需要安装或付费时沿用用户授权。

## 完成标记

实际读取本 setup 文档，确认用户选定的项目已登记、Runtime 可用，并读取其 `DSVIDEO_STATE.md` 后，在 `projects list` 返回的 `document` 所在目录写入 `SETUP_STATE.md`：

```markdown
# Dsvideo Setup 状态
setup_completed: true
完成时间：<实际时间>
已阅读：dsvideo-setup/SKILL.md
项目目录：<已确认的绝对路径>
```

该标记只表示首次项目 setup 完成，不代表模型已实测生成或所有工具都可用。失败或中断时不写完成标记；保留已完成的准备工作，下次继续。后续加载主 Skill 读取此标记，已有项目直接继续；用户仍可要求登记新的项目目录。
