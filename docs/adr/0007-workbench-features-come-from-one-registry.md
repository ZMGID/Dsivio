# 工作台功能只在一张注册表里声明，生成流程只有一个前端负责人

工作台会长期增删功能（目前 8 组 32 项，且会继续变）。在这之前，加一个功能要改五处：`workbenchPages.ts` 的 id 联合类型 + 数组 + 导航表、`chatRoutes.ts` 的 `ChatExtensionsNavItem` 联合 + 32 行 `||` 判定链、`WorkbenchHome.tsx` 的 32 分支 `switch`、外加 i18n 和页面文件。五处任何一处漏掉都是静默失效（路由回首页、侧栏不高亮、搜索搜不到）。

现在：**`src/chat/workbench/registry.ts` 是工作台「有哪些功能」的唯一权威。** 每个功能一行：`{ id, group, label, icon, load }`。

- `WorkbenchSubpageId` 从表推导（`(typeof WORKBENCH_FEATURES)[number]['id']`），`chatRoutes.ts` 用模板类型 `` `workbench/${WorkbenchSubpageId}` `` 和 `isWorkbenchSubpage()` 判定，不再枚举。
- 侧栏、首页目录、功能搜索读派生视图 `WORKBENCH_NAV`（`workbenchPages.ts`），空分组自动消失。
- `WorkbenchHome.tsx` 用 `feature.load()` 懒加载页面组件，每页只创建一次 `lazy`；首次进入才下载该页代码。
- `registry.ts` 是叶子模块：只依赖图标和 i18n 类型，不 import `chatRoutes` 或页面本体（页面通过 `load: () => import(...)` 延迟引用），因此 `chatRoutes → registry` 不成环。

**加一个功能** = 在对应分组目录建页面文件 + 表里加一行 + 补两个 i18n 标签。**删一个功能** = 删页面文件 + 删表里那一行。`registry.test.ts` 会实际 `load()` 每一项，漏改 import 路径在测试里就会失败。

## 页面目录与归属

```
src/chat/workbench/
  registry.ts            功能表（唯一权威）
  workbenchPages.ts      hash ↔ 页面 id ↔ 导航项；WORKBENCH_NAV 派生视图
  WorkbenchHome.tsx      中心区：读 hash，懒加载对应页
  WorkbenchSidebar.tsx / WorkbenchLanding.tsx / WorkbenchFeatureSearch.tsx   读 WORKBENCH_NAV
  WorkbenchPage.tsx      页面外框、卡片、CTA、空态（共享 UI）
  useMediaGeneration.ts  媒体生成流程负责人（见下）
  MediaTaskList.tsx      生成记录默认展示
  MediaGenerationRunner.tsx  通用生成表单（按模型能力渲染），是 hook 的消费者
  <group>/<Feature>Page.tsx  每个功能自己的页面；只在附近用的纯函数和 catalog 放同目录
```

一个功能页只负责：绑定意图（表单 → 请求）和展示。它不自己 `invoke`、不自己轮询、不自己维护任务列表。

## 媒体生成：一个流程负责人，页面按来源列自己的历史

图片创作 8 页、视频创作 6 页最终都要出图出片。它们共用一条链路：

- 后端 `media_generation.rs` 已是统一任务模型（云端图片 / 云端视频协议 / 本地 ComfyUI 同一个 `MediaTask`，落盘 `media-tasks/<id>/task.json`，进程重启可恢复）。本次给 `MediaRequest` / `MediaTask` 加了 `origin`（谁发起的，如 `workbench/main`、`chat`）和 `prompt`，`list_media_tasks` 改为按 `MediaTaskFilter { providerId?, model?, origin? }` 过滤。ComfyUI 任务同样带 `origin`。旧记录缺这两个字段时按默认值读取，永远不会匹配到某个 `origin`。
- 前端 `useMediaGeneration(filter)` 是唯一流程负责人：列表、运行中轮询（2.5s）、提交去重、作用域切换后丢弃迟到结果、恢复查询。它从原 `MediaGenerationRunner` 里抽出——那是唯一一处已经把这套规则写对了的地方，Runner 现在只是它的一个消费者。
- 工作台页面用 `useMediaGeneration({ origin: workbenchOrigin('main') })`：**历史属于页面，不属于模型**。用户换模型不会让上一批结果消失；聊天里的图片/视频工作室仍按 provider+model 列，两者都是同一个 hook 的不同过滤条件。
- 模型选择仍是 `WorkbenchMediaModelSelect`（读 `settings.workbenchMedia` 模型池，本机记住上次选择）；后端 `start_media_generation` 只接受模型池里的模型。

第一个按此接通的页面是「主图生成」（`image/MainImagePage.tsx`）：表单 → `buildMainImagePrompt()`（该页唯一决定提示词形状的地方）→ `generation.submit({ ..., origin })` → `MediaTaskList` 展示。后续图片/视频页照这个形状接，不再各写一份提交与轮询。

## Considered Options

- **继续手写三处枚举，靠 review 兜底**：否决。已经出现两份 32 项列表和一条 32 行判定链，且 ADR 0006 当时就预告「等页面铺开再改表驱动」，现在到了。
- **把功能表放进 `chatRoutes.ts`**：否决。`chatRoutes` 是全窗口路由词表，不该知道工作台每一页；它只需要「这个后缀是不是工作台的页」这一个谓词。
- **每个功能目录自己 `export const feature = {...}`，注册表 `import` 汇总**：否决。汇总文件仍要逐个 import，和一行表项相比没少改地方，却让页面模块在启动时全部被加载（失去懒加载）。
- **给工作台再做一层通用「Job」抽象（进度 / 持久化 / 取消）**：否决。`MediaTask` 已经是那个东西；自动化有自己的 run 历史；agent 有自己的 run 生命周期。第三套只会让调用方多学一个词。等真的出现非媒体、非 agent 的长任务再说。
- **在页面里保留原来的表单，只在下面追加通用 Runner**：否决（这是改动前 `mediaPool` 的实际行为，且只对 ComfyUI 出现）。结果是一页两个表单、两个按钮，用户不知道该点哪个。已接通的页面自己持有模型控件和结果区（`ImageStudio` 的 `modelControl` / `results`），未接通的页面沿用旧默认。

## Consequences

- 新功能不要再碰 `chatRoutes.ts`、`WorkbenchHome.tsx`、`WorkbenchSidebar.tsx`；如果发现必须改，说明那不是「加一个功能」，先回来改这份 ADR。
- 功能 `id` 就是用户书签和 `chat_remember_last_route` 里保存的路由后缀，改名视为破坏性变更。
- 生图/视频页接后端时，从 `useMediaGeneration` + `MediaTaskList` 起步；只有当某页确实需要不同的提交规则（比如多步：先让 agent 写提示词再出图）时，才在该页加一个流程函数，并且仍然通过 `generation.submit` 落盘。
- Agent 参与工作台（帮写提示词、图文创作）目前没有独立于对话记录的一次性调用入口——`run_agent_loop` 只经 `chat_send_message`、自动化 `action.agent` 或子代理触发。这是下一步要在 `chat/agent` 侧补的 Interface，不在工作台目录里造第二条 agent 调用路径。
- `Chat.tsx` 本次未改动，仍只有路由判定一行和中心区渲染分支（ADR 0006 的约束继续有效）。
