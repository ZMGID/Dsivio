---
name: dscraw-report
description: "用紫鸟 CLI 抓取 Shopee、TikTok Shop、SHEIN 巴西店铺日报；4家并发、最多6个窗口、Agent 处理异常并补采，保存原始数据、HTML/PNG 和累计 Excel。也用于维护日报配置，不用于账号管理。"
metadata:
  required-binaries: "ziniao-cli, dsivio"
---

# 紫鸟店铺日报

首次使用或环境缺失时，先读取 [环境准备](references/migration.md)，再按通用日报工作流让用户选择店铺、数据目录和可选定时时间。每日任务先完整读取 [通用日报工作流](references/daily-workflow.md)，只执行接收方确认的配置，不预设店铺、类目、数量或目录。异常时按需读 [恢复路径](references/recovery.md)。

采集、登录恢复、并发和补采统一按本文件执行；只有选用或修改报告模板时，再读独立保留的 [模板说明](references/templates.md)。目标是拿到用户指定店铺、指定日期的数据，不是巡检登录状态或生成错误清单。所有浏览器操作默认使用紫鸟 CLI；脚本包装固定动作，当前 Agent 负责处理脚本不能完成的步骤。不要求用户逐店确认。

## 1. 使用什么工具

以下命令在本 Skill 目录执行。`dsivio python` 是随 Dsivio 安装的 Python 3.12（已带 openpyxl），并让脚本内部调用的 `node` 使用内置 Node.js 22，不需要系统 Python/Node；仅当它报告内置运行时缺失（退出码 127）时，才按 [环境准备](references/migration.md) 换成已核验的宿主 Python 3.12+ 绝对路径。scripts/ziniao_cli.py 调用已安装的紫鸟 CLI，不是另一个浏览器工具。Excel 处理需要 openpyxl，报告渲染需要 Node、Playwright/Chromium。

~~~text
dsivio python scripts/ziniao_cli.py doctor
dsivio python scripts/ziniao_cli.py store list
dsivio python scripts/ziniao_cli.py store open --id STORE_ID --expected-name "紫鸟准确店名" --url "该平台入口"
dsivio python scripts/ziniao_cli.py page content --store-id STORE_ID
dsivio python scripts/ziniao_cli.py page query --store-id STORE_ID --selector "现场定位的CSS选择器"
dsivio python scripts/ziniao_cli.py page click --store-id STORE_ID --selector "唯一可见控件的CSS选择器"
dsivio python scripts/ziniao_cli.py page visit --store-id STORE_ID --url "该平台入口"
dsivio python scripts/ziniao_cli.py page exec --store-id STORE_ID --target-id TARGET_ID --script-file "本次只读观察.js的绝对路径"
dsivio python scripts/ziniao_cli.py store close --id STORE_ID
~~~

- 每次任务 doctor 一次。开店、导航、观察、登录按钮、日期选择、导出、下载、关店全部先用上述 CLI；采集脚本内部也走 CLI。不要另建 Playwright/CDP 会话或直调 Bridge 私有接口。
- content 用于确认当前页面与 targetId；业务读数用当前可见 DOM。query 先定位、click 再操作、之后重新观察结果。不要查询密码框值，也不要保存完整页面内嵌脚本或会话 URL。
- 复杂 JavaScript 写成 UTF-8 文件，用 --script-file，别在 PowerShell/CMD 里层层转义。引号错误、无效选择器是调用问题，不是店铺失败。普通 DOM 没找到时扩大到现场实际标签，不能凭一次空查询断言 Shadow DOM。
- click 要检查命中确认，不能只看退出码；成功提示可能在 stderr，visit/close 也可能是纯文本，不强行解析 JSON。参数有疑问先查对应命令 --help。
- **紫鸟 CLI 是从头到尾的主操作工具，computer use 仅为最后手段，不是普通异常分支。** needs_agent 表示当前 Agent 改用现场 CLI 操作，不表示升级到桌面工具；观察器/采集脚本失败不等于紫鸟 CLI 失败。
- 页面未加载完、葡语/中文差异、旧观察器缺字段、选择器失效、菜单需展开或悬停、登录/验证按钮、订单口径未切换、传参报错、导出等待和文件解析异常，都不能单独触发 computer use。不要凭“可能需要悬停”“DOM没找到”就切工具。
- Agent 先在同店用 content 确认当前页面和 targetId，按实际文字/标签用 query 或只读 exec 重新定位，过滤隐藏重复项，展开正确菜单并用 click 操作，再看实际结果。与当前故障相关的加载等待、--script-file 传参修复、--help 中可用的 CLI 交互方式也应尝试；仍有可行 CLI 路径就继续用 CLI，不重复盲点旧选择器。
- 只有已实际尝试相关且可行的 CLI 路径，并有现场证据证明某个必要交互仍无法完成时，才允许最后使用 computer use。切换前简短说明具体控件/动作、尝试过的 CLI 命令及结果、为什么剩余 CLI 路径不可行；“脚本超时”“试了几次”“字段缺失”都不够。未满足条件不加载或探测 computer-use、cua-driver，也不因某个桌面工具不可用再换另一个。
- 最后手段也仅由 Agent 操作当前已确认的店铺窗口，完成该步立即回 CLI 核验和采集；脚本不调用 computer use。全局认证、需要用户提供的凭据或明确拒绝的操作不靠换工具解决，不走旁路规避。

