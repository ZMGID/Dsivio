# dsimage 图片工作台代码审查

审查日期：2026-09-09。当前项目基准：`7f52004b`（制作、试品、反馈与持续生成）。上游基准：`bc83321d19cf51f694b1aa06efbc4cdcc7effd34`，已通过远端 HEAD 和该提交的指南核对。

按用户要求，本报告采用代码审查与模块测试；不以浏览器预览判断桌面功能，不评价未经实测的视觉效果或实际出图质量。以下问题基于实现前的代码。

**后续用户纠正（优先于下文原始建议）**：用户否定了把入口合成三个、把功能藏进折叠区和新增快捷按钮的做法。这轮界面重排已经撤回，保留原有六个功能入口和模板库，包含最新的「制作与试品」；后续精简应从原页面的重复内容和无效操作入手，不能把减少入口数量当作目标。下文有关合并入口、增加起点/按钮、替换快速出图表单的建议均未采纳，不作为实施计划。

本次实际保留：按路径区分导入商品、样图自然数字排序、任务改名不失效，以及[保存目录规范](image-studio-folder-convention.md)。模板复用与冻结等审查发现仍是待评估项，不声称已经补齐。

**结论**

已经做成应用内独立的「图片工作台」，路由为 `#chat/images`，有独立 React 页面和 Rust 图片模块，使用应用模型配置。内置 dsimage Skill 仍保留聊天入口，与页面共享模板和配置；脚本批次和页面任务各有自己的存储格式。

核心能力覆盖较多，但入口和工作流尚未收拢。最新「制作与试品」应成为套图主流程；继续与旧的样图换货、模板套图、从零设计、多品类批量并列，会让用户承担原本应该由软件处理的分流工作。

建议保留三个入口：**制作与试品、快速出图、模板库**。最近任务与图片设置继续作为辅助入口。先补齐新流程的模板复用和批量分组，再撤下重复导航；旧任务仍应能打开。

**当前入口与 dsimage 对照**

| 当前入口 | 对应上游能力 | 代码现状 | 精简建议 |
|---|---|---|---|
| 快速出图 | `gen` 单张/几张、带参考图改图 | 已有无参考图生成、参考图、修改版本、导出；仍强制先规划，再点击生成 | 保留；默认一次点击生成，把完整提示词编辑放入可展开的高级选项 |
| 制作与试品 | 新增编排，串联建模板、smart/replace、试品、修正、批量 | 已有真实后端动作、版本、确认门槛和增量生成 | 作为套图主流程，优先完善 |
| 样图换货 | `replace` | 已有模板 prompt、示例图、真实正背面、款型分支和样品确认 | 合入主流程的「使用模板 / 成套样图起步」 |
| 模板套图 | `smart` | 已有逐商品逐页规划、模板规则与试品 | 合入主流程的「使用模板」 |
| 从零设计 | `design` | 当前从商品需求直接规划，另有手动存模板；与新流程商品图起步重叠 | 合入「商品图起步」 |
| 多品类批量 | `client` 大单分类流程 | 已有识别分类、按分类选模板与确认；新流程目前是一任务一套规则 | 撤下独立一级入口前，保留批量导入、分类及每类模板能力 |
| 模板库 | list/import/export/create/freeze | 已有共享库、导入导出、编辑、预览；复用仍跳旧流程 | 保留；新建模板转到主流程，低频手动编辑收进更多操作 |

入口事实见 [types.ts:129](<E:/ZM database/Dsivio/src/chat/images/types.ts:129>)。独立路由见 [chatRoutes.ts](<E:/ZM database/Dsivio/src/chat/chatRoutes.ts>)，原生实现见 [image_studio/mod.rs](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs>)。共享边界见 [studio.md](<E:/ZM database/Dsivio/src-tauri/resources/skills/dsimage/guides/studio.md>)。

**应优先处理的功能问题**

1. **批量导入存在混品风险。** `image_studio_import` 用文件夹名称作为 `BTreeMap` 的分组 key，递归时又换成当前子文件夹名。一次导入 `客户A/001` 与 `客户B/001` 会落到同一组；每个 SKU 下都有「正面/细节」子文件夹时，也可能按素材类别跨商品合并。混品后图片会一起交给生成模型。应按所选商品目录的规范路径确定唯一商品身份，商品目录下的图片子目录只用于收集素材；保留「每个子夹一个商品」与「当前夹一个商品」的明确语义。证据：[mod.rs:250](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs:250>)、[mod.rs:266](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs:266>)、[mod.rs:285](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs:285>)。

2. **旧流程仅改任务名称、追加商品也会使已有方案和确认失效。** 非 workflow 任务只要整个 brief 有任何差异，就递增 revision、清空方案和 approvedGroups；结果展示和导出又只认当前 revision。因此改名后旧图仍在历史中，但不再作为当前成图导出。新流程已经对规则输入、试品和新增商品分别处理，应复用这套失效规则，或完成旧任务迁移。证据：[mod.rs:187](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs:187>)、[types.ts:211](<E:/ZM database/Dsivio/src/chat/images/types.ts:211>)、[mod.rs:981](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs:981>)。

