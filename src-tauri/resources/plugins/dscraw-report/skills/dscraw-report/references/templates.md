# 日报模板规范

模板决定展示方式；日报配置决定店铺、指标口径与输出位置；SKILL.md 决定怎么取数。这三部分分别维护。

每份用户数据目录下的 `配置/<日报名>.json` 用 report.outputs 列出需要生成的报告，每项在 template 中选择模板键，报告数量不限于两份：

| 配置值 | 文件 | 用途 |
|---|---|---|
| cards | templates/cards.html | 通用分店卡片 |
| kidswear-cards | templates/kidswear-cards.html | 童装日报已确认版式：紧凑标题、两列卡片、店名同行展示广告指标 |
| table | templates/table.html | 紧凑表格，适合跨店并排比较 |
| operations | templates/operations.html | 内部运营分析，仅 HTML，使用 operations 渲染器 |

kidswear-cards 在标题中分别展示订单数与 SHEIN 销量，广告指标与店名同行。cards/table 也按数量口径分别汇总，不把订单和销量相加。

不同报告可共用模板，也可各自使用不同模板。只修改某份报告的样式时，复制模板并修改该 outputs 项的 template。不要改采集流程来实现版式变化。

新增简版模板必须：

1. 保存为 templates/名称.html，名称不含路径分隔符；在日报配置中指定这个名称。无需增加平台或改渲染脚本。
2. 接收 `__DOCUMENT_TITLE__`、`__REPORT_JSON__`、`__SOURCES__` 三个占位符。report JSON 包含 title、dateIso、dateText、stores、capturedCount、expectedCount、按币种的 salesTotals、按口径的 quantityTotals；不要自己重算混币种合计。
3. 使用通用结果中的 name/currency/orderLabel/collectAds，不写死店名或数量。失败值显示未获取及原因，不转成 0。HTML 包含 [Sources] 来源块。
4. 每家店一个 `.store-row` 元素，页面含 h1；动态渲染 HTML 时转义文本。画布宽 1136，高度自动增长；不得固定三行或隐藏溢出内容。
5. 用 1 家、6 家及超过 6 家的配置试渲染，检查失败状态、广告开关、长名称和多币种；查看生成 PNG，确认无截断或重叠。

模板不包含 storeId 清单、输出目录或平台选择器，也不发起店铺请求。结果字段与生成命令见 SKILL.md 的“数据落盘与正式日报”，模板实现参考现有 HTML 文件。

## 内部运营模板

outputs 项使用 `renderer=operations`、`template=operations`，对应 `templates/operations.html`。统一入口 render_reports.py 按列表调用相应渲染器；运营模板不套用 summary 的 PNG 尺寸和 `.store-row` 契约。通过 `__DATA__` 注入当次数据，标题、日期、币种、来源数量等占位符由脚本替换。可以配置多份不同运营模板，不保存样稿业务数据。运营版使用同一批已核验数据与原始导出，不另行采集店铺。
