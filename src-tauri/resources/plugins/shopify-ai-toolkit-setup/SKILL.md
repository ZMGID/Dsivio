---
name: shopify-ai-toolkit-setup
description: Check Shopify's official store-operation Skills, CLI runtime, and store authorization in Dsivio.
kivio-market-managed: true
---

# 设置 Shopify 店铺运营

市场从 [Shopify/Shopify-AI-Toolkit](https://github.com/Shopify/Shopify-AI-Toolkit) 的固定提交安装 `shopify-use-shopify-cli`、`shopify-admin`、`shopify-shopifyql` 三个官方 Skill 到 `~/.kivio/skills/`。商品与库存操作走 Shopify CLI，销售报表用 ShopifyQL；其余面向应用开发的 Toolkit Skill 不属于这张市场卡片。

1. 确认当前对话能加载上述三个 Skill 及 `shopify-ai-toolkit-setup`。检查每个目录中的 `SKILL.md` 和 `scripts/`；缺少市场文件时通过市场安装或修复，不要手工复制。
2. 在 Dsivio 对话实际使用的环境里检查 `node --version`、`npm --version`、`shopify version`。缺少 CLI 时按 [Shopify 官方安装文档](https://shopify.dev/docs/api/shopify-cli/installation) 安装 `@shopify/cli`，完成后重新检查版本和 `shopify store auth list`。不要把市场“已安装”当成 CLI 已就绪。
3. 用户给出具体的 `*.myshopify.com` 店铺和任务后，按官方 `shopify-use-shopify-cli` Skill 为该任务确定最小权限，再通过 `shopify store auth --store <店铺域名> --scopes <权限>` 走浏览器授权。不要索取或回显访问令牌；没有目标店铺时只报告授权状态，不猜测店铺域名。
4. 读操作用 `shopify store execute --store <店铺域名>`；修改店铺数据时先明确目标和变更，并按 Shopify CLI 的 `--allow-mutations` 规则执行。销售汇总使用 `shopify-shopifyql`，GraphQL 字段和版本用 `shopify-admin` 校验；不要把 Storefront MCP 的买家查询当成卖家管理接口。

上游 Toolkit 的辅助脚本默认会向 Shopify 发送使用数据，可能包括搜索词、校验代码和代理标识。用户需要关闭时，可在运行 Dsivio 的环境设置 `OPT_OUT_INSTRUMENTATION=true`，或按上游说明创建 `~/.config/shopify-ai-toolkit/opt-out` 文件。不要把店铺令牌、顾客信息或订单明细传给这些辅助脚本。

三个 Skill、Node 和 Shopify CLI 均可用，且目标店铺授权已核实后，只把已安装的 `~/.kivio/skills/shopify-ai-toolkit-setup/SKILL.md` 的 YAML `description` 行尾追加一次 ` [setup completed once]`，其余内容不变。未完成不要标记。
