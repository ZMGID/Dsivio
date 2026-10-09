# Dsivio 内置环境与近期 PR 审查

日期：2026-10-09。目标：普通用户安装后无需自行安装基础开发环境，常用能力直接复用 Dsivio 内置运行时和依赖。

范围修正：按用户后续说明，WebView2 不纳入本次优化；无 VPN 只是背景情况，不作为主要设计目标或验收门槛，不扩展成完整离线分发方案。

这是现状调查和改进建议，不是新增工程规范。遵循 [统一工程规范](../engineering-standards.md)，本地 ASR 生命周期继续遵循 [ADR 0010](../adr/0010-media-gateway-describes-models-and-owns-local-asr.md)。本次未修改产品代码、合并 PR 或进行 Windows 安装验收。

## 结论

可以复用已有 `video-runtime`，不需要再建另一套运行时。主分支已打包 Python 3.12.12、Node 22.23.2、npm、FFmpeg/FFprobe 及媒体工具，但这些环境尚未完整接入普通 Skill 和通用终端。内置了文件不等于用户实际调用到了它们。

#3、#4 正在补齐入口和常用依赖，方向符合目标；它们尚未合并，且仍有重复执行、配置差异和缺 Git 等问题。优先解决基础环境缺失与调用稳定性，保留现有镜像能力即可，暂不扩展网络分发工作。

## PR 状态与建议

查询时 #1、#3、#4、#5 均为 OPEN，最新 `quality` 检查成功。PR 正文中的“CI 未等待”是提交时说明，不是当前检查状态。现有质量工作流只在 macOS 上运行，成功不能代替 Windows 安装测试。