3. **模板库无法直接接入制作与试品。** 点「使用模板」固定进入旧 `replace` / `smart`。新流程只有商品图、成套样图两种起点，没有选择已有模板的入口。因而从模板库开始的新任务无法直接进入新流程的共用规则反馈与持续生成。应增加「使用已有模板」起点，复制模板快照进入任务；在试品确认前不直接覆盖库中已确认模板。证据：[ImageStudio.tsx:548](<E:/ZM database/Dsivio/src/chat/images/ImageStudio.tsx:548>)、[ImageWorkflow.tsx:224](<E:/ZM database/Dsivio/src/chat/images/ImageWorkflow.tsx:224>)、[workflow.rs:327](<E:/ZM database/Dsivio/src-tauri/src/image_studio/workflow.rs:327>)。

4. **新流程的成图不能从页面冻结为样图模板。** 后端已有 `image_studio_freeze`，但前端用 `brief.feature !== 'workflow'` 隐藏了按钮。对从零设计成功的套图，「以后固定这套版式换品」是有用的收口动作；保存 smart 文字规则与保存成品样图不是同一件事。应在整套成功后提供「将这套效果保存为模板」，由系统处理模板类型。证据：[ImageStudio.tsx:1479](<E:/ZM database/Dsivio/src/chat/images/ImageStudio.tsx:1479>)、[mod.rs:919](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs:919>)。

5. **成套样图会被普通文件名排序打乱。** 导入器统一按 `a.name.cmp(&b.name)` 排序，`h1.png、h10.png、h2.png` 将以这个顺序返回；新流程把该顺序直接作为逐页样图顺序，用户需要手动前移/后移修正。应使用自然数字排序，保留可调整顺序的入口，并在制作前展示页序。证据：[mod.rs:313](<E:/ZM database/Dsivio/src-tauri/src/image_studio/mod.rs:313>)、[ImageWorkflow.tsx:153](<E:/ZM database/Dsivio/src/chat/images/ImageWorkflow.tsx:153>)、[workflow.rs:395](<E:/ZM database/Dsivio/src-tauri/src/image_studio/workflow.rs:395>)。

**制作与试品：逐步检查**

| 步骤 | 已有实现与判断 | 需要简化或补齐 |
|---|---|---|
| 1. 给图与要求 | 商品图/样图分流、页数和规格、样图顺序都有；功能成立 | 「开始制作」实际上只生成文字规则，不会出第一套图。商品图起步可以提供「用原商品试做」，避免只有一个商品时再次导入同样图片。保留原素材与试品的内部区分 |
| 2. 生成试品 | 可选择 1–2 个商品；真实生成调度、进度、失败记录已经接入 | 多张素材的正面目前按文件名或第一张推定，应在小卡片中明确显示所选正面；新流程未接入现有拖放导入能力 |
| 3. 反馈修正 | 共用规则反馈、规则版本、自动重新试做、单图修改都有；核心闭环成立 | 「反馈这页规则」目前仅把页号追加到反馈框，后端修改后会使整批规则失效，并重新生成所选试品的全部页面。应至少明确会重出多少张，再考虑只重出受影响页面 |
| 4. 确认并持续生成 | 只有当前版全部试品页成功才可确认；新增商品不会自动重做已完成商品 | 确认版本不会自动变成 replace 样图模板；主流程目前不支持每个分类各自规则/确认 |
| 5. 导出与后续复用 | 已有本版导出、历史图、任务恢复、模板共享 | 输出默认仍是 800×800、2048KB；模板交付字段没有完整映射。模板复用应回到同一流程 |

新流程的正确性基础值得保留：修改试品会撤回确认；旧规则图片、最新失败版本不能冒充有效试品；远程任务 ID 会保留并恢复查询；单图修改与共用规则修改有区分；追加商品复用未变化商品的方案与结果。不要为了减少界面控件删掉这些后端约束。

相关实现：[workflow.rs:73](<E:/ZM database/Dsivio/src-tauri/src/image_studio/workflow.rs:73>)、[workflow.rs:132](<E:/ZM database/Dsivio/src-tauri/src/image_studio/workflow.rs:132>)、[workflow.rs:304](<E:/ZM database/Dsivio/src-tauri/src/image_studio/workflow.rs:304>)、[workflow.rs:511](<E:/ZM database/Dsivio/src-tauri/src/image_studio/workflow.rs:511>)、[workflow.rs:585](<E:/ZM database/Dsivio/src-tauri/src/image_studio/workflow.rs:585>)。拖放入口明确排除 workflow，见 [ImageStudio.tsx:313](<E:/ZM database/Dsivio/src/chat/images/ImageStudio.tsx:313>)。

**具体删减清单**

