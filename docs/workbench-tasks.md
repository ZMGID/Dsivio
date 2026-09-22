# Workbench 任务清单

清单记录完整交付范围与实际状态；内部条目不作为中途停止或逐项要求用户确认的依据。每项写清「做什么 / 改哪里 / 怎么算完成」。框架约束见 [ADR 0007](adr/0007-workbench-features-come-from-one-registry.md)：功能只在 `registry.ts` 声明；生成走 `useMediaGeneration`；页面不自己 `invoke`。

图例：`[ ]` 待做 · `[x]` 完成 · `[~]` 进行中

## 当前执行：图片、视频功能拆分迁移

完整交付尚未完成。用户要求图片、视频功能全部迁入 Workbench 并退出旧执行后端；下面 M1–M4 仅为此前内部记录，不是分批交付边界。仅模板读写迁出不能宣布迁移完成。2026-09-22 用户否决当前交付与 UI 验收结论；主线程接手检查，不再派发子代理或新任务。

2026-09-22 已确定：先迁移，再继续优化 UI。原图片、视频工作室按具体功能拆进 Workbench；生成统一走 `media_generation`，分析、规划和改写复用 `run_ai_task`。迁移完成后删除旧工作室执行后端，不保留两套生成链。模板分别归内容管理的「图片模板」「视频模板」。

本节优先于后面的功能接线清单；后续清单中的占位功能不等于原工作室已有能力，也不全是本次迁移范围。执行时复用现有规则和数据，不重新造任务平台，不把整套工作室包进新页面。

### 本轮代码接入状态（2026-09-22）

- 图片独立表单：自由生图、图片复刻（整套换货）、模板套图、套图设计、批量套图；模板制作保留独立流程。
- 视频独立表单：短视频创作、视频拆解、视频复刻。旧图片/视频路由转向对应 Workbench 页面。
- 页面共用项目草稿、样品确认、记录与结果生命周期；图片项目在 `workbench/image_projects`，视频项目在 `workbench/video_projects`，均不再直接请求生成供应商。AI 规划和分析调用 `run_ai_task`；生成、回执、恢复、下载归 `media_generation`。
- 旧 `image_studio/engine.rs`、`video_studio` 命令/worker 和独立 Python 生成脚本已退出调用。历史文件目录保留兼容；分析抽帧运行环境保留在 `media_runtime`。
- 自动验证：主工作区合并后 Workbench、记录及路由前端 251 项，项目规则 40 项，共用媒体 HTTP/恢复 29 项，无会话 AI 7 项，视频输入 3 项通过；类型、局部 ESLint、协议、架构及前端构建通过。
- 已合并到 `/Users/zmair/ZM database/dsivio`，保留同期选品/工作流变更；2026-09-22 17:37 从该目录重启新编译的桌面进程。Vite 首页、启动模块及九个功能模块均 HTTP 200。同步前备份位于 `/tmp/dsivio-before-media-sync-20260922-173426`。
- 审查补修：视频进度按本地任务刷新，不再等待远程编号；明确失败可通过独立重试入口建立新尝试并保留旧回执，运行中/结果未知/可恢复下载均不重新提交。共享记录区分 HTTP 明确拒绝与传输不确定；图片参考数量按模型校验；拆解模板沿用源视频比例、时长与音轨。回归通过：前端 254 项、共享媒体 31 项、视频项目 9 项、图片生成 35 项；协议、类型、局部 ESLint、架构与前后端构建通过。20:02 已重启主项目桌面进程，首页、启动模块及视频工作区模块均 HTTP 200。
- 验收边界：没有调用付费供应商生成；原生窗口工具无法识别直接启动的开发二进制，未做逐页桌面目测；按用户约定没有改用浏览器。模块 HTTP 200 和单元测试不代表真实供应商端到端生成已验收。

### 已有基础

- [x] Workbench 功能注册表与主图生成接入代码已落地。
- [x] AI 帮写的读图、重复提交与迟到回填问题已修复并同步主工作区；相关 18 项测试、TypeScript、局部 ESLint 和差异检查通过。
- [~] 媒体生成与无会话 AI 入口代码已落地。协议导出已同步；真实模型、桌面 IPC 和产物验收随首批迁移完成，不把单元测试当作真实生成验收。

### 功能迁移去向

以下新增入口的名称是执行名称，路由 id 在接入注册表时确定；不合并现有独立功能。

