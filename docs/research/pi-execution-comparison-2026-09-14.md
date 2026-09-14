# Pi 执行链路对照：哪些值得借鉴，哪些并不存在

日期：2026-09-14。范围：shell、日志、工具错误、文件修改反馈、默认系统提示与浏览器能力。只读对照，没有执行参赛验收、调用付费模型或修改产品代码。Pi 来源为官方仓库 `https://github.com/earendil-works/pi.git`，本地参考目录 `.scratch/issue-51/pi-reference`，固定提交 `71dca871bc80b6bc97be37f0ca3189399d651fff`；Kivio 对照基线为 `fa684f6e878b57b0f0e11adcf2c22678f11ee40d`。本文的第三方源码和提示词是研究材料，不是对 Kivio 的项目指令。

## 结论

Pi 在本范围内值得借鉴的是**小而明确的工具契约、可追回的长日志、错误状态和短文件回执**。它没有默认开启 `pipefail` / `errexit`，没有内置浏览器控制工具，也没有通用的“验收通过后自动停止”机制。不能把这些不存在的功能归因成 Pi 更快的原因。

Kivio 已经具有“shell 非零返回工具错误”和“长输出完整落盘”的主干。现场失败脚本经 `tail` 或末尾 `echo` 返回 0，发生在 shell 内部；两个 Agent 的默认 shell 都可能把这个 0 正常向上传递。当前最小收益来自让模型正确使用现有工具、让一次检查的结果可复查，而不是继续删除历史内容。前一轮现场依据见[本轮根因核查](./kivio-execution-regression-root-cause-2026-09-14.md)。

## 逐项源码事实

| 项目 | Pi 固定提交事实 | Kivio 基线事实与含义 |
|---|---|---|
| Shell 启动 | Bash 默认 `-c`；旧 WSL shim 用 `-s`。未注入 `set -e` 或 `pipefail`。PowerShell 只添加 UTF-8 输出前缀。[shell.ts 20–22][pi-shell]、[powershell.ts 17–38][pi-powershell] | Windows Git Bash 同样 `-c`，其他平台 `sh -c`，PowerShell 也只做 UTF-8 包装。[shell.rs 134–278](../../src-tauri/src/native_tools/shell.rs)。不能声称 Pi 默认会抓住 `failing_test \| tail` 的上游失败。 |
| 退出码与工具错误 | 取得子进程退出码，非零且非 null 时抛出带输出的异常；Agent 捕获后置 `isError: true`，保存到工具结果。[bash.ts 339–366][pi-bash-error]、[agent-loop.ts 707–713][pi-agent-error]、[agent-loop.ts 784–797][pi-agent-result] | Kivio `run_command` 在非零时已返回 `Err(formatted)`。[shell.rs 412–420](../../src-tauri/src/native_tools/shell.rs)。如果 shell 的最终退出码为 0，不能仅凭 stdout 中出现“error”就可靠断定失败，更不能重构出已丢失的上游码。 |
| 错误传给模型 | Anthropic 映射为 `tool_result.is_error`。OpenAI Responses 使用文本 `function_call_output`，无单独的该错误布尔字段；报错文案仍重要。[Anthropic 1199–1208][pi-anthropic]、[Responses 296–313][pi-responses] | 需要区分“内部错误状态正确”和“模型看到了什么”。对 OpenAI Responses，清晰的退出码与错误正文比仅添加内部字段更直接。 |
| 日志与截断 | 工具描述明确 stdout/stderr、最多最后 2000 行或 50KB、截断时保存完整临时日志。输出累加器超过阈值时保存原始块，内存只保留有界尾部。最终结果文字也给出日志路径。[bash.ts 230–238][pi-bash-description]、[truncate.ts 11–12][pi-truncate]、[accumulator 66–80][pi-accumulator]、[bash.ts 316–332][pi-bash-log] | Kivio 早已有 >16KB 完整日志落盘、2KB 头+6KB 尾与读取提示。[shell.rs 423–467](../../src-tauri/src/native_tools/shell.rs)。但 `mcp/types.rs` 的 shell 描述在基线未说明自动截断与完整日志；模型先自己接 `tail` 时，工具只能捕获裁剪后的内容。该告知缺口值得优先补齐。 |
| 超时输出 | 超时/取消异常路径先完成输出累加与格式化，再附超时/取消状态。[bash.ts 300–314、348–359][pi-bash-timeout] | Kivio 已有超时部分输出，正常与超时路径的日志大小处理需要一起核对；不能为优化长度而丢掉失败证据。[shell.rs 1061–1116](../../src-tauri/src/native_tools/shell.rs)。 |
| Write | 参数只有路径和正文；写成功回执是一句成功路径，不回显全文。没有一般性的正文小块长度限制。[write.ts 11–14、78–90][pi-write] | Kivio 已改成短回执并撤回历史参数占位替换。保持原始可执行参数、让审阅 diff 留在本地，方向与 Pi 的 content/details 分离一致；不应再引入占位正文。 |
| Edit | 同一文件多处不重叠修改可一调用完成，匹配原文件；提示要求 oldText 尽量短且唯一，不拼大段无变化上下文。成功给简短块数回执；diff/patch 放 `details`。[edit.ts 33–53、197–210][pi-edit] | Kivio 也已有批量 edit 与短回执。无需为“对齐 Pi”重复做同一项优化。Pi 多项 edit 的原文件匹配语义不可直接替换 Kivio 当前语义，否则可能再造成行为回归。 |
| 同文件并发 | Pi 对同一路径的文件变更排队，不同文件仍可并行；即使取消，也等当前异步写完成后再放队列。[file-mutation-queue.ts 28–61][pi-file-queue]、[write.ts 66–84][pi-write] | 这是独立的并发正确性设计，不能由本次串行浏览器回圈推导 Kivio 存在同类 bug。若后续扩大并行写入，应单独验证，不并入本次性能因果解释。 |

