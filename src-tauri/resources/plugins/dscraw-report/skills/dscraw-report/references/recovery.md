# 已验证的 CLI 恢复路径

只在对应异常出现时读取。选择器仅是候选，必须先 content/query 确认当前店铺、targetId、唯一可见控件，再 click 并重读结果；不把历史 targetId、任务时间或选择器直接复用。

## Shopee 登录与导出

- 已自动填充的登录页，可以点击已启用的正常登录/验证按钮；主/子账号入口按 SKILL.md。不读取密码、不因字段已填仍停在登录页。验证候选 `.eds-button--primary.ios-action`。
- `ERR_SOCKS_CONNECTION_FAILED` 先同店重读，必要时 visit 原业务地址，再核对账号和日期，不换店取值。
- 广告活动弹层可能影响导出菜单，但不一定阻塞。先尝试可见业务按钮；确实遮挡时 query 现场关闭按钮，曾出现 `.rewards-homepage-prompt .eds-modal__close`。
- 导出已提交但下载菜单消失，重新打开导出任务列表，曾出现 `[data-testid="export-data-result-trigger"]`。按已记录的目标日、类型、提交时间匹配原任务，再定位其可见下载按钮；过滤隐藏重复项，不固定取最新第一条、不再次提交导出。
- 广告补采结束发 ready 前，先读 handoff 确认脚本待续步骤，需要经营入口时回该入口并复核；不要把广告卡片当经营来源。

## SHEIN 三份 Excel 已齐但期间合计冲突

先解析三份归档，确保目标日行存在、表头类型正确。用 CLI 选准确目标单日核验 GMV、销量、支付订单数，三项必须与各自 Excel 目标日行相同。已有 `scripts/resolve_shein_single_day.py` 支持：

```text
dsivio python scripts/resolve_shein_single_day.py --capture "本店capture.json" --checkpoint "本店checkpoint.json" --date YYYY-MM-DD --gmv "现场单日GMV" --units "现场单日销量" --paid-orders "现场单日支付订单数"
```

该脚本重读归档文件验证相等后才更新候选；页面仅作核验，不代替 Excel。不得为了通过校验而改传入读数或编辑归档。失败则继续查对应表头、日期或重新导出准确单日。

## 已验收但批次状态未收敛

Agent 手动验收后按 batch close 流程确认关窗；关闭失败不释放名额。顶层 needs_agent 不能单独证明该店缺数据：查具体店状态及候选、下载清单和正式 audit。已审计成功结果不重采，但未验收候选或未关闭窗口仍须处理。下载目录严格使用本次 store open 返回值，包含全角符号时也不得自行替换拼路径。