| 原功能 | Workbench 去向 | 接入方式 |
| --- | --- | --- |
| 单张生成与改图 | 主图、图片编辑、精修、海报等对应功能；无商品图的自由生成保留独立入口 | 共用媒体生成，保留各自输入要求 |
| 样图换货 | 图片复刻 | 保留整套逐页换货和页序，不缩成单图 |
| 模板套图 | 独立「模板套图」 | 引用图片模板，逐商品、逐槽位生成 |
| 从零设计整套 | 独立「套图设计」 | AI 规划与修改方案，再调用媒体生成 |
| 多品类批量 | 独立「批量套图」 | 保留分类、样品确认和按类批量规则 |
| 制作、试做、修订、导入导出图片模板 | 内容管理 → 图片模板 | 迁出模板读写与规则；试做调用媒体生成，正式生产回对应图片功能 |
| 视频拍法、脚本、改写与生成 | 短视频生成 | AI 生成脚本与提示词，媒体接口生成视频 |
| 独立视频分析 | 独立「视频分析」 | 保留报告输出，供视频复刻或视频模板引用 |
| 视频拆解后换产品生成 | 视频复刻 | 复用分析结果，改写后提交媒体任务 |
| 视频模板浏览、导入、保存 | 内容管理 → 视频模板 | 保留脚本、镜头、规格与模板类型 |
| 历史、结果打开、单项重做与导出 | 对应功能的记录与结果操作 | 新任务用共用媒体记录；旧数据兼容读取 |

### 执行顺序与验收

- [x] **M1 · 图片模板、视频模板分别迁出**
  执行交接：[M1 任务文档](handoff/migrate-content-templates.md)。代码迁移与自动验证已完成（2026-09-22）；本轮按约定未执行浏览器或真实桌面验收。
  两个入口为 `workbench/image-templates`、`workbench/video-templates`。页面复用 Workbench 外框、全局 Button/Input/TextArea，直接调用带类型的模板 IPC；`src-tauri/src/content_templates/` 是读写负责人，继续使用原 `image-studio/templates` 与 `video-studio/templates` 目录。
  原图片扫描、校验、素材复制、内置模板及导入导出实现已迁出；旧存储引用和兼容命令委托新模块。旧视频 worker 不再读写模板：bootstrap 的模板由 Rust 填入，成片/分析任务的模板准备仍归旧业务，最终写入交给新模块。原模板库入口转向内容管理，生成表单保留模板选择。未改写历史任务与作品文件。
  验证：6 项独立 Rust 模板测试、2 项旧图片模板回归、28 项前端页面/注册表/视频表单回归通过；协议生成和检查、`npm run typecheck`、相关 ESLint、`npm run architecture:check`、`git diff --check` 通过。新页面 UI 静态检查 0 error / 0 warn；视频旧任务模板准备校验通过，且没有模板文件写入。所有写入测试使用临时目录，未使用真实用户模板。
  复核补修：已修复旧视频模板显式清空剧本、镜头或规格后被兼容字段回填的问题；新增 5 项回归，修前 4 项失败，修后 11 项模板测试全部通过，主线程独立复跑确认。M1 代码与自动验证验收通过，桌面验收仍未执行。
  展示迁移补修：恢复原图片模板卡片、三页样图/页面安排、逐页预览及弹窗编辑；恢复视频卡片、镜头摘要和剧本展开。使用独立模板 IPC，样式局限在两页内；界面不再直接显示 smart/replace。新增对应回归后，页面与注册表共 12 项测试通过；类型、ESLint 与架构检查通过。UI 静态扫描的原生按钮命中为独立的缩略图页面选择控件，参照 Button.tsx 对列表项等独立交互的说明复核，3 条旧间距警告为保留原卡片尺寸；未声称经过浏览器或桌面视觉验收。
  AI 制作模板、换品试做、反馈改规则和生成后提取/冻结的业务流程继续归 M2/M3；本项不代表图片视频全部迁移完成。
  在 `registry.ts` 的内容管理中增加两个独立入口。模板读写归内容管理，复用已有文件读写和规则校验；不调用旧工作室生成状态机。保留图片模板 ID、槽位、相对素材路径及 smart/replace 语义；保留视频模板 `kind/script/shots/spec`。不要求先搬磁盘目录。
  完成：已有模板能读取、编辑、导入导出；引用素材可用；业务功能引用同一份模板。图片模板试做与反馈修订随 M2 接通，不将缺少试做的管理页标为全部迁移完成。

