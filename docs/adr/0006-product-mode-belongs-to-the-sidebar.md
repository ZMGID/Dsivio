# 形态归侧栏边界所有，不抬到 Chat

形态（对话 / 工作台）的状态和持久化住在 `src/chat/ChatSidebarPane.tsx` —— 那一列的 React 子树边界。`Chat.tsx` 不持有形态，也不知道有这回事。

依据很简单：**形态只决定左边那一列长什么样**。中心区看的是路由里的 `chatView`，跟形态无关；图片、视频、自动化这些页两种形态下都能打开，走同一套路由词表。既然没有第二个读者，这个状态就没有理由离开侧栏。

切形态之后要落到该形态的首页，这两条去处用的是侧栏本来就有的回调：工作台走 `onOpenExtensionsItem('workbench')`，对话走 `onNewConversation()`。两者在 `Chat.tsx` 里都已经包过 `runAfterLeavingSettings`（设置页开着时先播退场、flush 自动保存），所以形态切换不需要自己再处理一次设置页退场。

## Considered Options

- **形态做成路由段（`#chat/workbench/images`）**：否决。图片 / 视频 / 自动化这些页两种形态下都成立，做成路由段等于给每个中心页开两条 URL，还要处理 Rust 那边路由恢复时的形态回填。形态换的是导航，不是地址。
- **形态 state 提到 `Chat.tsx`，两个侧栏在那里三元选一**：先这么写过，否决。`Chat.tsx` 里多出一个 state、两个 handler、外加一整份 14 个 props 的工作台侧栏 JSX（共 +58 行），而那份 props 跟旁边 `ChatSidebarPane` 的几乎逐字重复。协调者拿不到任何好处——它不读形态。
- **抽一个 `useProductMode` hook 放 `src/chat/hooks/`**：否决。`hooks/` 下那些是跨区域的流程负责人（路由、消息队列、右栏）。形态没有跨区域，抽 hook 只是把 state 搬个文件，调用方该知道的事情一件没少，过不了规范里的「删除测试」。
- **工作台侧栏自己声明一份 props 接口**：否决。两个侧栏是同一列的两副面孔，共用十几个 prop。改用 `Pick<SidebarProps, ...>` 绑住，共用 prop 改名时工作台侧栏直接编译报错，而不是悄悄失配。

## Consequences

- **不要给 `Chat.tsx` 加形态相关的 state、handler 或 props。** `.eslintrc.cjs` 里有 `max-lines` 棘轮盯着这个文件，加了会挂 lint；但棘轮只拦体积，形态本身的归属靠这份 ADR 和 review。
- 工作台新增页时，`Chat.tsx` 只多两样：`isChatWorkbenchXxxPath` 的路由判定一行，和中心区的渲染分支（照抄 `ArtifactsCenter` 那几个的写法）。导航条目加在 `WorkbenchSidebar` 里，不经过协调者。
- 形态存在 `localStorage`（`src/chat/productMode.ts`），不进后端设置：它是纯 UI 偏好，没有跨窗口或跨端语义。副作用是多窗口下各窗口的形态独立——当前只有聊天窗口有侧栏，不构成问题。
- 路由恢复和形态是两件独立的事。用户上次停在工作台首页时靠路由词表里的 `workbench` 段恢复；如果恢复到的是一条对话，而记住的形态是工作台，会看到工作台侧栏 + 一条对话，侧栏首页不高亮。这是可接受的组合，不要为了「对齐」在启动时互相改写另一方。
- 中心区那串 `chatView === 'xxx'` 的三元链还在长。等工作台真的铺开页面、这串明显碍事时再考虑改成表驱动；现在照抄同构写法比提前抽象更好读。
- **后续（ADR 0007）**：工作台内部的页面枚举已改成表驱动——功能只在 `src/chat/workbench/registry.ts` 声明一次，`chatRoutes` / 侧栏 / 首页 / 中心区都从它派生。本条说的 `Chat.tsx` 那一行判定和一个渲染分支不变。
