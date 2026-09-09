# 媒体模型价格共用模型库

唯一价格数据源仍为 `src/data/modelDatabase.json`。新增 `mediaPricing` 表示按张、按秒计费，保留 `pricing` 表示百万 Token 价格，两者不混用。

2026-09-09 核对并更新：Grok Imagine Video 1.5 / Video，MiniMax H3 / H3 Max，Grok Imagine Image 2.0 / Image / Image Quality，MiniMax image-01 / image-01-live。

来源：[xAI 官方定价](https://docs.x.ai/developers/pricing)、[MiniMax 官方按量计费](https://platform.minimaxi.com/docs/guides/pricing-paygo)。MiniMax 费率为国内人民币计价；代理和国际地址只展示官方参考，不能当作其实际账单。

宿主从现有合并模型库提取媒体费率并注入内置插件运行环境。页面、聊天及内置脚本读取相同数据；原脚本里的独立费率常量已移除。开发环境直接读取仓库模型库。

未知模型、缺少费率、无法查余额均不阻塞出片。报价只作本地参考信息，生成仍由用户点击触发；无报价或报价过期时，点击生成会先补充本地报价状态。模型未收录时显示“以供应商实际计费为准”。不猜测代理费率，不额外要求转到聊天核价。

验证使用模拟供应商，未提交真实付费任务。