- [ ] **M2 · 图片功能逐项迁移**
  按上表拆分；先完成单张与样图换货，再完成模板套图、套图设计、批量套图及模板试做。保留有效的业务提示词、产品事实、槽位与样品确认规则；不用旧 `image_studio::agent/engine/generation` 执行。
  完成：每项从素材到产物、历史与导出均走新入口；整套和批量记录可关联各张媒体任务，部分失败可单独处理；模板修改不会改写已有任务的方案。每项验收后删除对应旧调用，不等所有图片功能做完才开始清理。

- [~] **M3 · 视频功能逐项迁移**
  先接通短视频生成，再迁视频分析、视频复刻和视频模板使用。共用模型目录与媒体参数，不保留旧工作室单独配置供应商、提交和查询的路径。
  两个实际差异在对应功能内解决：`run_ai_task` 目前没有原生视频输入，聊天 `mixer_video_analysis` 又依赖会话附件，不能传一个本地路径就声称已接通；旧本地参考音视频与共用接口的 URL 输入不同，需按现有协议接通或明确反馈，不能静默丢弃。
  完成：生成、分析报告、复刻、模板引用分别可用；恢复查询不重新生成；历史作品与草稿可读取。字幕、剪辑、数字人和短剧的新增能力另行排期，不冒充本次旧功能迁移。

- [~] **M4 · 删除旧执行后端和旧入口**
  收尾删除原 `src/chat/images/ImageStudio.tsx`、`src/chat/videos/VideoStudio.tsx` 及其不再使用的路由分支、API 适配、Tauri 注册；删除旧图片生成引擎、旧视频 worker/生成状态机、专属配置和重复供应商请求。同步解除 `studio` 草稿/历史/等待及视频代码对旧图片类型、锁和 Agent 的依赖。
  共用 `media_generation/video_providers.rs`、数据迁移读取，以及视频分析仍需要的抽帧/元数据工具保留或迁归所属模块，不能按目录名整包删除。只有失去全部调用方的脚本和打包依赖才删除。
  完成：新功能没有旧执行入口调用；旧模板、作品和未完成任务回执不丢失、不自动重新下单；更新后的协议、类型、架构与相关测试通过。旧数据保留不等于继续运行旧后端。

迁移期间沿用 Workbench 现有布局与全局组件；只做功能接入所需调整，不开展额外视觉重设计。用户未明确要求前不使用浏览器。真实桌面验收单独记录结果。

---

## 配套收尾（不阻塞迁移盘点）

- [ ] **0.1 修 ProvidersTab 测试**
  `src/settings/tabs/ProvidersTab.test.tsx` 预设顺序断言：ComfyUI 预设排到了 Codex OAuth 前面。决定预设顺序（建议 ComfyUI 放最后，本地方案不该抢云端入口），改实现或改断言其一。
  完成：`npx vitest run src/settings/tabs/ProvidersTab.test.tsx` 绿。

- [ ] **0.2 处理 `update-dot-preview.html`**
  根目录未跟踪文件。删掉，或挪到 `docs/research/` 并说明用途。

- [ ] **0.3 实机跑通主图页**
  `npm run dev` → 设置 → 媒体创作加一个图片模型 → 工作台 → 主图生成 → 传图、填描述、出图。
  检查：结果区出现图片；换模型历史不消失；重启 app 记录还在；提交中按钮禁用。
  发现的问题记到本清单末尾「实机问题」。

---

## 图片功能接线明细（纳入 M2，新增能力另行排期）

统一做法：照 `image/MainImagePage.tsx` —— `WorkbenchMediaModelSelect` 的 `render` 拿 `provider/model`，`useMediaGeneration({ origin: workbenchOrigin('<id>') })`，`ImageStudio` 传 `modelControl` / `results`，提示词形状收进该页一个 `build<Xxx>Prompt()` 纯函数并写测试。共用入口最多接收 16 张参考图片，各模型限制由后端校验。

- [ ] **1.1 海报封面 `poster`**
  `image/PosterPage.tsx`。参考图可选，尺寸按封面类型（`IMAGE_COVERS`）映射到 `aspect_ratio`。
  完成：`PosterPage.test.tsx` 断言 `startMediaGeneration` 带 `origin: 'workbench/poster'` 和对应比例。

- [ ] **1.2 产品精修 `retouch`**
  `image/RetouchPage.tsx`。必须 1 张原图；`IMAGE_RETOUCH` 六种精修类型各自一段提示词。
  注意：精修是图生图，`images` 必填；无图时提示不提交。

- [ ] **1.3 图片编辑 `edit`**
  `image/EditImagePage.tsx`。1 张原图 + 编辑指令。现有代码里有 `revokeImages(files.slice(1))` 只留一张的逻辑，保留。