### 数据源优先级（强制）

- **所有平台、所有店铺都必须先导出并归档 Excel，再确定日报数值。** Excel 中经过日期、店铺和指标口径核验的目标日行是主数据源；网页卡片默认只用于确认账号、统计日期、时区、订单口径、刷新状态和导出任务，不得因为网页更容易读取就跳过 Excel。
- 导出成功不等于取数完成：必须打开实际下载文件，核对文件声明的统计日期、工作表、表头和目标日行。正式结果须保留下载文件路径与 SHA-256；`downloads` 为空的页面读数不能直接作为正常成功结果。
- 唯一常规例外是 Shopee 当日/昨日数据延迟：已经先下载并检查 Excel，目标日行显示可疑 `0`，且同一店铺、同一目标日、GMT-03、订单类型“已下订单”的页面明确显示“更新中/Atualizando”时，才采用网页实时销售额和订单数覆盖文件中的可疑零值。此时同时保留 Excel 零值、网页实时值和更新中状态，来源标记 `single-day-page-realtime-updating`。页面也为 0 时可以确认真实零值，不得自行改成非零。
- TikTok Shop 与 SHEIN 不套用 Shopee 的“更新中网页覆盖”例外；它们以下载文件的目标日数据为准。网页数值可作交叉核验，但不能替代已要求的 Excel 导出。

## 2. 确定范围后直接采集，不做开关店巡检

一份日报配置保存在用户选择的数据目录的 `配置/<日报名>.json`，保存 stores、requirements、report.outputs、outputRoot 和 lastRun。先读取 `~/.kivio/dscraw-report/REPORTS.md` 中已登记的配置路径；没有配置时按工作流完成首次选择，已有配置直接复用，不每天重问。配置和经营数据不写入 Skill 安装目录。用户说全部店铺时核对实际范围，不能默认缩成旧童装六店；只支持 Shopee/TikTok/SHEIN 的 BR 站点，其他平台明确排除。按业务类目与用户顺序展示，不按平台重分组。

~~~text
dsivio python scripts/report_state.py plan --config "日报配置的绝对路径" --date YYYY-MM-DD
dsivio python scripts/collect_batch.py run --config "日报配置的绝对路径" --date YYYY-MM-DD
~~~

Dsivio 插件统一使用 --config 指定外部配置；--report 仅兼容旧 Skill 目录内配置，与 --config 二选一。没有日期时按配置时区取前一自然日；把 plan 的 dateIso、configSnapshot 和输出路径用于整个任务，跨午夜也不改变。有多份配置时按用户范围选择，不能任意取第一份。新增日报按现有 JSON 结构建配置并 plan 校验，不复制另一日报的 lastRun；不擅改已有店铺、广告开关或数量口径。

