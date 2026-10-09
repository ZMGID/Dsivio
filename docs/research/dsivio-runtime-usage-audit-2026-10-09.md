# Dsivio 内置环境及插件使用检查

检查版本：已合并 #3/#4 的 main，`2c3289224d760f4cb0b7eeb47f304669cb1987a7`。
检查使用隔离工作树，其代码与该 main 一致；没有改动主工作区的其他任务代码，没有运行 CI。
这些是新代码的打包能力，不代表用户已经安装的旧版本自动具备这些环境；仍需发布新安装包。

## 已打包内容

| 内容 | 版本 / 用途 |
| --- | --- |
| Python | 3.12.12，独立解释器 |
| Node.js | 22.23.2，随带 npm/npx；本次 macOS 实测 npm 10.9.8 |
| 报表 Python 库 | openpyxl 3.1.5、Pillow 11.3.0、lxml 6.1.3 |
| 文档和网络 Python 库 | pypdf 6.19.0、python-docx 1.2.0、requests 2.34.2，以及锁文件中的其他传递依赖 |
| 媒体工具 | FFmpeg、FFprobe；本次 macOS 实测分别 6.0、n4.4.1；跨平台版本取对应发行二进制 |
| 媒体组件 | yt-dlp 2026.8.19、comfy-mcp 0.10.0、comfy-cli 1.15.0、mcp-video-analyzer 0.10.0 |
| 视频引擎 | Dsvideo 及构建时安装的 Node 依赖 |

证据：`scripts/video-runtime/versions.json`、`requirements.txt`、`package.json`、`scripts/build-video-runtime.mjs`、`src-tauri/tauri.conf.json`。
uv 是构建工具，没有作为通用用户运行环境一起提供。Git/Git Bash、各插件 CLI、所有项目依赖及模型并未全部预装。

## 使用路径

- `dsivio python/node/npm` 通过资源目录的绝对路径运行内置环境；不在 PATH 上挑选系统解释器。缺失时退出 127，不自动下载解释器。
- Dsvideo 使用内置 Node，并明确传入内置 Python、媒体工具和 Python 包路径。
- App 初始化将 npm/npx/python/python3/ffmpeg/ffprobe 启动器及 Node 加入进程 PATH，普通 Agent shell、MCP 子进程继承这个 PATH。
- 裸命令的默认顺序仍是用户系统环境优先，内置环境在后。只对 macOS 缺 CLT 的 Python 占位程序、Windows Store 的 Python 安装别名特殊处理。
- 虾皮调研、商品搜款、日报、PDF/DOCX、1688 等已适配入口明确使用 `dsivio python`；剪映等额外依赖放技能专用 venv。
- 市场在使用前同步 App 自带 setup 文档；改变内容会清除旧 setup 完成标记。主技能的更新还受 catalog revision 和是否市场所有影响。第三方上游技能仍可能使用自己的环境探测和安装流程。

## 仍可能选错或重复准备环境的情况

1. **系统解释器遮住内置解释器（实测确认）**：插件用裸 `python3` 时可选到系统 Python。报表 setup 脚本只检查当前解释器的库；缺库就使用私有 venv/pip 安装分支，未主动查找另一个已齐全的内置 Python。代码：`src-tauri/src/media_runtime/launch.rs:256`、虾皮 `scripts/setup_runtime.py:42`。
2. **上游/未迁移插件安装说明**：例如企业微信仍写裸 npm/npx，Hypit 交由上游安装说明，Remotion 检查项目 Node。系统 Node 优先可能选到旧版。上游安装脚本是否会自行下载解释器，需要对各脚本单独核对，不能由 PATH 集成保证。
3. **shell 重新设置 PATH（代码确认，发生取决于用户配置）**：Agent 常规 shell 是 `sh -c` / Git Bash `-c` / PowerShell NoProfile，通常保留内置路径；但官方安装入口在 macOS 使用 `bash -lc`，Dock 交互终端会读取用户配置。profile 若覆盖 PATH，能丢掉内置目录。系统终端也没有保证继承 App 的 PATH。代码：`src-tauri/src/plugins/install.rs:342`、`src-tauri/src/dock/terminal.rs:283`。
4. **npm 自定义安装目录（代码确认）**：尊重用户 npmrc/参数指定的 prefix，但 App 自动追加的是默认 `~/.kivio/npm-global` 的 bin。自定义 prefix 的 bin 若不在 PATH，可能安装成功后仍探测不到 CLI，造成重复安装判断。
5. **缺库与缺解释器没有完全分开（代码确认）**：报表脚本在内置 Python 存在但包/导入路径受损时，同样进入 venv/pip 分支；更合理的是先提示修复内置包。显式 CLI 本身缺解释器则明确报错，不会自动重装。
6. **已有系统 Playwright CLI**：更新时故意继续使用系统 npm；新安装默认内置 npm。这是兼容现有安装的分支，当前不能说所有插件安装都统一内置。

## 正常的按需下载

内置解释器不等于内置全部插件和依赖。紫鸟/企业微信/Playwright/Hypit 等 CLI、Remotion 项目依赖、剪映专用依赖、渲染用 Chrome、WhisperX 的专用 Python 包和模型仍需按需准备。WhisperX 用内置 Python 创建私有 venv，不是另外下载系统 Python。Dsvideo 的渲染浏览器下载已有镜像钩子，浏览器没有全部随安装包预置。

## 本次验证

- 实际 debug App：Python 3.12.12、Node 22.23.2、npm 10.9.8 正常；openpyxl/PIL/lxml/pypdf/docx/requests 均能导入。
- 同一虾皮环境脚本 `--check`：通过 `dsivio python` 和内置 Python shim 运行均 `ready=true,bundled=true`；通过 `/usr/bin/python3` 运行 `ready=false,bundled=false`，目标指向私有 venv。仅检查，没有执行 pip 或下载。
- 模拟基础 PATH 加内置目录：裸 node/npm 找到内置版本，裸 python3 被已有 `/usr/bin/python3` 遮住，符合代码顺序。
- 前一轮已验证移动后的启动器、无系统 Node/Python 的 App CLI 和 Windows 16 项启动器回归。本次未做 Windows GUI 安装或用户所有第三方插件的全流程验收。

建议下一步：App 托管插件明确选择内置入口，统一环境探测返回版本/路径/来源/库状态；安装后按实际 prefix 校验 CLI；内置资源损坏先修复，不自动视为需要另一套环境。保留用户显式选择的项目 venv 和版本管理器。
