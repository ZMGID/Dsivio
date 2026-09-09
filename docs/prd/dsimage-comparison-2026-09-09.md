# 图片工作台与 dsimage 对照

核对日期：2026-09-09。上游：[ZMGID/dsimage](https://github.com/ZMGID/dsimage)，当前提交 `bc83321d19cf51f694b1aa06efbc4cdcc7effd34`，与工作台内置模板记录的来源版本一致。本次问题来自原生页面执行逻辑，而非模板资源落后。

## 本次已补齐

| 项目 | dsimage 上游 | 工作台本次修改 |
| --- | --- | --- |
| 并发提交 | `gen --n` 和批次统一进入任务池，默认 9 路 | 快速出图、样品、批量、单页重试共用最多 9 路的调度器 |
| 完成顺序 | 哪张先完成就先保存 | 先完成先保存，卡片保持方案顺序；慢图不阻塞其余图片 |
| 单张失败 | 按槽位记录，其他工作继续 | 单页错误保留，其他图片继续，最后汇总失败数量 |
| 进度 | 逐槽位完成日志、批次结果 | 展示本轮完成数、进行中数量、失败数量，同时显示多张生成中卡片 |

来源：[任务池实现](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/scripts/gen_image.py)、[gen 命令](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/scripts/dsimage.py)。

工作台还保留持久化与停止语义：异步编号先保存再查询，远程查询占用同一个并发名额；停止后不再取排队页，已提交的同步图片继续收尾。任务数据只由一个协调器写入。磁盘写入失败时停止追加请求，同时收集已经提交的结果。

## 已有功能及差异

| 能力 | 工作台现状 |
| --- | --- |
| replace 样图换货 | 模板提示词直出，支持 `prompt_by_kind`、`refs_by_kind`、`{sku}`、轮换 `{vary}`；可指定正反面 |
| smart 模板套图 | Agent 按商品和每页 brief 写提示词；传入模板风格、文字策略、款型分支和 26 类拍法资料 |
| 样品与分类放行 | 每类前两款、全部页面成功后由用户确认；改单页撤回所属分类的确认 |
| 单页修改和版本 | 保存提示词及参考图；选定旧图作为第一张编辑参考；保留历史需求版本 |
| 模板共享 | 页面和内置 Skill 共用 `template.json` 与素材目录；支持导入、导出、复制、冻结成图 |
| 客户共用要求 | 模板扫描已经合并 `要求.json` 中的语言、风格、品牌及生成输出默认值；原说明里的“尚未合并”已过时 |
| 交付导出 | 可设置画布与文件大小、保留原图、输出清单；参数目前由导出面板设置 |

对应源码：`src-tauri/src/image_studio/agent.rs`、`types.rs`、`storage.rs`、`mod.rs` 和 `src/chat/images/`。

## 仍未与上游对齐

| 项目 | 上游细节 | 工作台差距 |
| --- | --- | --- |
| 降并发与重试 | 429、超时、部分 5xx 后按 9→4→2→1 回退，间隔 15 秒 | 本次实现固定上限 9；失败仍手动重试。连接中断或已有远程编号时，不自动再次提交图片订单 |
| 派生背面 | 单独生成派生背面、先检查，再进入套图 | 尚无派生与确认步骤，需要导入真实背面 |
| 从零设计模板 | 先形成可复用 smart 模板，再按商品写提示词 | 先生成方案，可在之后提取为规则模板；步骤不同 |
| 样品选择 | 可选择有代表性的两个 SKU | 当前固定分类前两款，尚无独立样品选择器 |
| 模板修改后批量重出 | 可按 SKU 和槽位组合重出 | 当前主要是单页修改、单页重试；任务使用模板快照，修改模板不会自动更新现有任务 |
| 成套预览 | 提供整套拼图便于对照 | 当前提供结果卡片和逐张查看，尚无模板与成图的成套对照图 |
| 交付规则与白底原图 | 模板 `output.deliver` 驱动导出，商品白图随成果整理 | 面板默认 800×800、2048 KB；未完整自动带入各模板的交付规则，也未附带商品白底原图 |
| 批次状态互通 | `_dsimage/batch.json`、SKU jobs、状态与续跑 | 页面有独立 `tasks/*.json`；共享模板和设置，不等于共享完整批次续跑状态 |

来源：[replace 流程](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/replace.md)、[smart 流程](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/smart.md)、[design 流程](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/design.md)、[客户大单流程](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/client.md)。

后续优先补限流退避与失败页批量恢复，再接模板交付规则和成套对照预览。重试需要区分明确拒绝、结果未知和已保存远程编号，避免把“恢复查询”变成重新生成。

## 验证范围

并发测试使用可控的模拟供应商，覆盖真实异步等待重叠、9 路上限、乱序完成、单页失败隔离、停止后排队页不提交、远程编号先保存、磁盘错误时收尾。未调用真实供应商付费生图，无法据此保证具体供应商支持 9 路同时请求。