- run 放在可持续读输出的后台进程。最多4家启动检查/采集，最多6个本批次窗口（采集、预载、异常现场共用；无异常时4家采集、另2家预载）；不要另起同批次进程或额外窗口突破上限。先确认没有其他任务占用同店，不关其他任务的窗口。
- 正常页自动标记 startup=normal 并采集，正常完成后关店、补开下一家。不要先把全部店打开查账号再全部关闭；开了可用店就立即取数。
- expectedAccount 为空不等于未登录。脚本因此交回时，Agent 在这家已核对 storeId/expectedName 的窗口读取页面账号，立刻 ready --account 续采，不猜账号、不另做全店映射巡检、不关店重开。
- Agent 在后台运行期间查看增量输出及 status（有任务运行时约20～30秒检查一次），处理 needs_agent；不能只等待整批脚本退出。needs_agent 只是交接信号，不是已经恢复，更不是最终失败。只操作已交接的店，不抢 opening/ready/collecting 的店；额外补导出同样遵守最多4家采集。

## 3. Agent 接手异常：看页面、点按钮、接着取数

遇到 needs_agent、非零退出或超时，先读本店 reason、capture/checkpoint/handoff，再用 CLI 看这家当前页面。记录做了什么及操作结果，不把脚本错误原文直接写成最终未获取。

- **登录页：** 按下面各平台入口用 CLI 点击，等待已有浏览器登录流程完成。普通“登录”“使用邮箱登录”“主/子账号登录”按钮可操作，不因看到登录页就判外部认证阻塞。不读取、输出、填写密码或验证码；页面真正要求新的凭据、验证码或本人操作时才说明具体阻塞，不猜测或绕过验证。
- **加载/网络：** 同店等待后重读；ERR_SOCKS_CONNECTION_FAILED 在同店检查、必要时重试原地址，不换店补值。
- **控件/语言变化：** 按现场可见文字和实际字段定位，切对页面或订单口径；葡语页面或某标签缺失不是“数据不存在”。脚本不兼容时，Agent 用 CLI 完成剩余固定步骤、解析本次文件并记录映射，不反复运行同一错误。
- **导出处理中：** 查看原任务和本店下载目录，等原文件或下载原任务。一次等待结束不代表导出失败，不重复提交；不要关掉仍在下载的窗口。
- **文件/页面差异：** 先保留并解析下载原件，以文件目标日行为主；核对当前店铺的目标单日与同一口径，不把七天合计填日报，不为调平改原值。只有符合第1节“数据源优先级”的 Shopee 更新中条件，才用同日网页实时值覆盖文件可疑零值。

修复后把页面留在待续跑步骤，发 ready；账号或下载目录缺失时只针对该店补 --account 或 --download-dir。下载目录取本次 store open 返回的 downloadFolderPath，不猜路径。

~~~text
dsivio python scripts/collect_batch.py status --batch "batch.json绝对路径"
dsivio python scripts/collect_batch.py ready --batch "batch.json绝对路径" --store-id STORE_ID --account "现场核对的页面账号"
~~~

运行中的调度器会接收 ready；已经退出时用 run --batch "原batch.json绝对路径" 续跑。batch 位于 outputRoot/数据/dateIso/批量采集/batch.json。保留原日期、目录、归档和断点，不删锁抢进程、不换目录重导。中断后先确认旧进程已退出，再处理遗留锁和现场。

**难店放最后，不丢掉：** 一轮做几次有意义的 CLI 恢复仍无进展，可以先暂缓；次数只帮助决定是否后置，不触发 computer use，也不判最终失败。保存尝试及原导出状态，确认关窗不会打断下载或丢失未保存数据后，用 defer 关店释放窗口。正常店继续。正常队列结束、原 run 退出后，Agent 必须主动安排延迟补采，不等用户再催；默认等待 600 秒再只重开 deferred 店：

~~~text
dsivio python scripts/collect_batch.py defer --batch "batch.json绝对路径" --store-id STORE_ID
dsivio python scripts/collect_batch.py run --batch "batch.json绝对路径" --retry-deferred --retry-deferred-after 600
~~~

延迟计划写入原 batch，进程在等待期间中断后再次执行同一命令只等待剩余时间。补采只重开 deferred 店，不重跑 done 店；仍异常就按新现场继续 CLI 恢复，不能把一次补采失败当最终失败。继续按“有意义恢复→defer→等待600秒→只重开失败店”的循环处理，直到成功，或出现新的凭据、验证码、权限等真实外部阻塞；只有满足第1节的最后手段条件才用 computer use。不要把暂缓、等待或 needs_agent 当完成。close --batch ... --store-id ... 仅用于已由 Agent 验收取数或最终确认不可继续的异常店，不用于临时搁置。关闭未确认不释放名额；全局 Bridge/认证故障先处理，不能逐店跳过。若最终仍无法补齐，交付已取得的数据与具体缺项，不声称全部成功。

