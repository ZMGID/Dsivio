---
name: shopee-research-setup
description: 检查虾皮商品调研的 CUA 和本地 Excel 工具，首次使用或环境缺失时加载。
kivio-market-managed: true
---

# 设置虾皮商品调研

主 skill 随 Dsivio 安装到 `~/.kivio/skills/shopee-research`。设置只检查运行环境；商品、站点和登录在实际调研时确认。

1. 确认 `shopee-research` 及 `references/report-format.md` 已安装，缺失时使用市场修复。
2. 加载 `cua-driver`，按实际工具定义调用 CUA MCP 窗口列表，确认工具可用。缺失时引导用户到“设置 → 电脑控制”启用 CUA 并完成系统授权。浏览器操作规则统一遵循主 skill。
3. 检查本地XLSX生成工具及图片脚本依赖：Python的`Pillow`、`lxml`。缺失时补齐到独立环境并记录所用命令路径，表格与图片处理按主skill的报表规范执行。
4. 上述检查通过后，在已安装的本 setup 文件 YAML `description` 行尾追加一次 ` [setup completed once]`。随后加载 `shopee-research`：有商品请求就继续调研，否则简要介绍并提示发送商品。首次站点和结果目录由主 skill 配置。
