# Dsivio 日报环境准备

本 Skill 随 Dsivio 打包，使用自己的紫鸟/ZClaw 账号及店铺权限。现有采集流程适配 Shopee、TikTok Shop、SHEIN 巴西 BR 站。插件文件安装完成不代表登录或实店采集已经成功。

## 检查并补齐依赖

Python 3.12 与 Node.js 22 随 Dsivio 安装：`dsivio python` 已带 openpyxl，`dsivio node`/`dsivio npm` 使用内置 Node，不需要系统 Python、Node 或 `.venv`。命令在本 Skill 目录执行；HTML/PNG 渲染还需要本目录的 Node 依赖及 Playwright Chromium，另需紫鸟 CLI 和客户端。只补齐缺失项，不重复安装、重新授权或覆盖用户文件。

先用 `dsivio python -c "import openpyxl, sys; print(sys.version)"` 核验。缺少本目录 Node 依赖时执行 `dsivio npm ci`；缺少 Chromium 时执行 `dsivio npm exec -- playwright install chromium`。`dsivio npm` 先走 npmmirror 的 npm 源和 Playwright 浏览器镜像，失败时自动改用官方源重试一次。不要另起 Playwright 浏览器采集店铺，Chromium 仅用于本地日报截图。

仅当 `dsivio python`/`dsivio npm` 报告内置运行时缺失（退出码 127）时才改用宿主环境：在 Skill 下用宿主 Python 3.12+ 执行 `python -m venv .venv`，再用该解释器执行 `-m pip install -r requirements.txt`（Windows 使用 `.venv/Scripts/python.exe`，macOS/Linux 使用 `.venv/bin/python`），后续所有脚本使用已核验的解释器绝对路径；Node 依赖与 Chromium 用宿主 Node.js 22+ 的 `npm ci`、`npx playwright install chromium`。

紫鸟 CLI 缺失时安装采集流程使用的快照 `dsivio npm install -g @ziniao-open/cli@1.1.2`（装到 Dsivio 私有目录 `~/.kivio/npm-global`，不写系统 Node；内置运行时缺失时才用宿主 `npm install -g`）；已安装的 CLI 先检查版本和 doctor，不自动降级。更新采集相关依赖后重新验证。执行 `dsivio python scripts/ziniao_cli.py doctor` 检查客户端、Bridge 和授权；健康时不重新初始化。仅需要认证时，按紫鸟 CLI 的实际提示走 `ziniao-cli config init` 正常交互流程，由用户在本机登录或授权，不读取、记录或复制凭据。

## 首次配置与验收

读取 [通用日报工作流](daily-workflow.md)，列出自己的可见店铺，让用户选择店铺范围、数据目录和可选定时时间。配置、源文件、日报和台账放在用户选择的目录；Skill 安装目录仅放程序和依赖。每份先 plan 校验，再真实采集目标日 Excel、归档哈希、更新台账、渲染并通过 audit，实际查看 HTML/PNG 后才算首轮跑通。

原分享包记录：打包和隔离安装曾有自动测试；来源项目全量回归曾有 3 个 Shopee 交接断言失败。历史测试和其他宿主的运行结果不能代替当前 Dsivio 环境的实店验收。网络、店铺权限、验证码和页面变化按原 Skill 的恢复路径处理。

定时执行复用 Dsivio 当前对话的定时任务工具，具体提示词与规则只维护在 [通用日报工作流](daily-workflow.md)。
