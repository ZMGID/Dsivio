# Kivio → Dsivio 逐项合入决策清单

检查日期：2026-10-07。用途：让用户按编号选择合入范围；这是代码审查产物，不是新的工程规范，也不是已经执行的合并计划。

## 阅读方式与范围

- 上游：`ZMGID/kivio` 的 `main`，本轮重新 fetch 后固定在 [`6cb64d1d`](https://github.com/ZMGID/kivio/commit/6cb64d1d751445de6df047be076f12277140523e)。
- 本地：Dsivio `HEAD=cf5f8a84`（workbench: redesign home page），另有检查开始时的 29 个未提交路径。未提交编辑不视作已经提交的基线，也没有覆盖它们。
- 两仓没有 Git 共同祖先。此前按内容选择性同步，不能用“上游提交不在本地历史里”判断功能未合入。
- 完整已提交快照比较：**1,594 个路径**，其中 **354 个仅上游存在、730 个仅 Dsivio 存在、510 个两边内容不同**。这不是 1,594 个待实现功能；尤其不能据此删除 730 个本地路径。
- 近期增量：上次检查的 `37db36ca` 到当前上游，**60 个第一父链提交、659 个变化路径**。另补查了较早未整包同步的插件市场、媒体工作站，以及当前全树的定制边界和历史差异。
- 本文拆成 **166 个决策项**，包括每个新插件包、已有等价实现、排除项和必须保留的本地边界。同一提交可能对应多个决策项。
- [可筛选/填写决定的 CSV](kivio-dsivio-change-review-2026-10-07.csv)：每行对应一个编号，最后两列留给用户决定和备注。
- [完整文件差异索引 CSV](kivio-dsivio-change-review-2026-10-07-files.csv)：列出全部 1,594 个路径、关联提交、决策项和未提交标记。关联项表示核对范围，不是允许整文件替换。
- 结论依据为源码、差异、现有 ADR 和上游测试用例阅读。**没有运行应用、付费模型、测试套件或端到端验证**；“建议合入”不等于“已验证合入成功”。路径级全量覆盖也不等于对每一行代码作过独立正确性证明。

| 标记 | 含义 | 数量 |
| --- | --- | ---: |
| 建议合入 | 现有能力上的明确修复；仍需回归 | 43 |
| 需适配 | 值得考虑，但有 Dsivio 接口、架构或行为冲突 | 42 |
| 可选 | 新增能力或产品/视觉选择，不作为修复默认夹带 | 54 |
| 已有实现 | 本地已有相关或等价行为，不重复整包搬入 | 6 |
| 不合入 | 按当前产品边界排除，除非另行改变需求 | 11 |
| 保留本地 | 整树比较中的本地保护项，不是上游待移植项 | 10 |

## 功能级优先档（覆盖核对后的讨论范围）

以这里的功能块讨论是否纳入；下方细项仅作实现与证据索引。优先级是当前建议，不表示用户已经授权合并，也不表示按批次边做边增加范围。细项里的“建议合入/需适配”表达可移植性，与功能优先级是两个维度。

| 档位 | 功能块 | 覆盖细项数 |
| --- | --- | ---: |
| 优先合并 | Agent 稳定性 | 14 |
| 优先合并 | 上下文与费用统计 | 11 |
| 优先合并 | 聊天性能与交互 | 18 |
| 优先合并 | 附件与数据保护 | 6 |
| 优先合并 | 跨页面操作可靠性 | 11 |
| 优先合并 | 现有自动化完善 | 4 |
| 优先合并 | 桌面与公共控件稳定性 | 7 |
| 一般 | 定时聊天任务 | 8 |
| 一般 | 模型与浏览器配置 | 4 |
| 一般 | 插件市场能力扩展 | 5 |
| 一般 | 完整备份恢复 | 3 |
| 一般 | 外观与导航升级 | 6 |
| 一般 | 媒体与作品体验增强 | 6 |
| 不建议整包纳入 | 新增 30 个插件包 | 30 |
| 不建议 | 上游独立媒体执行链 | 2 |
| 不建议 | 恢复外部 Agent 运行时 | 4 |
| 不建议 | 桌宠与配套功能 | 2 |
| 不建议 | 全仓清理和上游发行替换 | 3 |
| 随所选功能处理 | 测试与构建配套 | 6 |
| 无需重复合并 | 已有或等价实现 | 6 |
| 始终保留 | Dsivio 本地能力和未提交工作 | 10 |

核对结果：166 个细项各归入一个功能块，无遗漏、无重复；60 个近期提交仍由文末覆盖表追溯。范围固定在上游 `6cb64d1d`，不包含此后新增提交。

上一版聊天概览需要显式补充三处：**桌面与公共控件稳定性**、**媒体与作品体验增强**、**测试构建及发行清理边界**。另外，“已有实现”和“保留本地”单列，避免把它们当作尚未合入的功能。

媒体体验增强只考虑适配到 Dsivio 现有预览、任务、作品索引；不恢复上游独立 MediaStation 或第二套执行链。媒体删除失败保护归入优先档的数据保护。

测试与构建修复随选中的功能进入范围；不默认删除本地测试，不执行全仓格式化，不替换 Dsivio 品牌、版本和发布配置。

## 分类导航

| 分类 | 项数 | 编号 |
| --- | ---: | --- |
| A · Ask、待办与 Agent 基础 | 8 | A01—A08 |
| B · 上下文、压缩、用量与费用 | 12 | B01—B12 |
| C · 聊天显示、阅读位置、输入与导航 | 18 | C01—C18 |
| D · 附件、作品、文件与知识库 | 9 | D01—D09 |
| E · 页面离开后的任务、草稿与设置操作 | 10 | E01—E10 |
| F · 模型请求、配置与浏览器连接 | 13 | F01—F13 |
| G · 定时聊天任务（新增功能组） | 8 | G01—G08 |
| H · 原自动化模块（不是媒体工作流编辑器） | 4 | H01—H04 |
| I · 插件框架与市场入口 | 6 | I01—I06 |
| J · 媒体创作（按 Dsivio 单一网关适配） | 7 | J01—J07 |
| K · 桌面与公共控件外观 | 13 | K01—K13 |
| L · 集备份与恢复 | 3 | L01—L03 |
| M · 不恢复的外部运行时与桌宠 | 6 | M01—M06 |
| N · 测试、构建、文档和历史整理 | 9 | N01—N09 |
| Z · Dsivio 必须保留的定制边界 | 10 | Z01—Z10 |
| P · 新增 30 个插件包（每包可独立选择） | 30 | P01—P30 |

## 合入前必须知道的几件事

1. **原生 API 请求身份 ≠ 外部 CLI 运行时。** F01—F07/F13 属于 Dsivio 仍保留的原生模型链路；M01—M04 才是已删除的外部运行时。本轮细审纠正上一轮概览对这部分过粗的排除。
2. **Ask 的输入修复 ≠ 之前 Ask 不挂载问题的根因证明。** A01—A03 改输入/翻页；C05 保护待处理卡片可见；本地交互快照同步继续保留，仍需复现“当前对话直接弹出”。
3. **上下文更新不只是表盘。** B01—B08 会改协议、usage 口径、压缩预算、历史保留和恢复，不能从大提交只挑一个组件。
4. **两种工作流不能混为一谈。** H/E06 是原 `src/chat/automation`；当前未提交工作在 `src/chat/workbench/workflow` 和 `src-tauri/src/generation_workflow`。
5. **窗口内状态保留不等于重启恢复。** E 组许多草稿和操作在当前窗口存活；不要对所有草稿承诺退出应用后仍存在。Tinyfish 授权离开应取消，和下载/安装相反。
6. **最新浏览器 Token 仍需补保护。** 静态阅读 `native_tools/shell.rs`：后台 stdout/stderr 先写原始日志，轮询结果再按当前完整 Token 替换；原始文件未脱敏，分块切开 Token 也可能漏掉。F12 应先修这两个缺口；本轮未用真实 Token 复现。
7. **新插件主要是 Skill 封装。** 新增 30 包不等于安装了 30 个 CLI，不等于账号或云权限可用；P 表描述来自上游清单，未进行真实业务验证。FFmpeg 必须遵守 Dsivio 已有媒体运行时约定。
8. **数据与品牌优先保留。** 本地项目指令、媒体网关、电商批准、发布入口、工作台、葡萄牙语、点阵动画、独立发行配置不能被上游整文件替换覆盖。

## 逐项决策

### A · Ask、待办与 Agent 基础

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [A01](https://github.com/ZMGID/kivio/blob/c6029f756a6821188d22ed2be0a8e811020f97ef/src-tauri/src/chat/ask_user.rs) **Ask 始终允许自定义回答** | 后端规范化问题时开放自定义输入，模型不能把用户限制在选项里 | 本地仍接受 allow_custom 限制 | 建议合入 | 可与 A02、A03 同批；保持现有 Ask 快照同步修复 |
| [A02](https://github.com/ZMGID/kivio/blob/c6029f756a6821188d22ed2be0a8e811020f97ef/src/chat/AskUserBlock.tsx) **Ask 输入数字和中文不误提交** | 输入框隔离快捷键；IME 合成期间 Enter 不选项、不提交 | AskUserBlock 尚缺这组保护，普通输入框已有 IME 处理不能替代它 | 建议合入 | 独立回归数字、中文确认、真正 Enter 提交 |
| [A03](https://github.com/ZMGID/kivio/blob/c6029f756a6821188d22ed2be0a8e811020f97ef/src/chat/AskUserBlock.tsx) **Ask 多问题翻页和提交** | 自定义答案有明确确认按钮；单选自定义与选项互斥；下一题恢复焦点 | 现有 Ask 流程未包含这轮完善 | 建议合入 | 依赖 A01、A02；测试多题、回看和键盘操作 |
| [A04](https://github.com/ZMGID/kivio/blob/8c0780c8c08ca9d92e6b8b6edbc1df25852b3e46/src/chat/agentTodoState.ts) **待办更新不被旧读取覆盖** | 使用 todoRevision 合并状态，旧会话快照不能覆盖更新的实时待办 | 本地没有 agentTodoState.ts；已有待办基础但缺增量保护 | 建议合入 | 主窗口和弹出窗口同时适配，不只改一个订阅点 |
| [A05](https://github.com/ZMGID/kivio/blob/8c0780c8c08ca9d92e6b8b6edbc1df25852b3e46/src-tauri/src/chat/agent/loop_.rs) **Agent 结束前检查待办** | 有未完成待办时，每轮最多追加一次收尾提醒，让模型更新状态 | 本地没有这轮收尾检查 | 需适配 | 与 A04 配合；可能多一次模型请求，不能宣称零开销 |
| [A06](https://github.com/ZMGID/kivio/blob/8b2f3e5ca98d3652d6e32b7ced8f8addcafc1872/src-tauri/src/chat/todo.rs) **早期待办重做与回退恢复** | 待办不反复修改系统提示词；回退、重答恢复对应状态 | 此前已在 76a38040 选择性同步 | 已有实现 | 不是本次新增缺口；保持已有测试 |
| [A07](https://github.com/ZMGID/kivio/blob/ec22e02fb6aed5e629dea01e0ee9f43754e833da/src-tauri/src/chat/sub_agent.rs) **子代理等待、身份和消息去重** | 主代理等所需子任务；跨轮工具消息 ID 不重复；子代理历史可继续 | 此前已同步，且 ADR 0005 明确保留身份 | 已有实现 | 不因 Git 没有共同祖先再次整组搬入 |
| [A08](https://github.com/ZMGID/kivio/blob/c7d94a0d3c636dcf7af494fbce54b8fe60bf3e61/src-tauri/src/chat/agent/compaction.rs) **压缩分支历史合并记录** | 上游补齐旧压缩分支的 Git 历史 | c7d94a0d 相对第一父提交没有文件变化 | 已有实现 | 历史整理提交不算新增功能，也不作为 cherry-pick 目标 |

### B · 上下文、压缩、用量与费用

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [B01](https://github.com/ZMGID/kivio/blob/ce772a2560927d65d64ef8d4c4e835243fd9e289/src-tauri/src/chat/agent/context_measure.rs) **上下文表盘使用实际请求用量** | 以主请求供应商回报的输入、输出为准，缺失值保持未知 | 本地仍是旧测量链路；没有新的 context_measure 模块 | 需适配 | B01—B08 为关联组；协议、后端、前端一起更新 |
| [B02](https://github.com/ZMGID/kivio/blob/ce772a2560927d65d64ef8d4c4e835243fd9e289/src-tauri/src/chat/model/types.rs) **缓存和流式 usage 归一化** | Anthropic 缓存读写计入对应输入；OpenAI 避免重复计数；合并不完整流式 usage | 本地有旧 usage 解析，缺本轮统一口径 | 需适配 | 必须和 B01、模型适配器一起验证 |
| [B03](https://github.com/ZMGID/kivio/blob/ce772a2560927d65d64ef8d4c4e835243fd9e289/src/chat/hooks/useConversationContext.ts) **拒绝过期的上下文结果** | measurementSeq、lifecycleId、run 绑定防止旧事件覆盖新值 | 本地缺这组跨端字段与生命周期判断 | 需适配 | 不能只移植表盘组件；包含弹出窗口 |
| [B04](https://github.com/ZMGID/kivio/blob/84f14d76ed5ce8dc3996fbfdfb9aa71f1de481c7/src-tauri/src/chat/commands/mutations.rs) **修改历史后重新计算上下文** | 换模型、删除回答、选择不同模型回答后使旧测量失效；相同选择不重复失效 | 本地未包含最终失效链路 | 需适配 | 与 C11、B03 配套，取 84f14d76 的后续修正 |
| [B05](https://github.com/ZMGID/kivio/blob/ce772a2560927d65d64ef8d4c4e835243fd9e289/src-tauri/src/chat/model/types.rs) **保留图片 detail 与改进估算** | 存储到模型请求保留 low/high/auto/original；按实际准备后的请求估算分类 | 本地未包含这轮图片 detail 及估算调整 | 需适配 | 实际用量与本地估算分开显示；不破坏媒体网关 |
| [B06](https://github.com/ZMGID/kivio/blob/ce772a2560927d65d64ef8d4c4e835243fd9e289/src-tauri/src/chat/agent/compaction.rs) **压缩预留空间和摘要上限** | 调整上下文预留策略；摘要输出上限调整为 16384，避免摘要本身挤满窗口 | 已有旧压缩实现，策略不同 | 需适配 | 这是行为变化，不是仅修表盘；小窗口、超窗、停止都要回归 |
| [B07](https://github.com/ZMGID/kivio/blob/ce772a2560927d65d64ef8d4c4e835243fd9e289/src-tauri/src/chat/agent/compaction.rs) **按预算保留完整历史组** | 按 token 预算保留最近 assistant/tool 组，避免切断工具配对，并保留最新组 | 已有历史保留逻辑，但还不是这轮预算策略 | 需适配 | 与 B06 同批；保留 Dsivio 项目指令和子代理上下文规则 |
| [B08](https://github.com/ZMGID/kivio/blob/ce772a2560927d65d64ef8d4c4e835243fd9e289/src-tauri/src/chat/agent/compaction.rs) **压缩提醒、重试与取消适配** | 读取文件提醒受剩余预算约束，摘要准备及恢复路径随新测量重做 | 本地已有旧超窗恢复与取消，不能覆盖丢失 | 需适配 | B01—B07 联合验收失败、取消、超窗恢复与压缩后重开 |
| [B09](https://github.com/ZMGID/kivio/blob/3483e1cd9fb1899032cbe1e779ca87c57cc72062/src-tauri/src/chat/agent/context_measure.rs) **新请求期间保留上一份上下文读数** | 新测量尚未回来时保留有效快照，避免显示跳空 | 本地未包含这个后续修正 | 建议合入 | 依赖 B01—B03，取 3483e1cd 的最终行为 |
| [B10](https://github.com/ZMGID/kivio/blob/3483e1cd9fb1899032cbe1e779ca87c57cc72062/src-tauri/src/usage.rs) **对话费用包含持久子代理请求** | 从持久 usage 账本累计主对话及子代理，包含已删除消息对应的真实历史花费 | 本地费用展示未采用这轮账本汇总 | 需适配 | 不要重复累计子代理；与上下文占用分开，不能按现存消息估费用 |
| [B11](https://github.com/ZMGID/kivio/blob/3483e1cd9fb1899032cbe1e779ca87c57cc72062/src/chat/SessionUsageStrip.tsx) **未知价格不显示成免费** | 账本汇总保留缺价请求数量，界面反映费用不完整 | 本地缺新版汇总语义 | 建议合入 | 依赖 B10；未知单价不能当作 0 美元 |
| [B12](https://github.com/ZMGID/kivio/blob/4354f73375ec4069c020f688577e1ec71396c359/src/settings/UsageStatsPanel.tsx) **Token 趋势图可读性** | 增加轴留白、可读刻度、面积渐变；隐藏曲线与提示项一致，单点居中 | 本地仍是旧图表表现 | 可选 | 展示优化，可独立；适配 Dsivio 当前主题变量 |

### C · 聊天显示、阅读位置、输入与导航

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [C01](https://github.com/ZMGID/kivio/blob/7bf437910e2b3c6733f3caff4df8f3116e12983e/src/chat/Chat.tsx) **第一轮回答结束闪空首页** | 流式回答转历史时不短暂回到空会话欢迎页 | 本地有会话切换动画，但没有这轮首答结束保护 | 建议合入 | 保留 Dsivio 点阵品牌动画；不能用删除动画代替修复 |
| [C02](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/ChatMarkdown.tsx) **流式代码延后高亮** | 生成期间显示转义文本，结束后才完整语法高亮，减少每个 token 的全文扫描 | 本地已有长流式优化，但未包含这一轮代码块策略 | 建议合入 | 与 C03 同批，验证代码复制和结束后高亮 |
| [C03](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/ChatMarkdown.tsx) **回答结束不重建 Markdown 树** | 保留带 key 的渲染树，避免定稿时重挂载代码等内容 | 本地缺本轮稳定树调整 | 建议合入 | 与 C02、C04、C06 一起验证高度和阅读位置 |
| [C04](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/MessageBubble.tsx) **过程片段渲染设上限** | 默认保留最近 20 个过程片段，手动展开及正在阅读的片段保留 | 本地直播过程仍有 Infinity 限制值 | 建议合入 | 不是删除历史；长工具回合和手动展开要验收 |
| [C05](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/MessageBubble.tsx) **待回答 Ask 和权限卡持续可见** | 裁剪过程片段时保留未解决交互与最新思考 | 本地尚无新裁剪规则 | 建议合入 | 依赖 C04；这是可见性保护，不能证明旧 Ask 挂载问题根治 |
| [C06](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/MessageList.tsx) **流式转历史保留阅读锚点** | 继承测量高度和 WebKit scrollTop；用户滚动后解除自动跟随 | 此前已有阅读位置修复，本轮是后续增强 | 建议合入 | 必须在 macOS 原生 WebView 验证，浏览器测试不够 |
| [C07](https://github.com/ZMGID/kivio/blob/84f14d76ed5ce8dc3996fbfdfb9aa71f1de481c7/src/chat/MessageList.tsx) **向上滚动自动加载历史** | 接近顶部提前加载，合并重复请求，保留锚点，失败可继续上滚重试 | 已有按需历史，但未包含这轮自动加载交互 | 建议合入 | 复用 Dsivio useConversationHistory，不把新逻辑都塞回 Chat.tsx |
| [C08](https://github.com/ZMGID/kivio/blob/84f14d76ed5ce8dc3996fbfdfb9aa71f1de481c7/src/chat/markdownUtils.ts) **普通下划线不误当强调** | 保留文件名、标识符和普通文本中的下划线 | 本地未包含该轮 Markdown 处理 | 建议合入 | 测试多下划线、合法强调、代码块、数学内容 |
| [C09](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/ChatMarkdown.tsx) **定稿文本不继续补全残缺语法** | 只在流式阶段修补未完成 Markdown，历史正文不丢残缺链接和分隔符 | 本地缺此定稿语义调整 | 建议合入 | 与 C03 同批；检查被用户停止的半句链接、图片和代码 |
| [C10](https://github.com/ZMGID/kivio/blob/c5944f9a703d4dbed058a96e46f0db39fe5410f4/src/chat/MessageBubble.tsx) **选中文字添加到聊天** | 分组回答、键盘选区、延迟 selectionchange 也能出现引用入口 | 现有引用入口未包含这轮恢复 | 建议合入 | 和 C12 一起验证只插入一次、焦点不丢 |
| [C11](https://github.com/ZMGID/kivio/blob/9abb6bd93e66b47e536ff0d7166518682c6203ef/src-tauri/src/chat/commands/mutations.rs) **删除单个模型回答** | 保留用户提问、兄弟回答和后续轮次，只移除目标答案 | 本地未包含这轮独立删除语义 | 需适配 | 必须带 B04；选中的回答被删才重选上下文，附件 GC 也要验证 |
| [C12](https://github.com/ZMGID/kivio/blob/7badbafd50119a38012b229a994438aa0f37e1c9/src/chat/InputBar.tsx) **引用插入不重复且立即显示** | 草稿只有一个写入入口；插入后立即从共享草稿同步输入框 | 本地未包含这两次连续修正 | 建议合入 | 84f14d76 与 7badbafd 一起取，不能停在中间版本 |
| [C13](https://github.com/ZMGID/kivio/blob/84f14d76ed5ce8dc3996fbfdfb9aa71f1de481c7/src/chat/goalPresentation.ts) **目标模式与草稿保持一致** | 是否处于待发送 goal 模式由 /goal 草稿派生，恢复草稿不丢模式 | 本地仍是旧模式与草稿协调 | 建议合入 | 主窗口和弹出窗口一起；普通历史输入恢复也需回归 |
| [C14](https://github.com/ZMGID/kivio/blob/bc40a10984bacf670aafdf3b3cace32f9accb897/src/chat/Sidebar.tsx) **侧栏刷新去重与过期结果隔离** | 上下文更新不触发无关目录重读；慢请求期间刷新合并，卸载后不跳转 | 本地侧栏有定制，缺这轮请求稳定性修复 | 需适配 | 按刷新逻辑移植，保留工作台导航、集/项目手动顺序 |
| [C15](https://github.com/ZMGID/kivio/blob/bc40a10984bacf670aafdf3b3cace32f9accb897/src/chat/conversationWarmCache.ts) **快速切回对话复用在途快照** | 空闲序列化尚未完成时也能复用预热中的会话快照 | 本地有 warm cache，缺这个在途窗口保护 | 建议合入 | 保留本地导航超时、重试和迟到结果隔离 |
| [C16](https://github.com/ZMGID/kivio/blob/d836ef9478117d294abb567634cf1bc2aa218fe4/src/chat/ChatMarkdown.tsx) **链接和来源标题更像可点击元素** | Markdown 链接与网页来源标题改进颜色、下划线和交互样式 | 现有功能可用，属于可见性调整 | 可选 | 不需要跟随整套主题替换 |
| [C17](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/ChatMarkdown.tsx) **公式样式补全** | 引入 KaTeX 样式，完善数学公式实际显示 | 检查到上游本轮补了样式导入 | 建议合入 | 先查最终样式加载是否重复，生产构建检查公式 |
| [C18](https://github.com/ZMGID/kivio/blob/dd2936cc8a32f527dd7767ce03a8f1c506c3c769/src/chat/KivioBlob.tsx) **空闲动画减少持续调度** | 对话状态不需要动画时停止无意义刷新 | Dsivio 使用点阵 Logo，不能直接换回上游 Blob | 需适配 | 只取调度原则，保留品牌和减少动态效果设置 |

### D · 附件、作品、文件与知识库

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [D01](https://github.com/ZMGID/kivio/blob/4728ba517a1ce7624c7c55a553ed7eafa42e3223/src-tauri/src/chat/gc.rs) **防止误删被引用附件** | GC 统一绝对路径与文件名口径，仅识别附件目录内引用；不确定时保守保留 | 确认本地仍是旧的两参数文件名比较逻辑 | 建议合入 | 高优先级；消息、工具结果、PDF 引用都要覆盖 |
| [D02](https://github.com/ZMGID/kivio/blob/bc40a10984bacf670aafdf3b3cace32f9accb897/src-tauri/src/chat/attachments.rs) **非图片产物统一保存文件引用** | 不再按文件大小把文档等产物塞进会话 data URL，降低历史负担 | 本地仍有旧附件内联逻辑 | 建议合入 | 与 D01、D03 同批，保留本地视频稳定产物 ID |
| [D03](https://github.com/ZMGID/kivio/blob/bc40a10984bacf670aafdf3b3cace32f9accb897/src-tauri/src/chat/storage/conversations.rs) **旧内联产物迁移不丢原数据** | 读取历史时迁移为文件，写入失败则保留原字节 | 本地缺本次旧数据迁移保护 | 建议合入 | 用旧会话、磁盘失败、重开测试，不只测新对话 |
| [D04](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/ArtifactsCenter.tsx) **作品操作跨页面保留状态** | 重命名、导出、删除保持忙碌与失败草稿；旧列表读取不复活已删除作品 | 有作品页且已做防闪定制，未包含这轮操作生命周期 | 需适配 | 保留用户要求的现有作品布局；与 E10 共用最小支撑 |
| [D05](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src-tauri/src/chat/artifacts.rs) **媒体生成结果进入作品索引** | 上游把完成的媒体任务索引到作品，允许没有来源聊天的产物 | Dsivio 媒体任务、图片视频库已有自己的统一来源 | 需适配 | 只考虑体验，不接入 media-station 第二存储；先确定是否要在作品汇总 |
| [D06](https://github.com/ZMGID/kivio/blob/24b5707b6327174e9f154ba624349b73a64a71dc/src/chat/dock/useFileTree.ts) **文件搜索拒绝旧响应** | 换关键词、切隐藏文件或目录后，旧查询结果不覆盖新查询 | 此前合的是展开树恢复，本条是新搜索竞态修复 | 建议合入 | 独立小修复；快速换词与 showHidden 一起验证 |
| [D07](https://github.com/ZMGID/kivio/blob/24b5707b6327174e9f154ba624349b73a64a71dc/src-tauri/src/chat/knowledge_base/store.rs) **知识库删除不被迟到向量回写复活** | 替换分块用事务，写入前确认文档仍存在且分块属于该文档 | 本地知识库还没有本轮删除竞态保护 | 建议合入 | 模拟删除发生在 embedding 返回前；不能只加前端过滤 |
| [D08](https://github.com/ZMGID/kivio/blob/7f142b375ebf0d37ec645d133c47a22f09bed57c/src/chat/ChatMarkdown.tsx) **流式 SVG 预览** | 流式阶段展示可预览 SVG，并有独立清理处理 | 本地未包含新增预览处理 | 可选 | 单独核查 SVG 清理与外部引用，不能直接把原始 SVG 注入 DOM |
| [D09](https://github.com/ZMGID/kivio/blob/7f142b375ebf0d37ec645d133c47a22f09bed57c/src/chat/MediaStation.tsx) **视频预览显示画面** | 上游媒体结果预览支持显示视频帧而非始终空白 | Dsivio 有自己的媒体预览与任务列表 | 需适配 | 将可用处理落到现有预览组件，不添加 MediaStation 页面 |

### E · 页面离开后的任务、草稿与设置操作

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [E01](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/notesDraftStore.ts) **笔记保存串行与草稿保留** | 保存期间新输入继续排队；离开重进保留草稿、失败和重试；删除后不复活 | 本地有自动保存，但无新版 notesDraftStore | 建议合入 | 24b5707b 和 a696d7ea 取最终状态；保存失败不能假装成功 |
| [E02](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/assistantDraftStore.ts) **助手编辑草稿与迟到写隔离** | 跨导航保留未保存草稿，晚到保存、复制、删除结果不打开错误助手 | 本地没有 assistantDraftStore | 建议合入 | 返回列表与临时离开不同；不要擅自改成自动保存 |
| [E03](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/skillLifecycle.ts) **Skill 安装和开关跨页面继续** | 同一安装去重，离开后完成仍刷新；失败可重试；保留 URL 草稿和启停状态 | 本地没有 skillLifecycle，新旧页面有差异 | 需适配 | 只取原生 Skill；剔除 CLI 扫描、Pi 分组，保留本地安装兼容修复 |
| [E04](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/market/marketOperations.ts) **插件安装、导入、启停操作保留** | 关闭对话框不伪装取消已发出的安装；离开页面后仍能看完成或失败 | Dsivio 插件市场有独立使用、建项目逻辑 | 需适配 | 和 I01/I02 按需组合，不能整文件替换 MarketPage |
| [E05](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/sessionBatchStore.ts) **会话批量操作跨导航可靠** | 批量归档、删除、钉住、移动、导出保持忙碌；部分失败可重试 | 本地有 SessionCenter，没有新版 sessionBatchStore | 建议合入 | 保留选中范围；只重试失败项，不重复打开保存对话框 |
| [E06](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/automation/automationDraftStore.ts) **自动化草稿跨页面保留** | 串行保存、保留较新编辑、隔离过期刷新；删除后迟到保存不复活 | 本地没有新版 automationDraftStore | 需适配 | 这是 src/chat/automation，不是正在开发的 workbench/workflow |
| [E07](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/settings/tabs/ComputerControlTab.tsx) **计算机控制安装/更新状态保留** | 离开设置后安装更新继续，返回可见失败并重试 | 本地有对应设置页，仍使用旧操作生命周期 | 建议合入 | 保留本地内置运行时定位；不要恢复已删 CLI 管理 |
| [E08](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/settings/useSettingsOcrDownloads.ts) **OCR 下载单一任务与订阅** | 设置和知识库共享同一安装，离开页面下载仍继续，终态释放监听 | 本地 OCR 状态仍分散在组件与旧 hook | 建议合入 | 下载中切页面、失败重试和卸载订阅都要测 |
| [E09](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/settings/WebSearchPanel.tsx) **Tinyfish 授权离开即取消** | 离开授权页取消后端流程，迟到 token 不写设置，新一轮授权不被旧结果覆盖 | 本地有 Tinyfish 配置，未包含这轮授权生命周期 | 建议合入 | 注意它应取消，不能一概照搬“所有任务离开都继续” |
| [E10](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/utils/windowStore.ts) **窗口级操作状态最小支撑** | 窗口生命周期的 store 保留操作结果，按操作 key 合并重复开始 | 本地没有该公共 windowStore | 需适配 | 只是 E 组的支撑，不是独立产品；不承诺应用重启后恢复所有草稿 |

### F · 模型请求、配置与浏览器连接

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [F01](https://github.com/ZMGID/kivio/blob/fd855af32c6eba80efcc7faf5c4d770629e13329/src-tauri/src/chat/model/anthropic.rs) **Anthropic 系统与缓存声明一致** | system 使用内容块，稳定 metadata.user_id，按能力声明 beta 并保留用户自定义 beta | Dsivio 保留原生 Anthropic 适配器，但缺本轮请求规范化 | 需适配 | fd855af3 与 26058234 成组；不能把它误归外部 CLI 删除项 |
| [F02](https://github.com/ZMGID/kivio/blob/26058234f63ff912245a0854198f893b55064880/src-tauri/src/provider_request.rs) **Claude/Codex 请求身份与正文一致** | 已有客户端身份选项的请求头、系统前缀、会话 UUID、正文标记同步 | 本地 provider_request.rs 仍有 cli_identity 配置，确实适用 | 需适配 | 保留 Dsivio UA/身份；不安装、不启动任何外部 CLI；包含后续纠正 |
| [F03](https://github.com/ZMGID/kivio/blob/4728ba517a1ce7624c7c55a553ed7eafa42e3223/src-tauri/src/chat/model/anthropic.rs) **工具名映射保留精确大小写** | 优先精确匹配 MCP 名，只有唯一候选才忽略大小写恢复；非流式历史也映射回来 | 新映射依赖 F02；本地旧请求身份行为需整体适配 | 需适配 | fd855af3、26058234、4728ba51 同组；测试 read/Read 两个工具共存 |
| [F04](https://github.com/ZMGID/kivio/blob/fd855af32c6eba80efcc7faf5c4d770629e13329/src-tauri/src/provider_request.rs) **OpenAI 默认请求头一致** | Chat/Responses 共用头部构造，缺省才补 UA、Accept，用户显式设置优先 | 本地有旧 provider_request，未采用这轮共享构造 | 需适配 | 保留 Dsivio 名称，检查自定义网关与覆盖优先级 |
| [F05](https://github.com/ZMGID/kivio/blob/26058234f63ff912245a0854198f893b55064880/src-tauri/src/chat/model/responses.rs) **Responses 正文和会话亲和字段** | store:false、会话亲和头、身份模式下的 prompt_cache_key 与 UUID 一致 | 本地 Responses 仍是旧版本行为 | 需适配 | 必须带 26058234：Codex 身份不再发送冲突的亲和头 |
| [F06](https://github.com/ZMGID/kivio/blob/26058234f63ff912245a0854198f893b55064880/src-tauri/src/chat/model/openai.rs) **Accept 跟随最终 stream 模式** | 强制流式的 OAuth 请求也发送 SSE Accept，避免正文与头冲突 | 本地尚无本轮最终正文后的判断 | 建议合入 | 与 F04/F05 配套，测试 stream 被适配器改写的情况 |
| [F07](https://github.com/ZMGID/kivio/blob/26058234f63ff912245a0854198f893b55064880/src-tauri/src/provider_oauth.rs) **OAuth 与身份头冲突修正** | Codex OAuth 保持自身 originator 语义，去掉矛盾的身份 version 头 | 本地保留 provider OAuth，不等同于外部 Agent | 建议合入 | 依赖 F02；不能删除现有 OAuth 登录能力 |
| [F08](https://github.com/ZMGID/kivio/blob/49d9d6e166269d751568192d95d4e9628db664eb/src-tauri/src/chat/model_metadata.rs) **模型特定思考参数映射** | 上游为部分 Claude 条目调整 Off 和 adaptive/effort 的发送方式 | 本地模型元数据与这轮上游不同 | 需适配 | 只在对应模型使用时合；来源是上游实现，未实测厂商当前支持 |
| [F09](https://github.com/ZMGID/kivio/blob/6c60d9c3625cca4b9af1aff83a7e20b2b16a69e9/src/data/modelDatabase.json) **模型目录和匹配规则更新** | 更新目录及 Qwen 变体匹配，补别名、窗口和元数据 | 本地保留自己的 GPT-6 配置和媒体模型池 | 可选 | 49d9d6e1 与 6c60d9c3 按条目合；不保证可用性或当前价格 |
| [F10](https://github.com/ZMGID/kivio/blob/ee55715a3a5b1f1b906683f44aa42a21c4bff7a1/src-tauri/src/provider_request.rs) **免 Key 端点全链路支持** | 允许空 API Key 聊天、模型发现和测试；空 Key 不发鉴权头，OAuth 仍需登录 | 本地未包含新版一致的可用性判断 | 需适配 | 覆盖模型选择、知识库、Lens 和自配置；媒体入口按 Dsivio 网关适配 |
| [F11](https://github.com/ZMGID/kivio/blob/ee55715a3a5b1f1b906683f44aa42a21c4bff7a1/src-tauri/src/settings.rs) **TASK/SMOL/SLOW 模型角色** | 配置角色模型及思考档；角色未配置先回退 TASK 再跟随主对话；迁移旧子代理覆盖项 | 本地尚无这套角色配置 | 可选 | 涉及 settings 迁移、子代理、自动化和辅助模型调用；必须一批完成 |
| [F12](https://github.com/ZMGID/kivio/blob/6cb64d1d751445de6df047be076f12277140523e/src-tauri/src/native_tools/shell.rs) **Playwright 扩展 Token 持久配置** | 设置或聊天自配置保存 Token；仅匹配 Playwright CLI 命令时注入环境 | 本地未包含最新配置与注入 | 需适配 | 先补后台原始日志和分块输出脱敏缺口；设置写入沿用现有负责人 |
| [F13](https://github.com/ZMGID/kivio/blob/48629d71aea4c459502b2ec3546cdc1da2f34b84/src-tauri/src/provider_request.rs) **原生请求身份内置版本更新** | 外部 Agent 提交同时更新原生 provider_request 默认身份版本；CLI 服务商预设另行排除 | 本地有这些配置，不能按提交标题全删 | 需适配 | 只抽原生请求配置；版本是否适用单独核验，不搬外部运行时 |

### G · 定时聊天任务（新增功能组）

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [G01](https://github.com/ZMGID/kivio/blob/92dec02adf137f19b01670f3edca4a953ed45838/src-tauri/src/scheduled_tasks/types.rs) **定时规则和编辑器** | 单次、间隔、每日、每周、每月、每年、五段 cron，展示下次执行时间 | 本地无 scheduled_tasks 模块 | 可选 | 本组新能力；时间为本机时区，闰日/不存在日期有明确规则 |
| [G02](https://github.com/ZMGID/kivio/blob/92dec02adf137f19b01670f3edca4a953ed45838/src-tauri/src/scheduled_tasks/mod.rs) **任务固定绑定聊天** | 保存时选择既有聊天或创建一个聊天，此后执行均进入该聊天 | 本地自动化不等于这个聊天定时器 | 可选 | 依赖 G01；删除目标聊天后停用；Dsivio 只绑定内置运行时 |
| [G03](https://github.com/ZMGID/kivio/blob/92dec02adf137f19b01670f3edca4a953ed45838/src-tauri/src/chat/commands/send.rs) **忙时排队与重复派发控制** | 目标聊天运行时排队，避免同一计划不断叠加或并发发送 | 本地没有这套独立派发状态 | 可选 | 依赖 G01/G02，接现有发送队列和运行状态 |
| [G04](https://github.com/ZMGID/kivio/blob/92dec02adf137f19b01670f3edca4a953ed45838/src-tauri/src/scheduled_tasks/store.rs) **任务执行记录与恢复** | 显示排队、运行、成功、失败、跳过、中断；退出后未完成任务有明确状态 | 本地无此定时任务账本 | 可选 | 不能承诺 App 关闭后仍后台执行；测试重启和目标删除 |
| [G05](https://github.com/ZMGID/kivio/blob/24b5707b6327174e9f154ba624349b73a64a71dc/src-tauri/src/scheduled_tasks/mod.rs) **定时状态持久化事务保护** | 保存失败不改变已提交状态，旧运行完成不能释放新等待者 | 属于新定时器的重要后续修复 | 可选 | 选择 G01—G04 时必须带上；不能只合首版 |
| [G06](https://github.com/ZMGID/kivio/blob/24b5707b6327174e9f154ba624349b73a64a71dc/src-tauri/src/scheduled_tasks/mod.rs) **暂停恢复与错过时段处理** | 恢复从当前时间计算；长时间错过的触发记录为跳过，避免补发风暴 | 本地无对应规则 | 可选 | 依赖 G01—G05；间隔锚点、睡眠、重启需回归 |
| [G07](https://github.com/ZMGID/kivio/blob/8c0780c8c08ca9d92e6b8b6edbc1df25852b3e46/src/chat/scheduledTasks/TasksCenter.tsx) **聊天创建定时任务和自动化** | 模型工具及页面入口可通过聊天创建，模型创建任务绑定当前会话 | 本地有自动化工具基础，没有新任务创建组合入口 | 可选 | 92dec02a 与 8c0780c8 配套；不是自动创建任务的默认授权 |
| [G08](https://github.com/ZMGID/kivio/blob/92dec02adf137f19b01670f3edca4a953ed45838/src/chat/scheduledTasks/TasksCenter.tsx) **侧栏任务中心和旧路由兼容** | “任务”下有定时任务/自动化两个页签，记住选择，自动化画布可独立展开 | 本地上次已把自动化提到主导航，尚无任务中心 | 可选 | 保持 Dsivio 两种侧栏形态和路由注册；旧自动化入口仍可打开 |

### H · 原自动化模块（不是媒体工作流编辑器）

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [H01](https://github.com/ZMGID/kivio/blob/a00ad75a4f9ac537221a84637e688fa121ab388d/src-tauri/src/automation/storage.rs) **未完成草稿可保存** | 停用的自动化允许保存未填完的节点，保存失败保留编辑并可重试 | 本地有自动化，未包含完整新版生命周期 | 建议合入 | 落在 src/chat/automation；不要修改当前未提交的媒体工作流 |
| [H02](https://github.com/ZMGID/kivio/blob/a00ad75a4f9ac537221a84637e688fa121ab388d/src-tauri/src/automation/validate.rs) **启用和运行共用校验** | 停用节点不要求配置完整；启用或整图运行必须满足统一可执行检查 | 本地现有校验入口尚未收拢成这一版 | 建议合入 | 与 H01 同批，避免“能启用但一运行就失败” |
| [H03](https://github.com/ZMGID/kivio/blob/a00ad75a4f9ac537221a84637e688fa121ab388d/src/chat/automation/RunDetails.tsx) **历史运行详情** | 查看节点输入、输出、失败和耗时；已删除节点仍显示历史记录；可定位现有节点 | 本地没有新版 RunDetails | 可选 | 说明快照缺失或超保存上限；不把当前节点名称当历史名称 |
| [H04](https://github.com/ZMGID/kivio/blob/a00ad75a4f9ac537221a84637e688fa121ab388d/src/chat/automation/RunStatusCapsule.tsx) **运行状态和编辑离开反馈** | 显示当前执行受理错误而非上次成功；选择历史记录与实时运行分开 | 本地缺这轮状态交互完善 | 建议合入 | H01/H02 与 E06 联合验证保存、运行和离开 |

### I · 插件框架与市场入口

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [I01](https://github.com/ZMGID/kivio/blob/143153c20aaf9f8ee1ece905ca89d5fb53adf445/src-tauri/src/plugins/marketplaces.rs) **外部插件市场发现** | 读取 Claude 格式 marketplace、增加来源、刷新、浏览插件包 | 本地有自己的应用市场，没有 plugins/marketplaces.rs | 可选 | 插件格式兼容不需要恢复 Claude 运行时；保留现有本地插件入口 |
| [I02](https://github.com/ZMGID/kivio/blob/143153c20aaf9f8ee1ece905ca89d5fb53adf445/src-tauri/src/plugins/packages/details.rs) **插件组件详情与选择安装** | 统一展示 Skill/MCP/命令等包内容，按实际组件安装、启停、卸载 | Dsivio 已有包管理和内容面板，双方实现分化 | 需适配 | 先对齐能力缺口，保留 a42d726b 的未选 symlink 处理 |
| [I03](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src/chat/market/MarketPage.tsx) **没有主 Skill 的插件也能使用** | 新包不要求固定 skillId，可打开普通聊天按需调用包内技能 | Dsivio 的使用入口依赖 skillId/项目创建等额外语义 | 需适配 | 30 个新包的共同依赖；保留 Dsvideo、Dsimage、紫鸟和项目初始化 |
| [I04](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/catalog.json) **插件图标、分类与许可证** | 新增分类、各插件图标及图标来源许可记录 | 本地目录、品牌、插件集合与上游不同 | 可选 | 随实际选中的插件包同步，不能覆盖本地 catalog.json |
| [I05](https://github.com/ZMGID/kivio/blob/bd1b6d92c337139f9d823dd9cb20d042817ad6f9/src/chat/market/PluginCenterHeading.tsx) **Skill/MCP 放入插件导航** | 插件、Skill、MCP 共用入口标题；自动化上移侧栏 | 本地已在 94425fc9 合入相应导航 | 已有实现 | 不重复做一遍；新任务中心 G08 是另一项选择 |
| [I06](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/catalog.json) **飞书/企业微信原有包调整** | 原有办公插件目录和图标随市场升级调整 | Dsivio 已有办公插件和本地内置资源 | 需适配 | 不是本次新增 30 包；先保留本地业务技能和安装行为 |

### J · 媒体创作（按 Dsivio 单一网关适配）

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [J01](https://github.com/ZMGID/kivio/blob/37db36caffcb5191d002caa8f08924f4e1ce0b4b/src-tauri/src/chat/media_station.rs) **上游独立媒体工作站** | 新增统一图片/视频表单、任务列表和存储目录 | Dsivio 已有图片视频工作台与媒体创作配置，架构不同 | 不合入 | 不新增第二套媒体入口/存储负责人；保留 ADR 0009/0012 |
| [J02](https://github.com/ZMGID/kivio/blob/6cd0bc978de02d3d696a424354ea613ec3477338/src-tauri/src/chat/video_generation.rs) **上游原生视频厂商适配** | 在 chat/video_generation 下新增 xAI、MiniMax、Ark、DashScope 执行 | Dsivio 已在 media_generation 统一处理媒体 | 不合入 | 若发现某个协议 bug，只修现有 route；不复制整套适配器 |
| [J03](https://github.com/ZMGID/kivio/blob/2940b03f1d93257da4548d4e0938afdb4d1a72c9/src/chat/MediaStation.tsx) **创建/历史分离与起步示例** | 媒体页分创建、历史，增加起步想法与界面重做 | 本地工作台已有不同业务流程 | 可选 | 仅借鉴界面方案，需用户选择；不随 Agent 修复混入 |
| [J04](https://github.com/ZMGID/kivio/blob/e320d9deb3bf1e0bdd28b52d8a8e2c36158fd30b/src/chat/MediaStation.tsx) **生成中的比例占位画布** | 结果未到时按选定宽高比显示占位，降低布局跳动 | Dsivio 预览链不同，需要检查目标页面是否有相同体验缺口 | 可选 | 适配现有组件，和 D09 一起按需要做 |
| [J05](https://github.com/ZMGID/kivio/blob/6cd0bc978de02d3d696a424354ea613ec3477338/src-tauri/src/chat/media_station.rs) **付费任务回执与恢复只轮询** | 持久化 provider task id，恢复已提交任务不重新扣费提交 | Dsivio ADR 0009 已有提交不确定不重交规则与统一任务 | 已有实现 | 保留更严格的本地语义；需要的是差异回归，不另建任务系统 |
| [J06](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src-tauri/src/chat/artifacts.rs) **媒体删除失败保留可恢复记录** | 清理失败时保留任务元数据；阻止在途索引把已删除作品恢复 | 本地有 delete_media_task，删除实现与数据布局不同 | 需适配 | 在 media_generation 检查并补目标行为；运行中删除规则保持现有约定 |
| [J07](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/chat/MediaStation.tsx) **结果继续编辑与按参数重生成** | 选择既有图片作参考，或复用视频参数创建新任务，不改写旧任务 | Dsivio 已有图片视频项目及任务结果流程 | 可选 | 先确认具体页面缺口；再次生成属于新请求，不能静默提交 |

### K · 桌面与公共控件外观

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [K01](https://github.com/ZMGID/kivio/blob/64eef3a12a7d3f5a442b63595d09928c2f154d43/src/styles/chat-01-main.css) **紧凑标题大纲且不挤正文** | 大纲变小，展开时不重新挤压回答排版 | 用户此前已要求本地横杆收拢，不能原样覆盖尺寸 | 需适配 | 只补排版稳定性，保留已确认的紧凑视觉 |
| [K02](https://github.com/ZMGID/kivio/blob/c5944f9a703d4dbed058a96e46f0db39fe5410f4/src/styles/chat-01-main.css) **正式构建大纲刻度可见** | 修正生产 CSS 压缩后标记宽度变零的问题 | 本地尚未包含这轮正式构建修复 | 建议合入 | 与 K01 可分开，生产构建验证；不改变横杆偏好 |
| [K03](https://github.com/ZMGID/kivio/blob/1f6dad8ef506b696eb19a9535b2363e600eb7830/src/index.css) **只有键盘操作显示焦点环** | 鼠标点击不残留突兀 focus ring | 本地已有等价提交 3b061fcf | 已有实现 | 跳过重复实现，公共控件回归时保留 |
| [K04](https://github.com/ZMGID/kivio/blob/92dec02adf137f19b01670f3edca4a953ed45838/src/components/AppDialog.tsx) **公共弹窗焦点返回** | 原生 dialog 的打开关闭正确恢复焦点，嵌套确认不抢错层 | 本地有 AppDialog，尚未含任务提交带来的该修复 | 建议合入 | 可从 92dec02a 单独抽出，不要求引入整个任务中心 |
| [K05](https://github.com/ZMGID/kivio/blob/92dec02adf137f19b01670f3edca4a953ed45838/src/settings/components.tsx) **禁用表单和弹窗右键菜单** | fieldset 禁用时 Select 不能操作；TextArea 菜单留在 dialog 内，过期粘贴无效 | 本地共享控件仍缺这一轮边界保护 | 建议合入 | 独立合入并验证全部复用页面，不局部复制控件 |
| [K06](https://github.com/ZMGID/kivio/blob/88bae83a6c229299949764dfa9e0726f461c04a1/src/settings/NavIcons.tsx) **插件/Skill 默认图标及导航图标** | 新增专用缺省图标，重画设置导航，去掉按钮按压缩放 | 本地有自定义导航图标，属于视觉选择 | 可选 | 7f008905、88bae83a 按图标取，不连带替换侧栏 |
| [K07](https://github.com/ZMGID/kivio/blob/5ef4e4e08ae1318e59b345a6941192ccecd8c39a/src/theme/theme.ts) **完整主题库和自定义导入导出** | 语义色板、内置主题、明暗方案、自定义主题 JSON | 本地没有新版 theme 模块，大量业务页仍使用当前主题变量 | 可选 | 范围较大；Dsivio 工作台/媒体/插件页也要迁移，不能只搬上游 124 文件 |
| [K08](https://github.com/ZMGID/kivio/blob/5ef4e4e08ae1318e59b345a6941192ccecd8c39a/public/theme-bootstrap.js) **刷新前恢复主题减少白闪** | HTML 启动阶段使用缓存主题先绘制，再与设置同步 | 本地有品牌冷启动流程，未采用这个 bootstrap | 需适配 | 可独立借鉴但不能删除点阵启动；核验 system/light/dark |
| [K09](https://github.com/ZMGID/kivio/blob/73b7fba3ff713b2e9ff064f09bec43926559a007/src-tauri/src/windows/traffic_lights.rs) **macOS 交通灯对齐与恢复** | 窗口布局、缩放、复用时测量并恢复原生按钮位置 | 本地窗口几何和侧栏定制不同 | 需适配 | b3eaa063 与 73b7fba3 一起；原生窗口实机验收 |
| [K10](https://github.com/ZMGID/kivio/blob/dbff027d16156ff21f4bebe62a24f6d0f4c5083d/src/styles/chat-01-main.css) **齐边分栏和标题栏布局** | 移除部分浮动圆角/间距，标题栏与侧栏宽度一致 | 本地有自己窗口布局，不属于纯缺陷修复 | 可选 | dbff027d 的整套视觉单独选；不能为交通灯强制改全部布局 |
| [K11](https://github.com/ZMGID/kivio/blob/dbff027d16156ff21f4bebe62a24f6d0f4c5083d/src/chat/Sidebar.tsx) **窄侧栏页签不挤坏** | 最近/集/项目标签和操作区可换行且不被压缩 | 本地仍用定制侧栏 | 建议合入 | 只取布局修复，保留对话/工作台两种形态 |
| [K12](https://github.com/ZMGID/kivio/blob/e929b6bc859e942330de81a36e7a9f2b2410b1af/src/chat/Sidebar.tsx) **平铺会话列表** | 经典/平铺切换，按集、项目、未分组筛选，并在选中范围新建聊天 | 本地没有这轮平铺导航 | 可选 | 不改变 ADR 0004 手排与钉住语义；区别于侧栏“形态” |
| [K13](https://github.com/ZMGID/kivio/blob/e929b6bc859e942330de81a36e7a9f2b2410b1af/src/chat/ProjectIcon.tsx) **项目图标** | 平铺/项目列表显示项目目录图标，增加文件系统图标读取 | 本地已有项目与工作台展示，未含该图标实现 | 可选 | 可与 K12 分开；验证图标读取路径与无图标回退 |

### L · 集备份与恢复

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [L01](https://github.com/ZMGID/kivio/blob/0830a72601a77becd4f1557d0c8e29e4d28655f0/src-tauri/src/chat/storage/set_backup.rs) **集完整导出和导入** | 携带集提示词、默认助手、对话、附件和钉住位置，重映射导入 ID | 本地有设置/聊天备份基础，没有这套完整 set_backup | 建议合入 | 0830a726、0c0e9461、7badbafd 取最终版本；保留 Dsivio 数据格式 |
| [L02](https://github.com/ZMGID/kivio/blob/0c0e9461c077f0beacd1590b4f651a8d1a305c12/src/settings/useSettingsBackupController.ts) **集纳入设置备份入口** | 设置备份含集及默认助手，取消早期新增的侧栏单独备份按钮 | 本地设置备份尚缺新版覆盖 | 建议合入 | 依赖 L01；最终入口在设置，不按中间提交另加按钮 |
| [L03](https://github.com/ZMGID/kivio/blob/7badbafd50119a38012b229a994438aa0f37e1c9/src-tauri/src/chat/storage/set_backup.rs) **失败清理与绑定兼容** | 导入失败清理本次新文件；对话标识与存储一致，处理重复绑定 | Dsivio 只读外部 CLI 历史需保留，不得恢复可续聊运行时 | 需适配 | L01 必带数据完整性保护；CLI 可运行绑定部分排除 |

### M · 不恢复的外部运行时与桌宠

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [M01](https://github.com/ZMGID/kivio/blob/f6b1a791abdc5e2226cd8a606e5c672bce251604/src-tauri/src/external_agents/session/codex_app_server.rs) **Codex 中断、模型列表、原生历史导入** | turnId 中断、live model/list、分页 rollout、excludeTurns | 外部 Codex 运行时已从 Dsivio 删除 | 不合入 | 保留本地历史只读，不恢复 external_agents |
| [M02](https://github.com/ZMGID/kivio/blob/1721a3d61ad12f01c994cd776d3ef2468a9fb166/src-tauri/src/external_agents/stream/claude.rs) **Claude 交互、系统提示快照和配额反馈** | MCP elicitation、URL 授权、恢复时更新提示快照、模型回退和额度提示 | 外部 Claude 运行时已删除 | 不合入 | 不是 A 组原生 Ask，也不是 F 组原生 API 身份配置 |
| [M03](https://github.com/ZMGID/kivio/blob/48629d71aea4c459502b2ec3546cdc1da2f34b84/src-tauri/src/external_agents/session/codex_app_server.rs) **Codex Plan 与异步问题协议** | 只读 plan、collaborationMode、异步问答原始索引、review/警告信息 | 对应外部运行时，不适用本地内置 Agent | 不合入 | 48629d71、4728ba51 中原生 F 组内容另取 |
| [M04](https://github.com/ZMGID/kivio/blob/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b/src/settings/PiExtensionsSettings.tsx) **外部运行时安装/导入生命周期** | CLI 导入、Pi 扩展/Skill 的跨导航操作恢复 | 这些管理能力已删除 | 不合入 | 不因 E 组 windowStore 恢复旧入口和设置页 |
| [M05](https://github.com/ZMGID/kivio/blob/bb8d154a6075c11e75c2f9a5b8d3d2c971cf10cf/src-tauri/src/desktop_pet/mod.rs) **Momo 原生桌宠** | 桌面窗口、拖动、行为模拟与跨平台集成 | 本地没有该能力，与电商 Agent 核心无关 | 不合入 | 可作为独立产品需求，不夹带到 Agent 修复 |
| [M06](https://github.com/ZMGID/kivio/blob/eb9f84c367240e3b6b64d3a4452ce273f7161299/src-tauri/src/desktop_pet/visual.rs) **桌宠短句与姿态更新** | 对话气泡、表情、明暗样式和动作素材后续增强 | 依赖 M05，本地未接入 | 不合入 | daca3346、eb9f84c3 随桌宠一起排除 |

### N · 测试、构建、文档和历史整理

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [N01](https://github.com/ZMGID/kivio/blob/dda735ab97a1bd8f162ebe32d991eac15c935ebd/scripts/check-desktop-package.test.mjs) **打包测试按全部资源映射构造样本** | 不再只复制 skills/licenses，新插件等资源自动纳入测试 fixture | 本地有更多媒体运行时和插件资源，不能照搬配置 | 需适配 | 只迁移 fixture 规则，保持 Dsivio 资源、名称、版本和发布源 |
| [N02](https://github.com/ZMGID/kivio/blob/f0edd84aef25c622b7ba82227191b23754537be7/src/chat/hooks/useConversationContext.test.tsx) **Node 20 延迟 Promise 测试兼容** | 替换不兼容的 Promise 测试写法，保留先后顺序断言 | 按本地运行环境及引入的测试决定 | 需适配 | e5de55ac、f0edd84a 随相关回归测试一起取 |
| [N03](https://github.com/ZMGID/kivio/blob/da0bc51a1548c89c55ad816d9ecb8cc4c2d96a3b/src/theme/theme.test.ts) **主题 bootstrap 测试加载修正** | 用 Vite 正确加载主题启动脚本 | 本地尚未引入新主题 | 可选 | 只在选择 K07/K08 后引入，不单独添加无主测试 |
| [N04](https://github.com/ZMGID/kivio/blob/403819c7e01cca7d7bbb2924eb36cfbb004fcf32/src/chat/segments.ts) **大规模删除测试和旧 helper** | 上游 403819c7、8c0780c8 混有测试精简及死代码清理 | Dsivio 有独立回归与绿色基线提交 | 不合入 | 不整批删除；仅删本次迁移证实不再使用的部分 |
| [N05](https://github.com/ZMGID/kivio/blob/8470af28f366719babbd05f9f188208e22e82602/src-tauri/src/chat/model/anthropic.rs) **全仓 Rust 格式化** | 无业务目的的大范围 rustfmt | 本地分叉较大，会放大冲突 | 不合入 | 只格式化真正修改的文件，不单独 cherry-pick |
| [N06](https://github.com/ZMGID/kivio/blob/154dde505375c3c4280cd733813e0aa1d24a3789/package.json) **发布版本、README 与星标图** | 3.1.0/3.1.1 版本、下载地址、发布说明、星标图和 QA 记录 | Dsivio 有独立品牌、版本与发布仓库 | 不合入 | 上游说明只作证据；不把上游验收写成本地已通过 |
| [N07](https://github.com/ZMGID/kivio/blob/84f14d76ed5ce8dc3996fbfdfb9aa71f1de481c7/scripts/probe-chat-history.playwright.js) **新增回归与原生验证脚本** | 聊天滚动、原生交互、性能和历史分页探针 | 本地已有部分基线，脚本选择性复用 | 需适配 | 保留品牌选择器和环境路径；脚本存在不代表本轮已运行 |
| [N08](https://github.com/ZMGID/kivio/blob/2cf0893711fd1126ed2c61a6c5889869234dbeb8/src-tauri/src/chat/video_analysis.rs) **免 Key 视频模型测试修正** | 视频分析模型测试接受免 Key 的有效选择 | 本地视频/媒体调用路径经过适配 | 需适配 | 选择 F10 时带相应回归；不恢复上游视频生成实现 |
| [N09](https://github.com/ZMGID/kivio/blob/6cb64d1d751445de6df047be076f12277140523e/src-tauri/src/provider_oauth/antigravity.rs) **macOS OAuth 测试端口释放判断** | 以连接被拒绝判断 listener 已关闭，避开 TIME_WAIT 导致同端口重绑定不稳 | 两仓当前差异中还有这条测试改进 | 建议合入 | 仅修改测试判断，不改 OAuth 运行行为 |

### Z · Dsivio 必须保留的定制边界

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [Z01](/Users/zmmini/zmdata/work/Dsivio/docs/dsivio-alignment.md) **Dsivio 品牌和发布** | 保留名称、图标、版本、更新源、葡萄牙语与点阵启动/切换动画 | 本地专有定制，整树比较会表现为被上游覆盖 | 保留本地 | 不可用 Kivio release/config/CSS 整文件覆盖 |
| [Z02](/Users/zmmini/zmdata/work/Dsivio/src/chat/workbench/registry.ts) **工作台侧栏与单一功能注册表** | 保留双侧栏形态、工作台首页、业务页注册和当前首页重做 | 大量 Dsivio 专有文件在上游不存在 | 保留本地 | 遵守 ADR 0006/0007；不存在不等于上游要求删除 |
| [Z03](/Users/zmmini/zmdata/work/Dsivio/src-tauri/src/media_generation.rs) **统一媒体任务与 CLI 客户端** | 保留 media_generation、app_cli、回执、幂等和不确定不重交 | 上游是另一套 chat/media_station 架构 | 保留本地 | ADR 0009/0012；所有借鉴媒体行为落回这里 |
| [Z04](/Users/zmmini/zmdata/work/Dsivio/docs/adr/0010-media-gateway-describes-models-and-owns-local-asr.md) **语音、ASR、取消和模型参数描述** | 保留系统 TTS、云语音、WhisperX 生命周期和真实参数校验 | Dsivio 已有自己的统一能力，上游不能替代 | 保留本地 | ADR 0010；取消事实不能简化成客户端 abort |
| [Z05](/Users/zmmini/zmdata/work/Dsivio/src-tauri/src/chat/agent/execute.rs) **电商平台与内容发布** | 保留 commerce/publish、账号权限、结果分类和批准路径 | 上游没有这些电商领域代码 | 保留本地 | 尤其保留 agent/execute.rs 的 commerce 批准与媒体超时定制 |
| [Z06](/Users/zmmini/zmdata/work/Dsivio/src/chat/market/MarketPage.tsx) **Dsvideo/Dsimage/紫鸟及项目初始化** | 保留内置插件、本地运行时、模式配置和项目创建/提示注入 | 上游插件目录和使用入口不能覆盖这些功能 | 保留本地 | I 组适配必须回归；资源缺失不能切换到 PATH 安装方案 |
| [Z07](/Users/zmmini/zmdata/work/Dsivio/src-tauri/src/chat/agent/prepare.rs) **项目 AGENTS.md/CLAUDE.md 指令** | 继续读取项目约束并纳入 Agent 上下文 | 本地 f7af1c00 新增，上游上下文重做可能碰同一文件 | 保留本地 | B/F 组迁移必须保留，不能因 prepare.rs 替换丢失 |
| [Z08](/Users/zmmini/zmdata/work/Dsivio/src/chat/AskUserBlock.tsx) **已有 Ask 同步、防闪与大纲紧凑修复** | 保留本地交互快照、作品防闪与用户确认的紧凑横杆 | 这些是过去针对本地问题的修复 | 保留本地 | A/C/D/K 组按行为叠加；切换对话才出现的问题单独回归 |
| [Z09](/Users/zmmini/zmdata/work/Dsivio/docs/handoff/complete-workflow-editor.md) **工作流编辑器未提交改动** | 保留当前 workflow、generation_workflow、commerce、i18n 和工作台样式改动 | 检查开始时 29 个未提交路径，包括新增测试 | 保留本地 | 本轮不覆盖、不暂存；后续合并前重新检查状态 |
| [Z10](/Users/zmmini/zmdata/work/Dsivio/src/chat/chatNavigationController.ts) **现有导航和历史读取负责人** | 保留 chatNavigationController、useConversationHistory、路由 keep-alive 和超时恢复 | 上游在 Chat.tsx 的组织方式与本地不同 | 保留本地 | C/G/E 组通过本地负责人接入，不提高协调页行数上限 |

### P · 新增 30 个插件包（每包可独立选择）

| 编号 / 实现 | 上游具体变化 | Dsivio 当前情况 | 建议 | 依赖与边界 |
| --- | --- | --- | --- | --- |
| [P01](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/github/.kivio-plugin/plugin.json) **GitHub** | 通过 gh 处理 Pull Request、Issue 和 Actions；需要 GitHub 账号授权。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P02](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/gitlab/.kivio-plugin/plugin.json) **GitLab** | 通过官方 glab 处理 Merge Request、Issue 与 CI/CD，支持自托管 GitLab。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P03](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/aliyun/.kivio-plugin/plugin.json) **阿里云 CLI** | 查询阿里云资源并执行明确授权的变更；需要对应 RAM 权限与地域配置。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P04](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/docker/.kivio-plugin/plugin.json) **Docker** | 检查容器、镜像和 Compose 项目；需要可访问的 Docker Engine。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P05](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/kubernetes/.kivio-plugin/plugin.json) **Kubernetes** | 通过 kubectl 诊断工作负载与发布状态；需要集群配置和相应 RBAC。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P06](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/helm/.kivio-plugin/plugin.json) **Helm** | 检查与部署 Helm Chart；需要 Kubernetes 集群，渲染可在本地完成。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P07](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/terraform/.kivio-plugin/plugin.json) **Terraform** | 审阅基础设施配置与执行计划；真实变更需要云凭证和明确授权。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P08](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/aws/.kivio-plugin/plugin.json) **AWS CLI** | 查询 AWS 资源和 CloudWatch 日志；需要 profile、region 与 IAM 权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P09](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/azure/.kivio-plugin/plugin.json) **Azure CLI** | 查询 Azure 资源组、资源与部署；需要租户登录和订阅权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P10](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/digitalocean/.kivio-plugin/plugin.json) **DigitalOcean** | 通过 doctl 查询 Droplet、应用与集群；需要 DigitalOcean API 授权。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P11](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/vercel/.kivio-plugin/plugin.json) **Vercel** | 检查 Vercel 项目、预览部署与构建日志；需要账号和项目访问权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P12](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/netlify/.kivio-plugin/plugin.json) **Netlify** | 检查 Netlify 站点、构建和草稿部署；需要账号及站点权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P13](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/cloudflare/.kivio-plugin/plugin.json) **Cloudflare Wrangler** | 开发与部署 Cloudflare Workers，检查 KV/R2；需要目标账户权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P14](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/supabase/.kivio-plugin/plugin.json) **Supabase** | 管理 Supabase 本地开发、迁移和 Edge Functions；本地栈需要 Docker。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P15](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/firebase/.kivio-plugin/plugin.json) **Firebase** | 检查 Firebase 项目、模拟器与 Hosting；需要 Google 项目权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P16](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/sentry/.kivio-plugin/plugin.json) **Sentry CLI** | 管理 Sentry Release、Source Map 与部署记录；需要组织和项目权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P17](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/stripe/.kivio-plugin/plugin.json) **Stripe CLI** | 调试 Stripe 测试事件和 Webhook；需要 Stripe 登录，默认使用测试环境。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P18](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/dingtalk/.kivio-plugin/plugin.json) **钉钉 Workspace** | 通过 dws 处理钉钉消息与文档；当前需企业管理员授权开通。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P19](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/tencent-meeting/.kivio-plugin/plugin.json) **腾讯会议** | 查询会议、录制和参会信息；需要腾讯会议 OAuth 与相应账号权限。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P20](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/google-workspace/.kivio-plugin/plugin.json) **Google Workspace CLI** | 通过 gws 处理 Drive、Gmail 与 Calendar；需 OAuth 项目，非 Google 官方支持产品。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P21](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/obsidian/.kivio-plugin/plugin.json) **Obsidian** | 管理 Obsidian 笔记、属性和 Canvas；CLI 需要桌面应用运行并启用。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P22](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/pandoc/.kivio-plugin/plugin.json) **Pandoc** | 转换 Markdown、DOCX、HTML 等文档；PDF 输出需要额外渲染引擎。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P23](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/quarto/.kivio-plugin/plugin.json) **Quarto** | 生成可复现报告、技术文档和幻灯片；代码块可能需要 Python 或 R。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P24](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/ffmpeg/.kivio-plugin/plugin.json) **FFmpeg** | 分析音视频、转码、剪辑和提取音轨；能力取决于本机编译的编解码器。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 需适配 | 依赖 I03；改用 Dsivio 已打包媒体运行时和 dsivio media，不新增安装/执行链 |
| [P25](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/imagemagick/.kivio-plugin/plugin.json) **ImageMagick** | 批量缩放、裁剪、转换和拼图；使用 ImageMagick 7 的 magick 命令。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P26](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/yt-dlp/.kivio-plugin/plugin.json) **yt-dlp** | 下载获准保存的网络音视频；部分站点需 FFmpeg、登录或额外运行时。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P27](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/rclone/.kivio-plugin/plugin.json) **Rclone** | 列出、复制和核对云盘文件；需要用户已有 remote，同步可能删除目标数据。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P28](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/sqlite/.kivio-plugin/plugin.json) **SQLite** | 检查本地 SQLite 数据库并查询数据；默认只读，不需要云账号。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P29](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/duckdb/.kivio-plugin/plugin.json) **DuckDB** | 分析本地 CSV、Parquet 与 DuckDB 数据；默认只读，远程扩展按需启用。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |
| [P30](https://github.com/ZMGID/kivio/blob/0413f0d69ddc80421dd6d78b81718ab81c090284/src-tauri/resources/plugins/bundled/httpie/.kivio-plugin/plugin.json) **HTTPie** | 构造和诊断 HTTP API 请求；认证沿用用户本地安全配置。 | 上游新增内置包；不代表 Dsivio 或用户环境没有同类工具 | 可选 | 依赖 I03，资源映射按 N01；首次使用仍需相应 CLI/账号/权限，未做真实业务验证 |

## 依赖与验收（功能关联，不代表执行先后）

| 分组 | 编号 | 必须一起处理的原因 | 合入后最低验收 |
| --- | --- | --- | --- |
| 数据保护 | D01—D03、D07 | GC 引用、文件迁移与原数据保留必须一致 | 老会话 PDF/工具产物仍可打开；迁移写失败不丢字节；迟到 embedding 不复活已删文档 |
| Ask 与待办 | A01—A05、C05 | 前后端交互契约、todo revision 与页面订阅同时生效 | 新旧会话直接弹 Ask；中文、数字、多题；旧快照返回不覆盖新待办；收尾提醒有界 |
| 长对话与输入 | C01—C15、C17、K02 | 过程裁剪、虚拟列表、滚动跟随、草稿和历史变更有关联 | 百轮工具过程；阅读中持续生成；macOS 流式结束不跳；历史加载失败重试；引用只插一次；删除答案保留其他轮次 |
| 原生请求协议 | F01—F07、F13 | 身份头、正文、工具名及 OAuth 必须一致 | 普通/身份、自定义头、流式/非流式、OAuth；大小写不同的 MCP 工具各自执行 |
| 上下文与费用 | B01—B11，关联 C11/F 组 | 跨端协议、压缩算法和账本口径不能各自升级 | 各供应商 usage fixture；迟到事件；换模型/删回答失效；超窗/取消/重开；父子费用去重，缺价标未知 |
| 页面操作生命周期 | E01—E10、D04、D06、H01/H02/H04 | 每项归原领域负责人，共享支撑仅供真实调用者复用 | 保存中继续编辑、切走返回、重复点击、失败重试、删除后迟到结果、订阅释放 |
| 任务中心 | G01—G08，关联 H/E06/K04/K05 | 首版调度和持久化后续修复不可拆出不完整版本 | 本机时区/闰日/cron；忙时排队；睡眠/重启/暂停恢复；目标删除；保存失败无状态漂移；旧路由兼容 |
| 插件扩展 | I01—I04，P 项逐个选 | 无主 Skill 包需 I03；资源映射和本地项目插件兼容必须先解决 | 包导入/启停/卸载；Dsimage/Dsvideo/紫鸟回归；每个选中 CLI 做真实环境检查 |
| 备份 | L01—L03 | 后端 ID、附件、钉住与最终设置入口一起 | 导出后导入新环境；默认助手、提示词、附件、位置一致；中途失败清理；本地只读 CLI 历史不变可运行 |
| 桌面外观 | K07—K13 按需选 | 主题、几何与平铺列表不是 Agent 修复的强制依赖 | macOS 交通灯/缩放/复用；浅深色/系统主题；窄侧栏；Dsivio 工作台和媒体页全链路 |

实际实施仍执行 [统一工程规范](../engineering-standards.md)、适用 ADR 和 `package.json` 中现有协议/类型/Lint/架构/测试检查。本表只补充功能验收场景，不另建规范。测试范围随实际选择收敛，不能用上游已记录的通过结果替代本地验证。

## 近期 60 个提交覆盖表

以下按上游第一父链时间顺序，列出每个提交对应的决策项。测试/文档/版本提交也保留，避免看起来像遗漏。提交链接用于追溯，**不是整提交合入建议**。

| 提交 | 原始标题 | 变化路径数 | 对应决策项 |
| --- | --- | ---: | --- |
| [92dec02a](https://github.com/ZMGID/kivio/commit/92dec02adf137f19b01670f3edca4a953ed45838) | feat: separate conversation scheduled tasks from workflow automation | 60 | G01、G02、G03、G04、G07、G08、K04、K05 |
| [dda735ab](https://github.com/ZMGID/kivio/commit/dda735ab97a1bd8f162ebe32d991eac15c935ebd) | fix(ci): populate packaging fixtures from all resource mappings | 2 | N01 |
| [c6029f75](https://github.com/ZMGID/kivio/commit/c6029f756a6821188d22ed2be0a8e811020f97ef) | fix(chat): always offer custom text in ask_user and harden its input | 3 | A01、A02、A03 |
| [1f6dad8e](https://github.com/ZMGID/kivio/commit/1f6dad8ef506b696eb19a9535b2363e600eb7830) | fix(ui): only show focus rings after keyboard input | 2 | K03 |
| [7f008905](https://github.com/ZMGID/kivio/commit/7f008905449b57f195bda3f9ba933197afa1b1b7) | feat(ui): add dedicated default icons for plugins and skills | 4 | K06 |
| [64eef3a1](https://github.com/ZMGID/kivio/commit/64eef3a12a7d3f5a442b63595d09928c2f154d43) | fix(chat): compact the heading outline and stop it reflowing answers | 1 | K01 |
| [f6b1a791](https://github.com/ZMGID/kivio/commit/f6b1a791abdc5e2226cd8a606e5c672bce251604) | fix(external-agents): catch up Claude Code 2.1.287 and Codex 0.160 | 15 | M01、M02 |
| [7bf43791](https://github.com/ZMGID/kivio/commit/7bf437910e2b3c6733f3caff4df8f3116e12983e) | fix(chat): prevent first-response completion flash | 3 | C01 |
| [1721a3d6](https://github.com/ZMGID/kivio/commit/1721a3d61ad12f01c994cd776d3ef2468a9fb166) | feat(external-agents): Claude prompt snapshot, MCP elicitation, fallback and quota notes | 12 | M02 |
| [6cd0bc97](https://github.com/ZMGID/kivio/commit/6cd0bc978de02d3d696a424354ea613ec3477338) | feat(media): redesign media station and add native video adapters | 17 | J01、J02、J05 |
| [48629d71](https://github.com/ZMGID/kivio/commit/48629d71aea4c459502b2ec3546cdc1da2f34b84) | feat(external-agents): Codex plan mode and remaining t3code parity items | 21 | F13、M02、M03 |
| [2940b03f](https://github.com/ZMGID/kivio/commit/2940b03f1d93257da4548d4e0938afdb4d1a72c9) | feat(media): split create and history views, add hand-drawn starter ideas | 4 | J03 |
| [8470af28](https://github.com/ZMGID/kivio/commit/8470af28f366719babbd05f9f188208e22e82602) | style(rust): apply rustfmt to backend modules | 21 | N05 |
| [88bae83a](https://github.com/ZMGID/kivio/commit/88bae83a6c229299949764dfa9e0726f461c04a1) | feat(ui): redraw settings and extension nav icons, drop press-shrink | 6 | K06 |
| [e320d9de](https://github.com/ZMGID/kivio/commit/e320d9deb3bf1e0bdd28b52d8a8e2c36158fd30b) | feat(media): aspect-ratio placeholder canvas while generating | 2 | J04 |
| [24b5707b](https://github.com/ZMGID/kivio/commit/24b5707b6327174e9f154ba624349b73a64a71dc) | fix: guard persistence and stale async results | 9 | D06、D07、E01、G05、G06 |
| [403819c7](https://github.com/ZMGID/kivio/commit/403819c7e01cca7d7bbb2924eb36cfbb004fcf32) | test: prune redundant tests and dead helpers | 49 | N04 |
| [a00ad75a](https://github.com/ZMGID/kivio/commit/a00ad75a4f9ac537221a84637e688fa121ab388d) | feat(automation): complete workflow lifecycle | 13 | H01、H02、H03、H04 |
| [fd855af3](https://github.com/ZMGID/kivio/commit/fd855af32c6eba80efcc7faf5c4d770629e13329) | fix(providers): make model requests consistent with their declared client | 4 | F01、F02、F03、F04、F05、F06、F07 |
| [26058234](https://github.com/ZMGID/kivio/commit/26058234f63ff912245a0854198f893b55064880) | fix(providers): close gaps in CLI identity request shaping | 5 | F01、F02、F03、F04、F05、F06、F07 |
| [d836ef94](https://github.com/ZMGID/kivio/commit/d836ef9478117d294abb567634cf1bc2aa218fe4) | fix(chat): make markdown links and web source titles look clickable | 3 | C16 |
| [bc40a109](https://github.com/ZMGID/kivio/commit/bc40a10984bacf670aafdf3b3cace32f9accb897) | fix(chat): stabilize loading and file references | 14 | C14、C15、D01、D02、D03 |
| [4728ba51](https://github.com/ZMGID/kivio/commit/4728ba517a1ce7624c7c55a553ed7eafa42e3223) | fix(chat): protect artifacts and agent contracts | 8 | D01、F03、M03 |
| [49d9d6e1](https://github.com/ZMGID/kivio/commit/49d9d6e166269d751568192d95d4e9628db664eb) | feat(models): refresh October model catalog | 6 | F08、F09 |
| [7f142b37](https://github.com/ZMGID/kivio/commit/7f142b375ebf0d37ec645d133c47a22f09bed57c) | fix(preview): show video frames and streaming SVG | 6 | D08、D09 |
| [a696d7ea](https://github.com/ZMGID/kivio/commit/a696d7ea87a2b31eb5999f5aa7371d08a6cfa25b) | fix(lifecycle): preserve tasks across navigation | 59 | D04、D05、E01、E02、E03、E04、E05、E06、E07、E08、E09、E10、J06、J07、M04 |
| [bb8d154a](https://github.com/ZMGID/kivio/commit/bb8d154a6075c11e75c2f9a5b8d3d2c971cf10cf) | feat(desktop-pet): add native Momo companion | 15 | M05 |
| [dd2936cc](https://github.com/ZMGID/kivio/commit/dd2936cc8a32f527dd7767ce03a8f1c506c3c769) | fix(chat): stabilize rendering and idle scheduling | 23 | C01、C02、C03、C04、C05、C06、C09、C17、C18 |
| [daca3346](https://github.com/ZMGID/kivio/commit/daca33467403e27a1f31344a861b00bc34d195f5) | feat(desktop-pet): add compact companion chatter | 17 | M06 |
| [5ef4e4e0](https://github.com/ZMGID/kivio/commit/5ef4e4e08ae1318e59b345a6941192ccecd8c39a) | feat(theme): add themes and refresh recovery | 124 | K07、K08 |
| [8c0780c8](https://github.com/ZMGID/kivio/commit/8c0780c8c08ca9d92e6b8b6edbc1df25852b3e46) | feat(chat): reconcile todos and create tasks | 37 | A04、A05、G07、N04 |
| [c7d94a0d](https://github.com/ZMGID/kivio/commit/c7d94a0d3c636dcf7af494fbce54b8fe60bf3e61) | chore: reconcile compaction branch history | 0 | A08 |
| [8b2f3e5c](https://github.com/ZMGID/kivio/commit/8b2f3e5ca98d3652d6e32b7ced8f8addcafc1872) | chore: reconcile todo branch history | 0 | A06 |
| [8350c8ce](https://github.com/ZMGID/kivio/commit/8350c8ce80e62a841d313cb147e1c114b0fbb19e) | docs: record pending chat and task changes | 1 | N06 |
| [da0bc51a](https://github.com/ZMGID/kivio/commit/da0bc51a1548c89c55ad816d9ecb8cc4c2d96a3b) | test(theme): load bootstrap through Vite | 1 | N03 |
| [65390304](https://github.com/ZMGID/kivio/commit/65390304501adde684b761a8bedbcac0f35ae6e1) | chore(release): prepare v3.1.0 | 9 | N06 |
| [e5de55ac](https://github.com/ZMGID/kivio/commit/e5de55ac3f999150f2b3c5f4cb71b1cc2057c3e2) | test: support deferred promises on Node 20 | 2 | N02 |
| [9abb6bd9](https://github.com/ZMGID/kivio/commit/9abb6bd93e66b47e536ff0d7166518682c6203ef) | fix(chat): allow deleting individual model answers | 5 | B04、C11 |
| [c5944f9a](https://github.com/ZMGID/kivio/commit/c5944f9a703d4dbed058a96e46f0db39fe5410f4) | fix(chat): restore selection actions and outline ticks | 6 | C10、K02 |
| [ce772a25](https://github.com/ZMGID/kivio/commit/ce772a2560927d65d64ef8d4c4e835243fd9e289) | fix(chat): align context meter with API usage | 61 | B01、B02、B03、B04、B05、B06、B07、B08 |
| [b3eaa063](https://github.com/ZMGID/kivio/commit/b3eaa0636abc9ea937d43e5d01e7385e772c2663) | fix(macos): keep traffic lights aligned with UI | 7 | K09 |
| [3483e1cd](https://github.com/ZMGID/kivio/commit/3483e1cd9fb1899032cbe1e779ca87c57cc72062) | fix(chat): retain context and total agent costs | 12 | B09、B10、B11 |
| [dbff027d](https://github.com/ZMGID/kivio/commit/dbff027d16156ff21f4bebe62a24f6d0f4c5083d) | fix(chat): align split panes and sidebar tabs | 5 | K10、K11 |
| [73b7fba3](https://github.com/ZMGID/kivio/commit/73b7fba3ff713b2e9ff064f09bec43926559a007) | fix(macos): stabilize traffic light positioning | 4 | K09 |
| [0830a726](https://github.com/ZMGID/kivio/commit/0830a72601a77becd4f1557d0c8e29e4d28655f0) | feat(chat): add set backup export and import | 16 | L01、L03 |
| [0c0e9461](https://github.com/ZMGID/kivio/commit/0c0e9461c077f0beacd1590b4f651a8d1a305c12) | fix(backup): include sets in settings backups | 10 | L01、L02、L03 |
| [e929b6bc](https://github.com/ZMGID/kivio/commit/e929b6bc859e942330de81a36e7a9f2b2410b1af) | feat(sidebar): polish flat conversation list | 13 | K12、K13 |
| [ee55715a](https://github.com/ZMGID/kivio/commit/ee55715a3a5b1f1b906683f44aa42a21c4bff7a1) | feat(models): support keyless endpoints and roles | 50 | F10、F11 |
| [0413f0d6](https://github.com/ZMGID/kivio/commit/0413f0d69ddc80421dd6d78b81718ab81c090284) | feat(market): bundle native plugin capabilities | 166 | I02、I03、I04、I06、P01—P30 |
| [eb9f84c3](https://github.com/ZMGID/kivio/commit/eb9f84c367240e3b6b64d3a4452ce273f7161299) | feat(pet): refresh mascot poses and speech cards | 6 | M06 |
| [84f14d76](https://github.com/ZMGID/kivio/commit/84f14d76ed5ce8dc3996fbfdfb9aa71f1de481c7) | fix(chat): refine drafts and history interaction | 19 | B04、C07、C08、C11、C12、C13、N07 |
| [4354f733](https://github.com/ZMGID/kivio/commit/4354f73375ec4069c020f688577e1ec71396c359) | style(usage): clarify token trend charts | 1 | B12 |
| [8a36dfc8](https://github.com/ZMGID/kivio/commit/8a36dfc8286c6801d9e076a1533338b66456347a) | docs: update feature notes and sidebar QA | 3 | N06 |
| [7badbafd](https://github.com/ZMGID/kivio/commit/7badbafd50119a38012b229a994438aa0f37e1c9) | fix(chat): sync restored bindings and draft input | 2 | C12、L03 |
| [326eed0b](https://github.com/ZMGID/kivio/commit/326eed0bfec27d493d8f74bc22abe0c8459b4add) | Merge remote-tracking branch 'origin/main' | 1 | N06 |
| [f0edd84a](https://github.com/ZMGID/kivio/commit/f0edd84aef25c622b7ba82227191b23754537be7) | test(chat): support Node 20 context regressions | 1 | N02 |
| [154dde50](https://github.com/ZMGID/kivio/commit/154dde505375c3c4280cd733813e0aa1d24a3789) | chore(release): prepare v3.1.1 | 8 | N06 |
| [2cf08937](https://github.com/ZMGID/kivio/commit/2cf0893711fd1126ed2c61a6c5889869234dbeb8) | test(video): accept keyless model selection | 1 | N08 |
| [6c60d9c3](https://github.com/ZMGID/kivio/commit/6c60d9c3625cca4b9af1aff83a7e20b2b16a69e9) | feat(models): add uncensored Qwen variants | 4 | F09 |
| [6cb64d1d](https://github.com/ZMGID/kivio/commit/6cb64d1d751445de6df047be076f12277140523e) | feat(browser): persist Playwright extension token | 11 | F12 |

## 全量快照差异的解释

文件索引覆盖固定 HEAD 与固定上游的全部路径。`A/D/M` 是两个快照之间的比较结果，不是建议的操作。Dsivio 当前有大量工作台、内容发布、媒体运行时和插件资源；上游不存在的本地文件一律不按差异表直接删除。

近期提交之外的差异包括此前已移植但组织不同的 Agent 代码、本地导航与性能修复、市场框架尚未接入的部分、品牌/翻译/发行，以及本地新业务能力。对应项已归入 I/J/N/Z；文件索引中的“基础历史或本地定制差异”是保留并按实际合入接缝再核对，不代表已证明每行都无用或都应合入。

对旧差异中特别抽查：`agent/execute.rs` 是本地 commerce 批准路径和媒体超时；`goal.rs` 主要是品牌；`image_prep.rs` 是供本地媒体模块复用的可见性；`provider_runtime.rs` 是本地 ImageRoute 归属；`provider_oauth/antigravity.rs` 另有 N09 的测试改进。

## 当前工作区保护与完成情况

- 本轮只创建本报告、决策 CSV 和文件索引 CSV；没有合并、提交、修改产品代码或重启项目。
- 用户原有未提交路径在文件索引中标记；未跟踪的 `src/chat/workbench/workflow/useWorkflowEditor.test.ts` 不属于已提交快照差异，单独列为保护对象。
- 报告的推荐是代码审查意见；选择范围后仍要逐项实现、补失败用例、验证依赖，不机械 cherry-pick。
- 后续按功能块一次性确定范围；细项编号用于内部实施和核对依赖，不要求用户逐项作技术选择。
