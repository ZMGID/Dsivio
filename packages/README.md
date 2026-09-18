# 应用包

每个包放在自己的目录里，包含：

- INSTALL.md：安装时发送的原文。
- DSIVIO.md：使用时挂载的指导正文。
- market.json：市场介绍、分类和对话示例。
- assets/：Logo 等展示素材，按需添加。
- files/：自制包的完整功能文件，按需添加。

网上现成项目：INSTALL.md 指向项目仓库，由 Agent 阅读说明并安装。

自制项目：完整功能文件随包上传到 files/，INSTALL.md 指向这些文件并说明安装方法。

两种包的安装和使用方式相同，只是来源不同。文档保持简短，密钥由 Agent 通过 kivio_inspect(topic="capabilities") 主动读取。

包放在 GitHub，不进入软件安装包。catalog.json 保存正式发布目录。本机 DSIVIO_MARKET_DRAFT_DIR 可指向单个包或 packages 总目录。

详见 [包规范](../docs/agents/dsivio-package-format.md)。
