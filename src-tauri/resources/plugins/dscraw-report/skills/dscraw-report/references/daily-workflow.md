# 通用日报工作流

本内置插件不预设任何真实店铺、类目、店数、输出盘符或任务。使用现有 Shopee、TikTok Shop、SHEIN 巴西 BR 站采集流程。

## 首次配置

先读 migration.md 检查环境与登录，再读取 `~/.kivio/dscraw-report/REPORTS.md` 已登记的配置（Windows 的 `~` 为当前用户目录）。已有配置和明确任务直接复用；新建日报时，AI 通过 CLI doctor、store list 获取用户可见店铺，只列出候选，不自动把全部店加入日报。让用户一次选择：哪些店铺、分几份日报、执行顺序、是否采广告、SHEIN 按销量或支付订单数展示、数据保存目录，是否需要定时及其时区/时间。用户已经给出的信息不重复询问；缺少店铺范围或数据目录时先等待选择，不代选目录或启用示例店铺。

以 examples/日报配置示例.json 为结构，每份保存到 `<用户选择的数据目录>/配置/<日报名>.json`，`report.outputRoot` 设置为 `<用户选择的数据目录>/<日报名>` 的绝对路径。配置、采集状态、日报和台账均在该数据目录内，不写到插件或 Skill 安装目录。将日报名称、配置绝对路径与用户约定顺序登记到 `~/.kivio/dscraw-report/REPORTS.md`，合并已有登记，不删除其他日报；此文件只登记路径，不保存凭据或经营数据。后续对话从登记中找配置，多份时按用户要求选择。

示例中的三店为虚构且默认禁用，不能直接采集。使用现场真实 storeId、准确 expectedName；expectedAccount 可先空白，由 Agent 在对应店窗读取后 ready，不猜账号。只保留用户选定店铺，enabled=true；每份日报使用独立的绝对 outputRoot、台账文件名和 outputs。BRL、America/Sao_Paulo、dateUtcOffsetMinutes=-180；广告默认关闭，确认后仅 Shopee/TikTok 可开。SHEIN 保留销量与支付订单数两种原值。新配置 lastRun=null，已有配置保留运行状态。不得放入凭据。

首次新建日报时读取 [模板说明](templates.md)，告诉用户内置四套模板及输出格式，让用户选择；没有版式偏好时采用示例的「通用卡片 cards（HTML+PNG）＋内部运营 operations（HTML）」组合，并说明这个默认。将选择保存到 report.outputs，后续每日复用，不重复询问；用户已有模板配置或已指定版式时直接沿用。模板不改变店铺范围和取数口径。

## 每日执行

1. 读取接收方选定的 reports 配置，确认没有同店采集进程，doctor 一次。按接收方约定顺序执行，各份独立保存，不套用任何原作者店数或目录。
2. 每份 `dsivio python scripts/report_state.py plan --config "配置绝对路径"`；默认配置时区前一自然日。只有用户明确指定日期才传 --date。冻结 plan 日期、快照、路径，跨午夜不重算。采集、组装、prepare、audit 全程使用同一份外部 --config。
3. 查同日正式 JSON、batch/capture/checkpoint/handoff：已审计成功且来源未变直接复用；未完成续跑原 batch，只补失败店。用 collect_batch run/status/ready，具体命令和并发限制见 SKILL.md。遇 needs_agent 由 AI 同店用 CLI 恢复，不是直接失败或切桌面工具。
4. 登录按钮、页面广告、加载/选择器变化先现场 CLI 检查。多次有意义恢复仍无进展 defer，正常队列完成且原 run 退出后，用原 batch `--retry-deferred --retry-deferred-after 600` 延迟补采，只重开失败店；真实新验证码/凭据/权限才请接收方操作。异常相关细节读 recovery.md。
5. 所有平台先导出并归档 Excel，核验店铺、目标日、表头、口径及 SHA-256；TikTok 订单/件数分开，SHEIN 三份 Excel 合并。仅 Shopee 满足 SKILL.md 更新中零值条件时网页覆盖并保留双来源。缺项 null，不补零、不沿用历史。
6. 候选验收后按 SKILL.md 执行 assemble_report → prepare → history_ledger → render_reports → audit → complete。各步骤失败先恢复，不能越过审计；历史清理状态独立于业务完成状态。
7. 最终重新 audit 每份正式 JSON，复算销售额、广告费、店铺覆盖及分口径数量；实际打开每份 PNG 检查日期、店名、截断和布局。交付全部 outputs 链接与必要缺项/维护说明，不自动向外部联系人发送。无实质变化保持安静，只通知完成、失败、需用户操作或重要范围变化。

## 跨日报汇总（需接收方确认）

默认只出各份独立日报，不自动把多个类目混算。现带 render_category_summary.py 是专用的童装/灯具/箱包三类汇总器，仅在接收方也采用该三类定义和数量口径时使用：

```text
dsivio python scripts/render_category_summary.py YYYY-MM-DD "童装正式JSON" "灯具正式JSON" "箱包正式JSON" "接收方汇总目录"
```

三份 dateIso 必须相同，币种均为 BRL；只合并销售额，订单/件数/SHEIN销量和支付订单数分开，ROI不合并，其他类目不自动加入。复算 JSON/HTML 并打开 PNG 后才交付。不同类目组合需要另外明确汇总范围及实现，不能改名硬套该专用脚本。

## 定时任务提示词

用户提出每天自动做日报并已给出执行时间时，首轮跑通后直接使用 Dsivio 当前对话的 `schedule_list` 检查已有任务；匹配任务用 `schedule_update` 更新，没有才用 `schedule_create` 创建，不重复创建。`schedule` 使用 `{"kind":"daily","time":"用户选择的 HH:MM"}`；工具按本机时区调度，先核对用户指定时区与本机时区，不能把巴西业务日期时区当作调度时区。创建或更新后核对 enabled、绑定对话和返回的下次运行时间，并将任务 ID 和时间登记到 REPORTS.md。不要求用户再次批准已要求的每日执行。未要求定时或未给出时间则保持手动；若工具不可用，如实说明，不声称已经创建。

Dsivio 和紫鸟客户端需在本机运行。Dsivio 关闭或电脑休眠导致错过计划超过五分钟时，现有调度器跳过该次任务；用户要求补跑时继续原日期/批次。定时任务发送提示词唤醒 Agent，按本 Skill 完成采集和恢复，不只用系统计划任务启动 collect_batch。提示词必须包含实际 Skill 位置、按顺序列出的配置绝对路径和完整工作流，模板：

> 使用安装在【实际 Skill 绝对路径】的 dscraw-report，读取 SKILL.md 和 references/daily-workflow.md，按顺序处理这些日报配置：【配置绝对路径列表】。使用已核验的 Python【解释器绝对路径】执行脚本。每份通过 --config 执行 plan，取配置时区的前一自然日并冻结日期；已有未完成批次继续原批次，完成采集、Agent 恢复及暂缓店补采、Excel 归档、组装、台账、渲染、audit 和 complete。在本对话交付配置要求的全部报告链接及必要缺项说明。复用配置，不重问店铺和目录，不创建定时任务，不向外部联系人自动发送。

详细执行规则只维护在 Skill；具体店铺和路径只维护在配置。不得复制原作者的账号、任务 ID 或固定店铺范围。