- [ ] **1.4 万物迁移 `migrate`**
  `image/MigratePage.tsx`。产品图 + 参考场景图，两组都必填，合计 ≤ 4。

- [ ] **1.5 一键换装 `dress`**
  `image/DressPage.tsx`。产品图 + 服装/模特参考。

- [ ] **1.6 图片复刻 `clone`**
  `image/CloneImagePage.tsx`。承接原样图换货，保留多页样图与商品对应关系。需要分析时通过 `run_ai_task`，生成经 `useMediaGeneration`；直接提交参考图只能作为单图路径，不能代替整套功能的迁移验收。

- [ ] **1.7 详情页生成 `detail`**
  `image/DetailImagePage.tsx`。一套多张（活页 3:4），一次 `submit` 只出 ≤ 4 张。
  不同槽位使用各自提示词；第一版逐项 `await submit`，记录媒体任务与槽位对应关系。共用 hook 会拒绝并行重复提交，不用 `Promise.all(submit)` 伪装批量。详情页与独立模板套图、套图设计分别保留用途。

- [ ] **1.8 图片页收尾**
  7 页都接通后，`ImageStudio` 的 `mediaPool` 默认分支和 `WorkbenchMediaModelSelect` 里「只对 ComfyUI 追加 Runner」的 `children` 分支就没有调用方了 —— 删掉，`WorkbenchPage.mediaPool` 一起删。
  完成：`rg mediaPool src` 无结果；测试绿。

---

## 视频功能接线明细（纳入 M3，新增能力另行排期）

- [ ] **2.1 `VideoStudio` 外框换成真实任务列表**
  `video/VideoStudio.tsx` 的 `VideoTaskList`（写死 0 条）→ `MediaTaskList`。`VideoStudio` 增加 `modelControl` / `generation` 入参，与 `ImageStudio` 同形。`VIDEO_TASK_TABS` 的「全部/进行中/完成/失败」按 `task.status` 过滤 `generation.tasks`。

- [ ] **2.2 短视频生成 `shorts`**
  `video/ShortsPage.tsx`。产品图作首帧（`options.firstFrame`，只能 1 张）或参考图（`options.referenceImages`），按模型 `videoModel(model).modes` 决定；时长/分辨率/比例走 `options.duration/resolution/ratio`。
  参考 `MediaGenerationRunner` 里 video 分支怎么组 `options`，抽成 `video/videoRequest.ts` 纯函数共用。

- [ ] **2.3 真人带货 `avatar`**
  `video/AvatarPage.tsx`。产品图 + 角色参考图。角色图先当 `referenceImages`；等 4.2 角色库做好后从库里选。

- [ ] **2.4 短剧带货 `drama`**
  `video/DramaPage.tsx`。多镜头 = 多次 `submit`，每镜头一条记录。

- [ ] **2.5 爆款视频复刻 `vclone`**
  复用独立视频分析的报告，再用 `run_ai_task` 改写为当前商品的方案，最后提交媒体生成。当前聊天 `mixer_video_analysis` 依赖会话附件，不能直接接受页面传入的本地路径；按 M3 复用已有视频处理和协议能力补齐无会话输入。

- [ ] **2.6 产品视频编辑 `vedit` / 视频字幕 `subs`（待定）**
  这两页不是生成，是剪辑/字幕（需要 ffmpeg / 语音识别）。先决定：做，还是从 `registry.ts` 删两行下线。不要留壳。

---

## AI 调用与后续图文功能

- [~] **3.1 `run_ai_task` 单入口（含 hook 与短视频页「帮我写」）**
  代码已落地，见 [ADR 0008](adr/0008-workbench-ai-calls-go-through-one-headless-entry.md)。`cargo test --lib ai_task`、自动化回归、短视频页测试已过。还差实机：配好 vision 模型后在短视频页点「帮写」，确认提示词框被填上且侧栏没有新会话。

- [ ] **3.2 `avatar` / `drama` 的「帮我写」**
  照 ShortsPage 的形状复制，各自一个 `build*AssistPrompt` 纯函数。

- [ ] **3.4 图文带货 `posts`**
  `copy/GraphicPostPage.tsx`。产品图 + 卖点 → `run_ai_task` 出文案与配图提示词；配图通过本功能 `useMediaGeneration` 提交，保留所选模型和 `origin`。文案复用现有 artifacts 能力前，先核对无会话保存及查询入口；不假定聊天产物已有按 Workbench 来源筛选的接口。

