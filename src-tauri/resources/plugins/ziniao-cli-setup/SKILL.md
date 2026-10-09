---
name: ziniao-cli-setup
description: Check the official Ziniao CLI, its Skills and authorization, and complete missing setup steps. Also load it when this integration fails, a dependency or authorization is missing, or Dsivio adaptation needs checking.
kivio-market-managed: true
---

# 设置紫鸟 CLI

本 Skill 只补齐缺失的本机 CLI、官方 Skills 和授权。先复用已有安装与配置，不因再次使用插件而重新安装、重新授权或覆盖用户数据。CLI 与官方 Skills 由紫鸟维护；插件市场只管理 `ziniao-cli` 和 `ziniao-cli-setup` 两个接入 Skill。处理店铺、紫鸟浏览器和本机页面时，按对应的官方 `ziniao-*` Skill 使用 `ziniao-cli`。

## 检查

1. 检查 `command -v ziniao-cli` 和 `ziniao-cli --version`。Windows 用 PowerShell 查询命令。Dsivio 自带 Node.js 22（`dsivio node --version`），并已把它和私有 CLI 目录 `~/.kivio/npm-global` 放在 Dsivio 终端的 PATH 末尾，因此不再要求系统 Node；只有 `dsivio node --version` 报告内置运行时缺失（退出码 127）时，才检查系统 `node -v`（官方 CLI 要求 Node.js 18 或以上）。不要把项目本地或 npx 的临时 Node 当成系统安装。
2. 检查官方 `ziniao-shared`、`ziniao-store`、`ziniao-page` 等所需 Skills 是否可读取。缺失时用 `ziniao-cli skills install --copy` 安装官方 Skills；不要自己编写或复制同名 Skill。
3. 执行 `ziniao-cli doctor`，根据实际报错判断 CLI、紫鸟客户端与授权状态。不要读取或输出 Token、Cookie 或配置文件中的密钥。

## 补齐缺失项

- CLI 缺失时，用 Dsivio 内置 npm 安装紫鸟官方正式版：`dsivio npm install -g @ziniao-open/cli`，再验证 `ziniao-cli --version`。`dsivio npm` 的 `-g` 装到 Dsivio 私有目录 `~/.kivio/npm-global`，不写系统 Node；先走 npmmirror 镜像，失败时自动改用 npm 官方源重试一次。仅当它报告内置运行时缺失（退出码 127）时，才按紫鸟官方 SETUP 用系统 npm 执行 `npm install -g @ziniao-open/cli`。安装、授权与打开授权页都在本机真实环境进行；工具没有足够权限时明确报告，不在隔离环境重复尝试。
- 只有 doctor 指出需要初始化时才执行 `ziniao-cli config init --new`。若紫鸟客户端未登录，先请用户在客户端登录，再继续同一次流程。init 可能等待审批一小时；在能持续读取输出的终端后台启动，一出现含 `memberAuth?cliRequestId=` 的授权链接，立即在本机系统默认浏览器打开，告知用户完成授权并保持 init 进程运行。macOS 用 `open`，Windows PowerShell 用 `Start-Process`，Linux 用 `xdg-open`。打不开时再提供完整链接。不要用无头或沙盒浏览器代替本机授权。
- 授权完成后再执行 `ziniao-cli doctor`。失败或超时按输出报告，不把 CLI 已安装误报成授权成功。不要自动删除已有的 `ziniao-assistant` 或其它用户 Skill。

完成后用简体中文报告实际 CLI 版本、官方 Skills、授权结果和 doctor 结论。验证成功时，只把已安装的 `~/.kivio/skills/ziniao-cli-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`；其它内容保持原样。这个标记只表示曾跑通，后续失败或用户要求复查时仍须重新检查。