## 4. TikTok Shop

入口：https://seller-br.tiktok.com/compass/data-overview?shop_region=BR

1. 登录页若显示“使用邮箱登录”，先 query 定位该文字（包括 span），再 click；已观察过的候选是 #TikTok_Ads_SSO_Login_Email_Panel_Button，必须现场确认可见。切换后操作可用的“登录”按钮，等待浏览器正常登录；不是先宣布认证失败。登录落在 homepage 时，用 CLI visit 回上面概览。
2. 数据概览选择“最近7天”。日期控件候选 [data-tid='m4b_date_picker_range_picker']，展开后按文字定位。读取 input[placeholder='开始日期'] 与 input[placeholder='结束日期'] 的实际日期，须覆盖目标日；快捷项可能含今天，“对比日期”不是统计日期。
3. 在采用任何页面指标前，先用关键指标 [data-testid='export-button'] 导出一次 Excel。先查自动下载；没有时用 [data-testid='export-history-button'] 查看本次记录，在 .pcm-ae-record-item 找对应任务再下载，不取历史第一项或重复提交。
4. 打开实际下载的 Excel，按文件声明的分析区间和“每日数据”取目标行：GMV→sales，订单数→orders，并另存商品成交件数；不把商品成交件数、SKU订单数或对比日期误作订单数/统计日期，也不取七天合计。金额可能整数/小数分行；工作表声明范围 A1 但有数据时用 openpyxl reset_dimensions 后读取。重复/缺失日期或总计冲突，Agent 用 CLI 切目标单日、重新导出并核对两个日期框；TikTok 网页卡片只作交叉核验，不能替代 Excel。

5. 仅 `collectAds=true` 的 TikTok 店继续进入“营销 → 店铺广告”的 GMV Max 数据面板；使用 `scripts/tiktok_ads.py`，普通店不采广告。确认同一页面账号、BRL、UTC-03及目标单日起止日期后，导出概览 Excel（`Campaign overview data YYYYMMDD - YYYYMMDD.xlsx`）；24小时的“成本”合计与总计一致才写入 adCost，ROI取文件总计并核对总收入/成本。不能取预算、净成本、充值或退款广告费，不相加小时ROI；广告总收入/SKU订单不替代经营销售额/订单数。以“广告组数据”归档并保留独立广告来源。已完成经营时只补广告，原导出等待期间不重复提交；日期/控件变化由 Agent 用 CLI 恢复。

正常固定步骤用脚本；直接调用只用于接手单店，不与批量线程同店并跑：

~~~text
dsivio python scripts/collect_tiktok.py --store-id STORE_ID --expected-name "紫鸟准确店名" --expected-account "页面账号" --date YYYY-MM-DD --download-dir "本店下载目录" --output-root "本次输出根目录"
~~~

## 5. Shopee

经营入口：https://seller.shopee.com.br/datacenter/overview

1. 普通登录页有 Login with Main/Sub Account 时，CLI 定位并点击后检查跳转；有“进行验证”且可见可用时点击并查看结果，候选 .eds-button--primary.ios-action。不要因按钮叫验证就直接停。若后续确需新凭据或本人操作，再记录具体阻塞。
2. 核对页面账号、BRL/GMT-03、订单类型“已下订单”；当前是“已付款订单”就用 CLI 切换，不因选错口径直接弃店。缺少 Local Seller 字样本身不算身份失败。
3. .bi-date-input.track-click-open-time-selector 打开日期，按实际文字选“过去7天”，检查真实起止日覆盖目标日；在读取日报金额前，点击 button.track-click-normal-export 一次导出经营 Excel并等待实际文件下载完成。
4. 打开下载的 Excel，按工作表/表头确认“已下订单”的目标日每日行：销售额(BRL)→sales，订单数→orders；不取扣除补贴销售额、已付款订单、页面默认“今日实时”或七天汇总。葡语文件按实际字段语义核对（如 Pedido Feito），不因中文解析器报错就丢文件。仅当 Excel 目标日行是可疑0，才用 CLI 切到同一目标单日复核；账号、GMT-03、已下订单口径和“更新中/Atualizando”均明确时，采用页面有效实时值并记录 `single-day-page-realtime-updating`，同时保留 Excel 零值证据。若页面同样为0，则按真实0记录；“-”仍不是0。
5. 仅 collectAds=true 进入 https://seller.shopee.com.br/portal/marketing/pas/index 。独立选目标单日，核对日期控件、GMT-3及 from/to；[data-testid='export-data-dropdown-trigger'] 打开菜单，按名称选“广告组数据”，核对确认框后提交一次，匹配本次任务再下载。CSV区间汇总不能填成七天中某一天；广告组与商品行不能重复相加。层级/全店覆盖无法核实时，Agent 读同一目标单日页面“花费/广告支出回报率”，记录 single-day-page-fallback；广告销售额不混入经营，ROAS不求和或平均。