| 内容 | 建议 | 保留方式 |
|---|---|---|
| 样图换货、模板套图、从零设计三个一级入口 | 合并后撤下 | 变成制作起点：已有模板 / 商品图 / 成套样图 |
| 多品类批量一级入口 | 分类能力迁入后撤下 | 多款导入时按需要展开分类、各类模板与确认 |
| 快速出图里的强制「画面方案」阶段 | 从默认路径移除 | 一次点击完成内部规划和生成；高级用户仍可查看、编辑 |
| 「图片要求」和「统一风格」同时常驻 | 合并常用输入 | 一个需求框；模板默认风格与细调放高级项 |
| 快速出图里的平台、商品分类、款型、任务名称等常驻表单 | 按任务相关性收起 | 商品套图需要时才显示；名称自动生成后可改名 |
| 提示词优化旁的助手选择 | 从默认界面收起 | 优化使用默认配置，助手切换作为高级选项 |
| 模板库的「创建规则模板」独立手工流程 | 降级 | 主按钮进入制作与试品；手工模板编辑放更多操作 |
| 模板编辑器的页面 ID | 从普通编辑移除 | 系统生成；用户只改页面用途、顺序与内容 |
| 大段解释 Agent、规则、槽位、提示词的常驻说明 | 精简 | 页面展示「这一步做什么、下一步点哪里」；技术信息按需展开 |
| 保存任务按钮作为独立必经步骤 | 逐步弱化 | 沿用已存在的操作前自动保存和本地草稿；增加清楚的保存状态 |
| 历史图片、远程恢复、失败重试、原图保留 | 保留 | 结果区按需出现，不在空白首页堆按钮 |

`快速出图` 当前仍有「生成画面方案 → 画面方案 → 开始出图」的两次提交，见 [ImageStudio.tsx:1055](<E:/ZM database/Dsivio/src/chat/images/ImageStudio.tsx:1055>)、[ImageStudio.tsx:1111](<E:/ZM database/Dsivio/src/chat/images/ImageStudio.tsx:1111>)。完整 prompt 编辑直接显示，见 [ImageStudio.tsx:1178](<E:/ZM database/Dsivio/src/chat/images/ImageStudio.tsx:1178>)。

**与上游尚未完全对应的边界**

- 自动派生背面：上游存在 derive；页面要求真实背面，缺图时报错。这是明确的行为差异，保留真实商品优先的限制合理，但应在选模板时提前指出缺什么素材。
- 多品拆分：上游支持同夹按 SKU 文件名拆品；页面目前按目录分组，不能视作完整等价。
- 模板参数：页面保留 JSON 和部分款型分支，但生成模型采用全局图片设置，并发采用本地调度；`output.deliver` 等字段没有完整驱动页面导出。界面不必逐项复刻 CLI，但支持范围应明确。
- 单张默认：上游 gen 默认无文字；当前所有入口的默认语言统一为 pt-BR。巴西模板可保留这一默认，自由创作更适合默认无文字或记住用户最近选择。
- 任务互通：模板、模型配置共享已经存在；`_dsimage/batch.json` 与页面 `tasks/*.json` 不双向同步。从聊天完成的脚本批次不会自然变成页面里可继续编辑的同一个任务。
- 模板发布：新流程 build/refine 在试品确认前已经保存/更新共享模板，没有独立草稿或已确认标识。建议任务中自动保存草稿，库中明确区分草稿与可复用版本，减少未完成模板混入库中。

上游参考：[入口与分流](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/SKILL.md)、[gen](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/gen.md)、[replace](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/replace.md)、[design](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/design.md)、[client](https://github.com/ZMGID/dsimage/blob/bc83321d19cf51f694b1aa06efbc4cdcc7effd34/skills/dsimage/guides/client.md)。

**实施顺序与验收**

1. 先修批量导入身份、样图排序、旧任务无关字段变更使结果失效的问题。
2. 给制作与试品增加已有模板起点、用原商品试做、确认效果保存为样图模板；保留旧任务可继续打开。
3. 在主流程内接入批量分类，再移除四个重复一级入口；不新增另一套工作台。
4. 快速出图缩成「要求 + 可选参考图 + 常用规格 + 生成」，高级设置按需展开。
5. 验收完整路径：商品图 → 首套试做 → 共用反馈 → 换品验证 → 确认 → 新增商品 → 只生成新增 → 导出 → 保存模板 → 从库中开启新任务；再覆盖同名目录、多层素材目录、10 页以上样图顺序、失败页与远程恢复。

**验证记录**

- `npm exec -- vitest run src/chat/images`：6 个测试文件，27 项通过，包含最新制作与试品的 8 项测试。
- `cargo test --manifest-path src-tauri/Cargo.toml --lib image_studio::`：25 项通过，覆盖规则版本、增量生成、试品确认、并发、失败和远程恢复。编译有 5 条其他模块的既有警告，无测试失败。
- 没有调用真实图片供应商，没有使用真实付费生图验证商品一致性。上述测试通过说明已覆盖的状态与组件行为成立，不能代替完整业务路径、文件导入边界和实际图片质量验收。
