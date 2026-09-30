---
name: davinci-resolve-setup
description: Check Dsivio integration and connect DaVinci Resolve Studio 21.1's official MCP server, then verify with one read-only project query. Also load it when this integration fails, a dependency or authorization is missing, or Dsivio adaptation needs checking.
kivio-market-managed: true
---

# 设置 DaVinci Resolve

只接入 Resolve Studio 自带的 MCP，并做一次只读验证。市场安装 `davinci-resolve` 和 `davinci-resolve-setup` 两个 Skill。已有连接直接复用。如果市场管理的两个 Skill 文件损坏，使用市场的“重新配置”修复。不要安装第三方 Resolve MCP、Python 桥或社区脚本服务。

## 0. 核对 Dsivio 接入

先确认当前对话能加载 `davinci-resolve` 和 `davinci-resolve-setup`。本插件没有命令组件。不要为了凑齐安装而创建空的 MCP 配置。若市场显示“需修复”或 Skill 缺失，使用市场的安装/修复入口恢复，不手工复制到 `~/.agents/skills`。

## 1. 确认 Studio 21.1

在本机查找 DaVinci Resolve。macOS 常见路径是 `/Applications/DaVinci Resolve/DaVinci Resolve.app`。免费版没有 MCP，也没有 File > Setup AI Assistants。版本低于 21.1 时停下来，请用户先把 Studio 更新到 21.1 或更新版本。不要代为下载或激活许可证。

## 2. 读取 Resolve 写出的 MCP 配置

请用户打开 Resolve，选择 File > Setup AI Assistants。Dsivio 不在自动识别名单里。Resolve 会把服务器定义写进它认识的客户端配置，例如 Claude Desktop、Claude Code 或 Codex 的 MCP 配置。

只读取它已经写入的那一条服务器：transport、command、args、url、env、cwd。不要猜测命令、端口或脚本路径。找不到已写入的配置时停下来，把菜单路径告诉用户，等他们完成后再继续。不要把密钥或完整环境值贴进对话。

## 3. 登记到 Dsivio

用 `kivio_configure` 的 `mcp_upsert` 写入这一条。配置 JSON 只含 ChatMcpServer 字段：`name`、`enabled`、`transport`、`url`、`command`、`args`、`env`、`headers`、`cwd`、`enabledTools`。新建时不要传 id；已有同名服务器时用检查到的 id 做局部更新。名称使用 `DaVinci Resolve`，并启用它。写入后按产品要求重新加载 MCP，使新服务器在下一轮可用。

## 4. 只读验证

Resolve 保持打开，并载入一个项目。做一次只读查询，例如列出当前项目的时间线名称。成功的标准是工具返回该项目的真实数据。连接失败、服务器未列出工具、或 Resolve 没打开，都不算完成。不要在验证时渲染、删除、覆盖导出文件或改时间线。

分别报告 Dsivio 组件接入、Resolve 版本是否为 Studio 21.1 及以上、MCP 来自哪份已写入配置，以及只读结果。市场“已安装”只表示 Skill 就位。

完整完成接入和真实只读查询后，只把已安装的 `~/.kivio/skills/davinci-resolve-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`；原有描述和其他内容不变。只要验证未完成，就不要标记。这个标记只表示曾跑通。用户要求重跑、Resolve 未打开或调用失败时仍可重跑，不重复追加标记。