~~~text
dsivio python scripts/collect_shopee.py --store-id STORE_ID --expected-name "紫鸟准确店名" --expected-account "页面账号" --date YYYY-MM-DD --download-dir "本店下载目录" --output-root "本次输出根目录"
~~~

需要广告时才追加 --collect-ads。脚本会核验当前“昨天”；若目标日不是它，Agent 用 CLI 选准确目标日并核验，不反复重跑错日期。经营完成后广告中断，复用原经营归档；恢复时留在待继续的页面，不重导经营。

## 6. SHEIN

入口：https://sellerhub.shein.com/#/sbn/managementAnalysis/trade

1. 若跳到经营概览，CLI 按可见文字点“交易概览”（候选 .merchant-ui-tabs-panel-title span），检查实际页面，不只看传入URL。登录页走本店已有正常登录入口，按第3节处理。
2. 选唯一可见“近7天”，读取“统计期间”的真实起止日期、账号、站点/BRL及刷新证据；站点可为 shein-all，时区来自BR配置时如实记录。不要点隐藏页签同名按钮。页面布局变化或缺少旧版更新时间标签时，Agent 核对实际加载与日期后继续，不把观察器缺字段当数据缺失。
3. 在采用任何页面指标前，依次切 GMV、销量、支付订单数对应卡片，确认趋势图标题后各导出一次并等待实际文件完成。部分版本卡片名是“新客GMV/新客销量/新客支付订单数”，但文件同时含总量和新客列，日报必须取总量。三份都可能叫“交易概览趋势图.xlsx”，按表头确认类型，不按重名编号猜。
4. 打开三份 Excel，按目标统计日期合并：GMV→sales；orderMetric=unitsSold取销量，paidOrders取支付订单数。两种原值都保留，不能混同。期间合计差异先保留，Agent 用 CLI 切准确目标单日交叉核验，不因几分钱差异弃店、不调平原值；SHEIN 页面卡片不能替代 Excel。无广告采集。

~~~text
dsivio python scripts/collect_shein.py --store-id STORE_ID --expected-name "紫鸟准确店名" --expected-account "页面账号" --order-metric unitsSold --date YYYY-MM-DD --download-dir "本店下载目录" --output-root "本次输出根目录"
~~~

--order-metric 必须按本店配置选择 unitsSold 或 paidOrders。金额 BRL 2,089.22 与 Shopee 的1.643,39格式不同，按实际字段解析；空白、-、解析失败不填0。

## 7. 数据落盘与正式日报

单店脚本只产候选 capture.json、checkpoint.json、handoff.json，位于 outputRoot/数据/dateIso/{TK采集|Shopee采集|SHEIN采集}/storeId/。collected/done 不是正式日报完成。Agent 核验候选和补采结果，汇总到 plan 返回的正式 JSON 路径：保留 dateIso、完整 configSnapshot、downloads、sources，stores 每个启用 storeId 恰好一条。

- 每店结果用 storeId、name、sales（无千分位小数文本）、orders（非负整数），可选 adCost、roi；未取得仅对应字段为 null，注明 missingReason/adMissingReason，保留其他有效值。来源写实际账号、日期、时区、币种、刷新依据、文件/页面读数及恢复证据，不放凭据。
- 合并脚本 capture 的下载清单，保留归档路径与 SHA-256；不要把 batch.json 当正式结果。Agent 用 CLI 下载的文件确认来源、日期、可打开后立即归档：
- 对正常成功店铺，正式 JSON 的 `downloads`/下载清单必须包含本次实际核验的 Excel；不能以空下载清单加网页读数结束。Shopee 使用更新中网页实时值时，也必须先归档产生可疑0的 Excel，并在 `sources` 中同时说明 Excel 值、页面值、目标日期和更新中状态。