| PR | 改动 | 审查意见 |
| --- | --- | --- |
| [#1](https://github.com/ZMGID/Dsivio/pull/1) | CI/Release 使用 Node 22 | 合理；只改变构建环境，不直接解决用户电脑缺 Node。 |
| [#2](https://github.com/ZMGID/Dsivio/pull/2) | 内嵌 WebView2 bootstrapper | 已关闭、未合并；按用户明确要求，不纳入后续工作。 |
| [#3](https://github.com/ZMGID/Dsivio/pull/3) | `dsivio python/node/npm`、出表依赖、CLI 私有安装目录、npm/Playwright 镜像 | 核心改进，建议修复下述问题后落地。审查 head：`69f5da46`。 |
| [#4](https://github.com/ZMGID/Dsivio/pull/4) | 通用命令启动器、Python 占位程序处理、更多 Skill 迁移、渲染 Chrome 镜像 | 依赖 #3；不能先于 #3 合并。审查 head：`ab366ae5`。 |
| [#5](https://github.com/ZMGID/Dsivio/pull/5) | 构建测试临时目录增加原子计数器 | 改动范围合理；本机单独编译运行该文件的两个测试，均通过。 |

## 合并前需要修复的问题

### P1：npm exec/x 在命令失败后重复执行

#3 的 `src-tauri/src/media_runtime/launch.rs:28` 和 #4 的 `scripts/video-runtime/shim.rs:28` 将 `exec`、`x` 放进允许换源重试的列表。任何非零退出码都会触发完整命令重跑，无法判断失败发生在下载阶段还是已经执行了业务命令。

本机从 #4 提取原样启动器，用已有内置 Node/npm 编译运行。执行 `npm exec -c '<内置 node> <本地脚本>'`，脚本每次写一行记录后退出 7。结果：记录两行、最终退出 7，启动器输出“改用 npm 官方源重试一次”。该复现命令不需要下载包。

后果：可能重复改文件、生成内容或调用外部服务。#4 已让 `npx` 只执行一次，但同义的 `npm exec` 仍重试；[npm 官方文档](https://docs.npmjs.com/cli/npm-exec/) 明确它执行包内命令。

建议：`exec`/`x` 与 `npx` 一致，不自动重跑业务命令。若要提供下载回退，先完成独立的包获取，再执行命令一次。其他安装命令的生命周期脚本也不能一概视为“纯下载”。

### P2：两个 npm 入口的配置规则不一致

#3/#4 的 `launch.rs:117` 只检查参数和环境变量中的 registry，没有检查 `.npmrc`；`launch.rs:202` 除显式 `--prefix` 外强制设置私有 prefix。#4 的终端启动器识别部分 `.npmrc` registry，并保留环境变量 prefix，但仍不识别 `.npmrc` prefix。

本机复现：把一个临时 userconfig 的 prefix 指向临时目录，运行 #4 启动器 `npm prefix -g`，实际仍返回 `~/.kivio/npm-global`。因此正文中“用户通过 .npmrc 指定 registry/prefix 一律不改动”未完全实现。

建议：明确 App 内部安装器固定使用私有目录的规则；公开的 `dsivio npm` 与终端 npm 共用配置解析与换源规则，覆盖参数、环境变量、用户配置和项目配置。保持实现单一，避免用两套常量和互相对照测试长期维持一致性。

## 仍需考虑的环境依赖

| 使用场景 | 证据与剩余依赖 | 建议 |
| --- | --- | --- |
| Git 来源插件安装 | `src-tauri/src/plugins/packages.rs:794` 的 `git_command` 直接启动系统 `git`；GitHub marketplace 来源使用 clone/fetch。#3/#4 未补 Git。 | 需要解决没有系统 Git 时的插件安装：可复用发行归档，或提供私有 Git，按实际调用需要选择。 |
| 第三方 Python 插件 | #4 的 SRT 白板、剪映 setup 使用内置 Python 创建 venv，但额外依赖仍通过 pip 从 PyPI 下载。 | 高频轻量依赖可预打包，低频大依赖按需安装到专用 venv，无需用户先装 Python。 |
| 本地 WhisperX | `media_generation/local_asr.rs` 已用内置 Python，但仍 pip 安装专用依赖，再下载 ASR/alignment/NLTK 模型。 | 准备流程继续归现有 ASR owner，依赖和模型按需下载，不在插件里再建安装器。 |
| 日报 HTML/PNG | #3 已内置 openpyxl，但仍需 `dsivio npm ci` 和 Playwright Chromium 准备。 | 常用日报可预打包锁定 Node 依赖；评估随包浏览器或首次一键下载。不要未经版本兼容验证就共用另一 Provider 的浏览器。 |
| 视频渲染 Chrome | #4 改为 npmmirror 优先，失败回退官方源；浏览器仍不是随安装包交付。 | 保留现有按需下载和镜像逻辑，验证用户无需先安装开发环境。 |
| OfficeCLI、外部浏览器等 | 部分安装路径仍用 GitHub/raw.githubusercontent.com 官方脚本或独立应用安装器。 | 按插件实际使用范围检查所需环境，不要求本次解决全部外部应用的安装。 |

## 建议的交付范围

1. **基础安装包**：Python、Node/npm、FFmpeg/FFprobe、openpyxl/Pillow/lxml、pypdf/python-docx，以及当前常用功能必需的轻量依赖。#3/#4 可作为基础。内置 Skill 显式使用 `dsivio python/node/npm`，避免用户已有旧运行时或缺库的系统 Python 干扰。
2. **首次按需准备**：浏览器、少用插件的大型依赖、本地 ASR 模型。App 展示下载大小、进度、取消和重试；无需让普通用户手工配 PATH、装 Python 或复制 pip 命令。
3. **调用稳定性**：修复命令失败后重复执行、npm 配置处理差异，解决实际需要的 Git 依赖。保留已有镜像实现，不新增完整国内分发或离线资源包。

不建议把 Torch/CUDA/全部模型及每个第三方插件依赖都塞进基础包。具体新增体积需要两个平台实际打包对比；PR #3/#4 的体积数据是估算，不是本次测量。当前支持的发行目标是 macOS Apple Silicon 和 Windows x64。

## 验收建议

- 干净 Windows 10/11：无 Node/Python/Git、Python Store 别名开启；从安装到常用出表、日报、浏览器插件准备和视频渲染走完整路径。
- 干净 Mac：无 Homebrew/Command Line Tools，从 Finder 启动，不弹 Python/CLT 安装框。
- 已有旧 Python/Node：内置 Skill 使用锁定运行时；用户自己使用终端时保留预期配置和 venv 行为。
- 下载失败、中断、重启、软件升级：恢复准备，不重复执行业务命令，不写 App 安装目录，不依赖手动修复环境。

本次验证：读取近期五个 PR diff 和关键调用路径；查询当前 GitHub 检查；真实编译运行 #4 启动器复现两项行为；单独运行 #5 资源测试。未运行完整 #3/#4 应用构建、Windows 实机测试或全功能离线验收。
