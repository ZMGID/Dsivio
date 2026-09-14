# Kivio 上下文效率复核：官方资料与实现证据

调查日期：2026-09-14。范围：解释 Kivio、OpenCode、DSH 同题测试的上下文差异，并寻找进一步排查方向。本调查不修改产品代码。

## 资料版本与证据边界

- DeepSeek：本日直接打开的官方 API 文档。它们是滚动更新资料；搜索索引中的旧版内容可能不同。当前模型表将 `deepseek-flash` 映射至 DeepSeek-V4.1-Flash，并说明旧 `deepseek-v4-flash` 别名亦由该模型服务。这不能证明先前测试当天使用的后台快照，比较时还应保留实际请求 model、端点、时间及响应模型标识。[当前模型表](https://api-docs.deepseek.com/quick_start/pricing/)
- OpenCode：官方仓库 `dev` 本次读取固定到提交 `df23b7f9488a38e6f8064a0739d4f8cde86d7cfb`，提交时间 2026-09-13。下文明确研究的是该提交的 `packages/opencode` 实现，不能假定等于用户已安装二进制，亦不能假定新版 `packages/core` 路径行为相同。[固定提交](https://github.com/anomalyco/opencode/commit/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb)
- 官网可证明协议约束、源码可证明该版本的分支逻辑；是否在此次任务触发，必须再对会话原始记录核实。字符统计不能直接替代 tokenizer 或 API 实报，也不能精确地把输入差额全归给 diff。

## 1. 写入回显：OpenCode 确实把展示信息和模型结果分开

OpenCode 的 write 成功结果主体是简短确认，另外附带检测到的 LSP 错误；diff 用在权限请求 metadata。edit 同样将 diff、filediff 放 metadata，模型结果以简短成功确认和必要诊断为主。这支持 Kivio 将成功回显缩短、审阅 diff 单独保留的方向；不能为了缩短而丢掉失败原因或诊断。[write 实现 L47–90](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/tool/write.ts#L47)、[edit 实现 L167–194](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/tool/edit.ts#L167)

OpenCode 的 edit 仍采用 oldString/newString，并非天生不需要重复旧代码。Kivio 编辑参数较长可能包含工具设计、选择修改范围、实现体量和返工等因素，不能只凭一次总量把它认定为工具接口缺陷。[edit 参数](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/tool/edit.ts#L42)

## 2. 不能把“所有工具输出限制 2K 字符”说成 OpenCode 的通常做法

该提交普通工具输出默认上限为 **2,000 行与 50 KiB**，两者同时约束，可由配置覆盖。超出后保存完整输出，回传预览、完整文件路径，以及定向 Grep/分段 Read 的提示；可以选择保留开头或结尾。[truncate.ts L12–39、L76–125](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/tool/truncate.ts#L12)

`compaction.ts` 的 2,000 字符用于其历史文本序列化，不等于正常每轮工具结果预算。旧工具 prune 另有条件：要求配置启用，跳过最近用户轮，保护 `skill`，保留约 40K 工具结果 tokens，候选清理量超过 20K 才标记清理。这里没有“总上下文达到 170K 就压缩”的规则。[compaction 序列化 L25–79](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/session/compaction.ts#L25)、[prune L251–293](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/session/compaction.ts#L251)

建议（工程推断）：按结果性质设预算。成功写入回执可以极短；编译错误、测试失败、代码读取需要足够正文和可追溯全文。优先避免大而无关的查询，其次用有恢复路径的截断，不宜一刀切 2K 后让模型反复重读。

## 3. DeepSeek 工具会话的推理回放是协议要求

DeepSeek 现行 Thinking Mode 文档要求：请求带 `tools` 时，所有此前轮次的 `reasoning_content` 都要回传并进入上下文，甚至此前未实际调用工具的轮次也包括在内；缺失可能导致 400。没有 tools 时则无需回传，传入也会被忽略。不能将旧版纯聊天/R1 经验套到当前工具会话，更不能为降低数字随意删除工具历史的推理。[Thinking Mode](https://api-docs.deepseek.com/guides/thinking_mode/)

OpenCode 对 DeepSeek 会确保 assistant 有 reasoning 字段，并在 interleaved 配置下把推理转入提供方支持的字段。这不是把推理全部清空。[provider transform L287–329](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/provider/transform.ts#L287)

可优化的是减少无效工具轮、错误重试和返工诱发的新推理；已生成的必要推理较多，本身不足以认定框架有 bug。

## 4. Responses 兼容不意味着支持服务端保存历史或自动压缩

DeepSeek Responses 是无状态 API：`previous_response_id`、`store`、`context_management`、`truncation` 等不支持。原生 reasoning item 的明文 content 有效，summary 与 encrypted_content 不支持。请求被接受不代表所有兼容参数生效。因此应核对 Kivio 是否按该提供方实际能力构建历史，而不能仅靠通用 Responses 参数期待服务端压缩或保存。[Responses 兼容表](https://api-docs.deepseek.com/guides/responses_api/#compatibility-details)

此项是能力边界和待审计点，不是已证明本次存在重复回放。UI 同时存展示推理和原生 item，也不能直接推导实际请求发送了两份。

## 5. 缓存、最终上下文、累计消耗、耗时是四种不同指标

DeepSeek Responses 的 cached_tokens 是 input_tokens 的明细，reasoning_tokens 是 output_tokens 的明细，不应分别再加一次。[Responses usage 定义](https://api-docs.deepseek.com/api/create-response/)

DeepSeek 缓存复用匹配的输入前缀；修改较早历史可能使原先完整前缀暂时无法命中，新公共前缀会再建立缓存。缓存不是免费生成，也不代表历史不占上下文。压缩会改变前缀，因此“文本更少”不保证下一轮更便宜或更快，应实测缓存命中、未命中输入和输出量。[缓存机制](https://api-docs.deepseek.com/guides/kv_cache/)

比较建议（测量设计）：同时记录最终 input+output、累计缓存命中/未命中输入、累计输出及其中推理、模型请求耗时、工具耗时、任务总耗时、验收质量。不能用 269K/169K 的比例直接断言耗时或费用也应相同比例。

## 6. 压缩触发必须考虑可用输出空间，但属于待验证边界风险

DeepSeek 文档明确输入与生成 tokens 的总长度受上下文窗口约束。当前模型表为 1M 上下文、最大 384K 输出；不要把旧的 128K 说法当成现行官方上限，实际请求输出预算仍可能更小。[Chat API max_tokens](https://api-docs.deepseek.com/api/create-chat-completion/)、[模型表](https://api-docs.deepseek.com/quick_start/pricing/)

OpenCode 的该实现按 input/context limit 与最大输出预算计算 usable，而非简单固定百分比；有 input limit 的分支使用可配置 reserved，默认至多 20K，亦不能视为所有提供方下的理想实现。[overflow.ts](https://github.com/anomalyco/opencode/blob/df23b7f9488a38e6f8064a0739d4f8cde86d7cfb/packages/opencode/src/session/overflow.ts#L7)

若 Kivio 仅采用 90% 窗口阈值，应检查剩余 10% 是否容纳实际允许的输出及即将进入的大工具结果。这是接近窗口上限时的候选风险；没有证据说明本次 269K 已触发，也不能仅由预算相加推定服务端必然立即报错，可能表现为输出截断。阈值测试应覆盖大输出预算、超大工具结果、压缩失败和无 usage 返回等情形。

## 本轮可落地的调查结论

优先调查并改善成功写入回显、无关大输出、错误重试和实测环境混杂。保留必要推理，使用可恢复且按类型区分的结果预算。压缩阈值和模型元数据属于独立兼容性检查，不能当作这次 100K 差额的既定原因。后续本地源码与会话证据应补在本文并明确区分已发生问题和潜在风险。

## 本地原始记录复核

本地代码基准为 `4e3141a2`，工作区另有用户未提交变更；本轮没有修改产品代码。OpenCode 安装包版本是 `1.18.30`，上述上游源码仅作设计对照，实际行为以下列本机会话为准。

证据入口（原文含用户任务，不复制整份到仓库）：

- [Kivio 会话](C:/Users/11028/AppData/Roaming/com.zmair.kivio/conversations/conv_c88caab1-0213-41b8-ab99-aae1959544a9.json)，读取最终 assistant 的 `model_messages`、`tool_calls`、`usage`、`anchor_usage`。
- [Kivio 单请求实报](C:/Users/11028/AppData/Roaming/com.zmair.kivio/usage/usage-2026-09.jsonl)，筛选该 conversationId 和 `operation=chat_tools_planning`，得到 94 条成功请求；另 1 条标题生成不纳入。
- 在 `E:/ZM database/examkivio/opencode` 执行只读命令 `opencode export ses_f64455e9dffeS7g8UQ32muEvWm`，得到 108 个 assistant step、114 次工具调用。
- [DSH 会话](C:/Users/11028/.dsh/sessions/--E-ZM~0020database-examkivio-dsh--/session-ae3cbdc5-452f-4190-81cb-69e2c186fe9c/session.v3.jsonl.zstd)，按串接的 Zstandard frame 解压，共 492 条事件；85 条 assistant/message、99 次 tool/call、3 次 turn/start。末次实报 `inputTokens=227, cacheReadTokens=176128, outputTokens=541, totalTokens=176896`。只解首个 frame 会误以为文件仅有 session header。

### 实报差额可精确对账，因果贡献不能等同于可节省量

| 项目 | Kivio | OpenCode | 差额 |
|---|---:|---:|---:|
| 最后一轮输入＋输出 | 269,150 | 168,948 | 100,202 |
| 首次输入 | 14,455 | 12,745 | 1,710 |
| 累计推理输出 | 85,719 | 69,671 | 16,048 |
| 累计非推理输出 | 95,971 | 60,670 | 35,301 |
| 其余输入净增长 | 73,005 | 25,862 | 47,143 |

计算：其余输入净增长 = 最后输入＋输出 − 首次输入 − 累计全部输出。Kivio 的 output 已含 reasoning；OpenCode 本会话 output 与 reasoning 分列，须相加。两边各相邻请求的 `下一次完整输入 − 上一次total` 均无负值。两边最终上下文与累计消耗不能混用。OpenCode 会话未发现 compaction part 或 tool.state.time.compacted 标记。

这是一项对账恒等式；其余净增长包含工具返回、图片、消息边界、协议/提示变化等，不是逐段 tokenizer 测量，也不是删掉某类内容后重新完成任务的反事实结果。

### P1：成功写入回显冗长，有原始记录和实现双重证据

Kivio 的 44 次 write/edit 结果合计 92,334 字符，OpenCode 50 次对应结果合计 2,963 字符。Kivio 16 次结果含 `diff clipped`。实际模型回显最多 80 行，而非无限完整 diff；完整结构化 diff 留在 UI 数据里，native 路径没有再次把整个 structured_content 拼进模型正文。[回显实现](../../src-tauri/src/mcp/registry.rs:1157)、[native 与 MCP 输出分支](../../src-tauri/src/chat/agent/execute.rs:887)

进一步按实际请求分组：仅调用 write/edit、且中间没有额外 user/image 消息的轮次，Kivio 37 轮的下一次输入净增加合计 **29,397 tokens**；OpenCode 49 轮为 **1,263 tokens**。差额 **28,134 tokens** 支持写入回显是重要来源，但两边修改内容不同，不能承诺删 diff 后必然节省同样数值。

建议：成功结果返回路径、成功状态、行数与必要警告/诊断；完整 diff 继续供 UI 审阅。失败与部分写入信息必须保留。不要通过降低写入可靠性或省略错误来节省 tokens。

### P1：无关目录查询制造高 token 密度噪声，本次实际发生

Kivio 第 91 次模型响应（0 起索引 90）之后，仅有一次 bash 和一次 tool result。该命令为检查临时目录清理情况，列出整个 Temp 目录再过滤 `kivio`。原输出 6,345 行、382,602 bytes，完整日志外置，模型仍收到 18,106 字符，其中大量无关测试目录和 UUID。

前一请求 total 为 **256,740**，下一请求 input 为 **266,958**，净增 **10,218 tokens**。这是整个单工具间隔的实报净增（包含消息封装），不是字符估算；按字符统一除以 4 会显著低估这种 UUID 列表。来源为上述 Kivio 会话该 tool result 和对应 usage 记录。

当前配置 maxToolOutputChars 为 24,000；全局截断已经存在，并保留头尾，因此不是“没有截断”。问题是查询范围失控及信息密度低，字符上限又不能精准代表 tokens。[全局限制](../../src-tauri/src/chat/agent/execute.rs:1021)、[默认及规范化](../../src-tauri/src/settings.rs:1251)

建议：清理验证只检查本次创建的明确路径；目录搜索限定根目录和数量。大日志继续外置，回显退出码、关键行及全文定位。可按工具类型设更合适的预算，不宜一刀切 2K。

### P1：实报锚点缺少完整的适用范围检查，属源码发现，未证明影响本次100K

当前 resolve_usage_anchor 检查清空边界、provider 与压缩时间，但没有比较锚点模型或工具/系统提示版本。单模型 assistant 的 provider/model 在 group_meta=None 时均不保存，解析锚点又会回退到会话当前 provider；修改会话 provider/model 的路径未在该处分离旧锚点。切换提供方后，旧 usage 甚至有机会被按新 API 格式解释。[锚点解析](../../src-tauri/src/chat/commands/context.rs:627)、[单模型消息存储](../../src-tauri/src/chat/commands/messages.rs:162)、[修改元数据](../../src-tauri/src/chat/repository.rs:878)、[会话设置变更](../../src-tauri/src/chat/commands/mutations.rs:1035)

工具集/系统提示修改后，当前算法仍可能使用旧请求 total，只加消息尾部估算；新增 schema 和指令不一定属于尾部消息。这意味着“优先实报”还需要“实报对应哪份请求”的条件。该缺口不应通过恢复 max(全文估算, 实报) 掩盖。

建议后续实现并测试：保存锚点 provider、model、API format 和请求配置版本；配置变更后保留“上次实报”作为历史参考，当前预测用量需重新计算或明确标示差异。检查同提供方换模型、跨提供方切换、启停工具、切换助手/系统提示的案例。此轮仅静态链路审查，尚未用运行中应用复现。

### P2：压缩预算没有显式预留实际输出额度，属接近上限时的边界风险

自动压缩判断使用固定 `window * 0.90`，比较前未扣除本次请求 max_output_tokens。输出上限另由模型元数据选择。[压缩判断](../../src-tauri/src/chat/agent/compaction.rs:1395)、[输出额度](../../src-tauri/src/chat/model_metadata.rs:828)

剩余10%不保证容纳所有模型的允许输出。建议按实际协议的 input/context/输出限制计算预算并做边界测试；不能据此认定本次269K过大或将170K定为通用压缩点。主动重写历史也可能破坏此前高度命中的缓存前缀。

### P1（评测）：端口与会话干预没有完全隔离，影响效率归因

Kivio 本次工具日志实际出现 `127.0.0.1:8765` 同时有三个 LISTENING PID；同一诊断读取自己的 state.db 时 events、requests 均为0，后续还检查8791等端口。当前没有历史PID归属证明，不能断言一定连到了某个参赛者；但不能把相关排查时间全部归因于模型或框架效率。

此外，DSH 原生记录是3个 turn/start，Kivio与OpenCode各为1条原始用户任务；DSH并不是完全相同的无干预轨迹。模型别名和 API 格式也需记录，当前官方映射只证明当前关系，不能反推测试时后端快照。

建议下一次测评：每个Agent分配独立端口、数据库和临时路径，固定API模型版本/参数，记录所有人工干预；做多次重复，分别报告质量、模型输出、工具输入增长、耗时与费用。

### 执行体量差异仍需保留，不能全部算框架缺陷

Kivio 本次 store.py 有两次整文件 write，参数分别22,862和21,135字符；最大 app.js write 参数55,916字符。OpenCode也用old/new编辑接口，不能据此声称改成某一种工具就能消除35,301个非推理输出差额。需要判断代码规模、必要修改、重复实现和验收返工；本报告不重新评价成果功能得分。

## 处理顺序

1. 优先精简成功写入/编辑回显，并让临时目录与命令查询范围更精确；这两项已有本次实测证据。
2. 修补实报锚点适用范围；这是统计正确性独立问题，需要针对变更配置的回归测试。
3. 隔离评测端口/数据库并记录干预，再比较Agent行为与代码体量。
4. 单独验证输出预留和提供方能力适配；不直接删除协议要求的推理，也不为追平169K而强制提前压缩。

本轮按 Research 技能完成官方主源与本地记录交叉核实，仅新增调查报告；未实现上述新改动，未提交或推送本报告。

## 补充：整文件重写与大段修改的复原检查

针对用户强调的“整文件重写、大段修改和返工”，逐项复核44次文件操作。14次write中12次是新文件创建；2次覆盖分别是把149字节任务入口占位文件app.py实现为真实程序，以及对刚创建的store.py再次整份写入。其余30次使用edit。不能把首次创建大文件全部算成不必要的重写。

store.py的第二次write参数21,135字符；工具报告新增96行、删除111行，属于真实整文件覆盖候选。是否应该全部改成局部edit，需要结合修改跨度和生成成本判断；本次并非普遍反复整文件重写。

### 新确认的P1：diff退化把局部修改夸大成上千行变动

从原始model_messages取第一次app.js的write正文，在内存中按随后第一次edit的六组old_string/new_string依次精确替换。每组旧串均唯一命中，无模糊匹配；比较复原前后完整文本的逐行LCS，得到：

| 指标 | 结果 |
|---|---:|
| 修改前行数 | 1,303 |
| 修改后行数 | 1,313 |
| 不变行数（完整LCS） | 1,283 |
| 实际最小行差异 | +30 / -20 |
| Kivio返回的统计 | +1,088 / -1,078 |

这六组局部修改跨越文件多个位置；去掉公共前后缀后，中间区间为1,078×1,088=1,172,864个DP单元，超过代码的250,000上限。`build_diff_ops`因此把整个中间区间全部标为Remove/Add，导致统计夸大和回显包含未改代码。[阈值](../../src-tauri/src/native_tools/files.rs:837)、[粗粒度回退](../../src-tauri/src/native_tools/files.rs:975)

这一操作返回3,085字符，并带diff clipped标记。该算法只影响diff及增删统计，不意味着文件执行时真的错误修改了全部1,000多行。审阅者却可能把它误判成大规模重写；回显再截前80行也不保证包含真正的修改点。这让单纯“保留80行diff便于模型自检”的收益存疑。

建议分开处理：使用能够保留局部匹配的diff算法/分块策略，避免把DP复杂度回退冒充精确改动统计；成功回显继续精简，审阅diff保留在UI；对已有文件的整份覆盖采取局部编辑优先、明确大规模重构例外的策略。此轮只复原和验证，未修改产品实现。
