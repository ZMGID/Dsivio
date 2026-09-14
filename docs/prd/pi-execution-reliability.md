# Pi 对照后的执行可靠性改进

本次范围来自用户要求：拉取 Pi agent 源码对照，并修复 Kivio 问题1复测中暴露的执行/验证问题。参考仓库 `earendil-works/pi` 固定于 `71dca871bc80b6bc97be37f0ca3189399d651fff`，详细差异见研究记录。

## 要达到的行为

1. 普通 bash 调用保持现有 shell 语义、后台作业、超时和权限边界。不得全局启用 errexit/pipefail 使普通 `grep`、`head` 或预期失败探测产生新回归。
2. 在已有 bash 工具提供显式 `check: true`，用于有限的测试/构建检查。该模式使用 Bash 的 errexit 与 pipefail：失败管道返回非零，未处理的失败后不执行尾部成功 echo；显式 `if` 等预期失败处理遵循 Bash 自身语义。它不解析或猜测命令是否为测试，也不承诺阻止脚本主动屏蔽错误。
3. 检查模式必须以前台执行，拒绝后台配置/自动识别的常驻服务。使用本机 Bash；Windows 无 Git Bash 时明确报不支持，不擅自将 Bash 选项套入 PowerShell。普通 PowerShell 回退不变。
4. 检查模式每次保存捕获的完整 stdout/stderr、退出码，超时保存已捕获内容，并返回可定位日志路径；短输出也保留日志，长输出继续遵守现有内联上限。日志保存失败必须告知，不能谎称已保存。shell 命令内部自行丢掉的输出无法恢复。
5. 工具描述显著告知输出上限、完整日志、检查模式；指引测试直接执行、不先接 tail/head/grep，检查退出状态并读取同一次日志，不为看不同片段重复跑。
6. 精炼验证指引：先探测已有浏览器工具/项目验证脚本，复用后再考虑自行实现驱动；区分测试驱动错误与应用错误，先复现最小失败场景；已通过检查在相关代码/配置/环境未变化时复用，所有交付要求满足后结束。
7. 保留真实历史参数、短文件回执、完整审阅 diff 与实报计数。Pi 的默认 shell 同样未启用 pipefail；显式检查模式是针对 Kivio 已复现故障的补充，不能宣称来自 Pi 的现成功能。

## 验证

通过真实工具入口测试失败管道、失败后 echo、普通 head 管道、显式预期失败、长日志中段恢复、短成功日志、拒绝后台检查、超时日志。复用既有取消/进程树、shell 引号、上下文实报、文件完整性测试。类型与完整测试集在实现结束后验证；报告既有平台失败，禁止声明已验证付费整题的耗时或完成率。

## 实现验证记录（2026-09-14）

- 工具入口回归先红后绿；最终 `cargo test --manifest-path src-tauri/Cargo.toml --lib check_mode_`：3 通过，涵盖上述情形及 `background:false` 不得绕过已识别的常驻服务拒绝。
- 前端完整测试：207 文件、1607 用例全部通过；`npx tsc --noEmit` 通过。
- Rust 完整测试：2405 通过、3 失败、47 忽略。两项已知 Windows 平台失败为 `path_env::tests::common_dirs_macos_expands_home` 和 `path_env::tests::merge_unix_from_minimal_path_adds_common_dirs`。另一个未改动的 `chat::hooks::tests::http_non_2xx_is_reported` 全量运行收到 502 而非夹具预期的 500；同一测试二进制单独执行及 Cargo 定向复核均通过。该偶发失败原因尚未确定，不将复跑成功写成全量通过。
- Standards 复审无发现；Spec 复审发现的常驻服务绕过已修复并复核，剩余发现为 0。
- 开发应用已重新编译启动。未调用付费模型运行整题，尚不能承诺耗时、轮次或上下文节省幅度。

本地原始日志位于 `.scratch/issue-51/pi-{check-red,check-green,check-final,full-rust,full-frontend,tsc,http-recheck,http-direct}.log`，不纳入提交。
