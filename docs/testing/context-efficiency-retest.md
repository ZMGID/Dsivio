# 上下文效率复测清单（#51）

本清单不自动启动付费模型任务，也不操作其它 Agent 的进程或目录。

## 确定性验收

- `native_file_receipts_are_short_on_the_wire_and_keep_review_diff`：真实临时文件、内置 write/edit 与本地 HTTP 模型服务。检查下一轮实际 JSON 请求的工具结果不含代码正文，结构化记录仍保留完整 diff，最终文件正确。
- `distant_local_edits_preserve_large_file_diff`：1303 行中文 CRLF，六处跨区域替换后1313行，1283行不变，+30/-20、六个 hunk。
- `diff_budget_exhaustion_reports_unknown_counts_without_changing_write_result`：计算超限不影响文件落盘，明确 `diff_complete=false`，不显示伪精确增删数。
- `stale_write_snapshot_does_not_overwrite_external_changes`：旧内容失效时保留外部修改。应用内路径锁和替换前内容校验共同使用；跨进程校验为乐观并发检查，不声称能锁住不合作的外部写入者。Windows 使用原子替换，去除先删除旧文件的空档。
- `measured_view_survives_append_and_reopen_but_rejects_configuration_or_history_changes`：请求来源可持久化，追加消息仍适用；模型、提供方、协议、系统指令、工具及 schema、历史前缀变化使旧值失效。无法证明等价的任意请求体覆盖诚实回退估算。
- `run_loop_reserves_output_and_does_not_send_an_unfixable_prompt`：小窗口加大输出预留，固定上下文已超预算时不发送模型请求。
- 长日志检查：全文路径可读取；单行大输出、多行输出都只返回有限首尾，保留退出状态。目录默认40条样本，并返回总数量。

## 三方隔离复测

每次仅运行同一题，各 Agent 使用题目副本。正式启动前填写：

| 项目 | Kivio | DSH | OpenCode |
|---|---|---|---|
| Agent 版本 / 提交 | | | |
| 实际模型 ID / 提供方 | | | |
| API 格式 / 服务地址（无凭据） | | | |
| 推理强度 | high | high | high |
| 输出上限 / 工具配置 | | | |
| 项目目录 | | | |
| 独占端口 / 监听 PID | | | |
| 数据库路径 | | | |
| 本次专用临时目录 | | | |
| 结果目录 | | | |
| 开始 / 结束时刻 | | | |
| 人工追加提示 / 干预 | | | |

启动前只检查已分配端口与对应进程，发生冲突则另选空闲端口；不终止其它任务服务。结束后仅验证和清理登记的本次临时路径，不枚举整个共享 Temp 来猜测归属。

分别记录最后一次实报输入/输出、缓存命中、累计输入/输出、工具结果字节/字符、调用次数、失败重试、用时和题目验收结果。没有实报时明确写“估算”。记录首次创建、局部编辑、必要整份重构和返工原因；不能把每次 write 算成浪费。

相邻请求 token 差额包含协议封装及其它消息变化，不能将组间差额当作保证节省量。相同模型配置和任务验收标准下比较最终成果；不能通过降低 high、省略必要推理回放或提前有损压缩来凑170K。

## 预算依据与兼容性

[Gemini Models API](https://ai.google.dev/api/models) 分别提供 inputTokenLimit 和 outputTokenLimit；因此 Gemini 的输入额度不再次扣除输出。其它内置适配器按共享上下文为有效输出留空间。[Claude 上下文说明](https://platform.claude.com/docs/en/build-with-claude/context-windows) 指出新型号可能接受输入加 max_tokens 超窗的请求，但这仍可能使输出在窗口边界停止；本地预算保留完整回答空间。

模型覆盖新增可选 `maxInput`。旧配置无此字段时保持可加载；未知模型使用现有后备窗口并标示估算，不自动将新型号限额套到旧别名。旧消息无请求身份时不伪造来源；旧文件 diff 无完整性字段时保留旧记录，不补写“已验证”。