- [ ] **3.5 种草文章 `articles`**
  同 3.4，长文本输出，结果落 `.md` 文件。

- [ ] **3.6 图片复刻 / 爆款复刻的分析步骤**
  回头补 1.6 和 2.5 的「先分析再生成」。

---

## 内容管理与统计（模板优先，见 M1）

- [ ] **4.1 图片视频库 `assets`**
  `content/AssetLibraryPage.tsx`。`useMediaGeneration({})`（空过滤 = 全部）列所有产物，按 kind / origin 分组，支持打开、在 Finder 显示、删除。
  删除需要后端新增 `delete_media_task(id)`（删目录），加到 `media_generation.rs`。

- [ ] **4.2 角色库 `roles`**
  `content/RolesPage.tsx`。持久化角色（名字 + 1–3 张参考图 + 描述）。后端：`src-tauri/src/workbench/roles.rs` 新模块，存 `app_data/workbench/roles/<id>/`；命令 `roles_list/save/delete`。然后 2.3 从库里选角色。

- [ ] **4.3 使用记录 `usage`**
  `stats/UsagePage.tsx`。读现有 `usage_get_stats`，按天/模型汇总；`MediaTask` 目前不记 token/费用，先只列次数。

---

## 后续平台能力（不作为本次迁移前置）

- [ ] **5.1 决定电商自动化 5 页去留**（店铺绑定/概览/商品档案/自动化上架/上架检查）需要抖店/快手/微信小店 OAuth 与开放平台资质。没有资质前从 `registry.ts` 删行下线，代码留在目录里不删。
- [x] **5.2 智能选品：1688 同款找货 + 选品库**。视频榜单移除；同款找货接 1688 商品搜索，选品库本地 SQLite 持久化。接口与验证方式见 [智能选品](sourcing.md)。2026-09-22 桌面后端实测返回 10 条真实商品并保存搜索记录，选品读写与去重通过。
- [ ] **5.3 决定视频发布 4 页去留**（发布/账号授权/记录/数据）依赖平台发布 API。
- [~] **5.4 电商工作流：编辑器与后端执行已接通，供应商生成待实跑验收**
  2026-09-22 用户将范围扩展到后端执行。保留已有视觉结构和本机草稿；实际实现及验证记录见 [工作流任务文档](handoff/complete-workflow-editor.md)。
  - 画布新增视口定位、拖入节点、自动排列、复制选中子图（仅复制内部连接）、配置面板直接断线、节点聚焦和可收起节点库；窄窗口改为受约束的纵向布局。列表支持复制和可撤销删除。
  - `generation_workflow` 负责 DAG 校验、后台顺序执行、运行快照与逐节点检查点、取消和继续运行。AI 调用复用 `run_ai_task`；图片、视频与 ComfyUI 复用 `media_generation`。媒体回执持久保存，恢复先查原任务，不自动重发生成。
  - 支持素材/提示词输入、文本组合、文本 AI、提示词优化、图片理解、图片/视频生成、文本/图片/视频结果和图片 ZIP。视频图片明确区分首帧/参考图。条件、提取、裁剪、转换、缩放、循环和 3D 节点保留旧草稿但禁用新增，尚未实现。
  - 草稿继续使用原本机存储；后端运行快照与产物检查点原子保存到 `workflow-runs/<runId>/run.json`。离开画布只停止前端轮询，后台继续；应用重启后标记中断，由用户继续。新一次运行会重新执行全部节点，继续运行只复用原快照。
  - 验证：33 项前端测试、6 项 Rust 执行测试、TypeScript、局部 ESLint、架构检查；开发版桌面真实命令链 `node scripts/probe-workflow.mjs` 跑通文本输入→组合→输出并核对落盘结果。浏览器检查 768/1000/1280/1440 宽度及节点配置、下拉、断线、撤销和错误反馈。真实付费模型生成未执行，不据此声明供应商端到端验收完成。

---

## 长期 · 架构债

- [ ] **A. 两套生图合并**
  已提升为当前 M2 / M4，不再作为可延期架构债：保留业务规则与数据，删除旧执行后端，生成只用 `media_generation`。
- [ ] **B. 进度从轮询改事件**
  `media_generation` 完成时 `emit` 一个 `media-task-updated`，`useMediaGeneration` 订阅代替 2.5s 轮询。只在轮询明显碍事（多页同时开、任务多）时做。
- [ ] **C. `MediaTask` 记录 token / 费用**，给 4.3 用。

---

## 实机问题（0.3 及之后发现的，随手记）

- （空）