~~~text
dsivio python scripts/download_files.py REPORT_JSON STORE_ID "下载文件" --download-dir "本店下载目录" --type "数据类型" --start YYYY-MM-DD --end YYYY-MM-DD
~~~

--type 的值：TikTok 用“关键指标”；Shopee 用“店铺经营”或“广告组数据”；SHEIN 用“GMV”“销量”或“支付订单数”。此脚本复制归档、核对哈希后删除本次中转原件；已归档的直接复用。导出前保留本店目录基线，不只凭“最新文件”判断归属。下载/生成中的原任务继续等，不能重提。

- 七天已核验逐日数据保留在 dailyRecords：dateIso、sales、orders（真实订单数）、unitsSold（件数）、source及可用广告字段。目标日顶层orders按配置展示，SHEIN额外保留paidOrders。广告对象写adRecords时用真实adId、adType、metricBasis，不猜ID；预算/目标ROAS是设置快照，不倒填历史或从花费反推，不改变广告设置。
- 日报配置 report.outputs 决定所有报告及位置；summary模板 cards/kidswear-cards/table 支持HTML和PNG，operations仅HTML。不要固定报告数量。不同币种分别汇总，订单与销量分别标明，台账不混口径。日常只累计本次窗口，不主动补历史缺口。
- 正常队列和暂缓补采处理完后，依次执行（REPORT_JSON 换实际路径，各步骤使用同一份 --config）：

~~~text
dsivio python scripts/assemble_report.py --config "日报配置的绝对路径" --date YYYY-MM-DD
dsivio python scripts/report_state.py prepare REPORT_JSON --config "日报配置的绝对路径"
dsivio python scripts/history_ledger.py REPORT_JSON
dsivio python scripts/render_reports.py REPORT_JSON
dsivio python scripts/report_state.py audit REPORT_JSON --config "日报配置的绝对路径"
dsivio python scripts/report_state.py complete REPORT_JSON
~~~

assemble_report 从已验收候选组装正式 JSON，不采集浏览器；仅在候选已完整核验、或补采后需要更新正式数据时执行。已审计通过且来源未变的同日正式 JSON 直接复用，不因 batch 顶层残留 needs_agent 或历史清理待办而重新采集。assemble_report 支持 --config；Agent 手工补齐的正式 JSON 不盲目覆盖，先核对来源与候选一致。

history_ledger 维护总台账及每店长期Excel，按日期+storeId更新，不每天另建台账。render_reports 生成outputs全部文件；经 `dsivio python` 运行时自动使用内置 Node，改用宿主 Python 且 Node 不在PATH时加 --node 绝对路径。`audit` 是交付前只读闸门，会重新核对正式 JSON 路径/业务日期、每个成功店铺的目标日每日行、平台必需 Excel 清单、归档 SHA-256、TikTok/SHEIN 数量口径、累计台账、每店台账、HTML 日期与 `[object Object]`、PNG 文件头和尺寸；任一项不一致就回到对应步骤修正，不能继续报完成。`complete` 会自动再运行同一审计，然后才清理超过三天的原始下载并更新 lastRun。日报及台账长期保留；同日正式文件覆盖，不另建“修复版2”。

所选日报全部生成后，按通用工作流重新 audit 每份正式 JSON，复算销售额、广告费、覆盖数和分口径数量，实际打开全部 PNG 检查店名、日期与布局。可选汇总须事先确认范围、同日同币种，不能混合订单与销量；明细变化后重建并重审受影响汇总。

完成状态区分业务与维护：lastRun.status 根据店铺数据状态判定，缺项仍是部分完成；cleanupStatus 单独记录历史清理是否完成，cleanup.pending 保留原因。清理路径越界时不强删、不重抓正常店，不把“维护待处理”解释成数据失败；交付时如实附维护提示。

交付以拿到的数据、覆盖店数和文件为主。暂缓店未回收处理不能结束；部分完成必须明确，不能拿“所有店打开过”当完成。消息投递且delivery=png_only时只发实际PNG的MEDIA行；本地任务提供配置要求的全部报告链接。业务配置、原始文件、台账、临时脚本与会话信息不提交到Skill仓库。