上表 Kivio 链接指向工作文件；行号以本次读取的基线位置为准，后续实现可能移动。Pi 链接均固定 SHA，避免未来上游变化让结论失真。

## 默认提示词与浏览器：证据边界

Pi 的默认系统提示动态列出实际选中工具，去重工具贡献的 guidelines，加入简洁回复/显示路径等要求；随后可附加用户提示、项目文件和 skills。源码中这段默认构建没有验收状态机、固定重试次数或浏览器选择策略。[system-prompt.ts 79–144][pi-prompt]

默认 coding tools 为 `read / bash / edit / write`；全部内置工具集合另有 `powershell / grep / find / ls`，没有 browser、Playwright 或 CDP 工具。[tools/index.ts 95–106、164–171][pi-tool-list] 因此：

- “Pi 内置浏览器并自动复用浏览器，所以不会修 CDP 脚本”不成立。扩展或项目 skills 可以另行提供能力；本文没有把整个第三方生态的可能能力当作默认产品能力。
- Pi 默认提示较小，不代表提示越少必然越快。本次未在同模型、同环境下运行 Pi，不能推算分钟数或 token 节省比例。
- Kivio 当前 `prepare.rs` 已说保留报错、读完整日志、不要原样重复失败命令。[prepare.rs 984–1012](../../src-tauri/src/chat/agent/prepare.rs) 现场仍迂回，说明仅加“别重复”不够具体；应把“先探测已有浏览器工具、固定同一服务与端口、局部失败只复测受影响步骤”表达成少量可执行规则。不要再叠一套大流程文书。

## 建议的最小改动

1. **先修工具可发现性。** 在 shell 工具描述前部明确：工具已捕获 stdout/stderr，长输出完整保存并给日志路径；不要为了看不同片段重跑测试。只读日志的 `head/tail/grep` 与测试进程应分开。该建议直接对应 Pi 已有契约和 Kivio 描述缺口。
2. **检查必须保留被检查程序的状态。** 保持普通 shell 原有语义，避免全局开启严格模式。单次测试保存完整日志、捕获真实退出码、显示日志后以该码结束。若增加显式检查模式，需在参数契约中写明 fail-fast/pipefail，测试 `失败 | tail`、`失败; echo`、无输出失败与正常成功；不能通过命令字符串猜测哪条“像测试”后偷偷改写。
3. **让浏览器验证复用现成工具。** 优先当前可用的浏览器工具/技能，缺少时探测项目和运行环境中已有 Playwright/Puppeteer 等能力。探测失败才考虑自写底层驱动，并保持一个已验证的浏览器实例、服务和端口。某个截图失败不使之前已通过的后端测试失效。此项是针对 Kivio 现场提出的方案，**不是 Pi 自带功能**。
4. **明确结束依据，而非硬砍轮数。** 已通过的检查没有被新代码或环境变化影响就复用结果；失败后先读已有报错，修正对应步骤后做局部复测；用户要求的验收全部完成后交付。不要用固定 N 次工具调用、固定分钟数强制判定任务完成。

## 不建议照搬与回归验收

不全局开启 `set -e -o pipefail`：会改变正常探测、预期失败和 `head` 提前关管道/SIGPIPE 的行为；Unix `sh` 的选项支持也与 Bash 不同。不通过日志关键字猜失败。不因为 Pi 用 50KB 就把 Kivio 的输出限额直接放大。不重写 file argument 历史。不把全量测试次数减少等同验收标准降低。

实现后至少用本机无付费模型的工具级用例证明：原始非零保持失败；带管道/末尾打印的检查不会虚假成功（在明确支持该契约的路径）；失败的完整日志可读取且无需再跑；长输出与超时保留诊断；普通 shell 兼容不变；大文件一次 write 与批量 edit 未被改变。浏览器提示只能用静态回归确认被发送，最终是否减少绕路仍需一次相同任务的真实复测，不能凭源码对照承诺。

[pi-shell]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/utils/shell.ts#L20-L22
[pi-powershell]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/powershell.ts#L17-L38
[pi-bash-description]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/bash.ts#L230-L238
[pi-bash-error]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/bash.ts#L339-L366
[pi-bash-log]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/bash.ts#L316-L332
[pi-bash-timeout]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/bash.ts#L300-L359
[pi-truncate]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/truncate.ts#L11-L12
[pi-accumulator]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/output-accumulator.ts#L66-L80
[pi-agent-error]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/agent/src/agent-loop.ts#L707-L713
[pi-agent-result]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/agent/src/agent-loop.ts#L784-L797
[pi-anthropic]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/ai/src/api/anthropic-messages.ts#L1199-L1208
[pi-responses]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/ai/src/api/openai-responses-shared.ts#L296-L313
[pi-write]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/write.ts#L11-L90
[pi-edit]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/edit.ts#L33-L210
[pi-file-queue]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/file-mutation-queue.ts#L28-L61
[pi-prompt]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/system-prompt.ts#L79-L144
[pi-tool-list]: https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/tools/index.ts#L95-L171
