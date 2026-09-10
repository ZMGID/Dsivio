# 帮我安装并初始化紫鸟开放平台 CLI（ziniao-cli），然后跑 doctor。

正式环境（npm `latest`）。

## 硬性约束

- 安装 CLI、写授权配置、打开授权页，都必须在本机真实环境执行。先申请离开沙盒（unsandboxed / 完整权限），等用户批准后再跑。
- 用户拒绝离开沙盒：立刻停止并说明原因。禁止在沙盒内改用 `npm i -g` / `config init` 重试。
- 禁止只用无头、内置或沙盒浏览器打开授权链接，也禁止只把链接贴出来等我自己点。
- Windows 用 PowerShell，不要用 Git Bash。
- 后续处理紫鸟店铺、紫鸟浏览器和本机页面自动化时，一律用 `ziniao-cli`。用户没有明确要求换命令时，不要改用其它 CLI。

## 本机环境

先在沙盒外检查并准备：

1. 判断 OS（macOS / Windows / Linux），后面打开授权页必须用对应命令。
2. `node -v`。未安装或无法使用，先在沙盒外安装系统级 Node.js（≥18），不要用项目本地 / npx 临时 Node 冒充。

## 步骤

1. 沙盒外：`npm install -g @ziniao-open/cli`
2. 沙盒外：`ziniao-cli --version`。失败则排除沙盒 PATH / 装到沙盒 npm 前缀，修好后再继续。
3. 沙盒外：`ziniao-cli skills install --copy`
4. 沙盒外检查并卸载 `ziniao-assistant`（有则删，没有就跳过）。只查你自己实际会读取的 skill 目录（用户级、项目级、本次会话已加载的 skill 根），不要扫一份写死的全局路径。目录名为 `ziniao-assistant` 或 SKILL.md 的 `name` 是它，就删除；是符号链接则删链接本身，不要追到仓库源码。不要用 `npx skills add` 重装它。
5. 沙盒外启动紫鸟客户端（只做一次，不要当成已登录）：
   - 当前 Shell 能解析到 `ziniao` 时，执行一次 `ziniao start`
   - 找不到 `ziniao`：提示我手动启动紫鸟客户端
   - macOS 上 `ziniao start` 失败或没有反应，且存在 `/Applications/ziniao.app`：再执行一次 `open -a "/Applications/ziniao.app"`
   - `ziniao start` 成功返回只表示进程已拉起，不等于窗口已登录
6. 沙盒外后台执行：`ziniao-cli config init --new`（会阻塞直到审核结束或超时 1 小时）。不要前台卡住后才去找链接。
7. 一旦 stderr/stdout 出现授权链接（含 `memberAuth?cliRequestId=`）：
   - 同一非沙盒环境、立刻用系统默认浏览器打开，不要先问我：
     - macOS：`open "<url>"`
     - Windows PowerShell：`Start-Process "<url>"`
     - Linux：`xdg-open "<url>"`
   - 打不开再发一条醒目、可点击的完整 URL，并说明已尝试拉起失败。
8. 打开成功后提醒我：到已打开的页面完成授权/审批，不要关 init 进程。
9. 若 `config init` 报「紫鸟客户端未登录」：
   - 暂停，提示我先在已打开的紫鸟窗口完成登录
   - 等我确认后再重新执行 `config init --new`
   - 不要再执行 `ziniao start`
10. 等 `config init --new` 结束（通过 / 拒绝 / 超时）后再执行：`ziniao-cli doctor`
11. 用简体中文汇报：CLI 版本、Skills 是否装上、是否发现并移除 `ziniao-assistant`、init 结果、doctor 结论。
