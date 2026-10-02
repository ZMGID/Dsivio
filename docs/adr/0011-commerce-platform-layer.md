# 电商平台层只有一套实现

**状态：接受（2026-10-02）。** 依据 [Workbench PRD](../prd/workbench-prd.md) 与 [ADR 0012](0012-one-implementation-three-entries.md)。

## 背景

店铺绑定已经把各平台的授权、令牌刷新和签名放在 `workbench/shops`。商品、类目、订单和上架如果再在页面、对话工具和命令行里各写一遍，指标口径和「提交结果不确定」的规则会分叉。

## 决策

1. **一套实现**放在 `workbench/commerce`。上架记录、去重、重新提交和结果分类只在这里改。九个平台（Shopee、TikTok Shop、SHEIN、Mercado Libre、抖音、快手、微信小店、淘宝、拼多多）各一个适配器，由 `adapter.rs` 统一分发；某平台不开放的能力由 `capabilities_for` 声明为不支持，页面据此显示，不假装成功。目前所有适配器只对照官方文档的录制响应验证，没有真实店铺验证。
2. **不另存令牌**。调用前通过 `shops::open_shop_session` 走现有的 `shop_check` / `verify` 刷新和轮换。签名复用 `sign_shopee`，不复制 HMAC 规则。
3. **三个入口只做转接**：
   - 工作台：`commerce_*` Tauri 命令 → `api.commerce*` → `useCommerce`。页面不直接 `invoke`。
   - 对话：原生工具 `commerce`。`submit` 和 `resubmit` 在默认审批策略下需要确认；只读操作不需要。
   - 命令行：`dsivio commerce` 经 `app_cli` 连到正在运行的 App，进程不读密钥。
4. **上架记录**存在 `{app_data}/workbench/commerce.sqlite3`。同一次提交的多个店铺共用一个 `groupId`，每个店铺一条记录。审核中、在售、提交中、不确定或封禁会挡住新建；已拒绝或失败必须 `resubmit` 同一条记录。传输层在 `add_item` 发出之后失败，记为不确定，只能查询。
5. **指标**按店铺时区从订单和退货单汇总。接口没有的键放进 `unsupported`，数值记 0，不用 0 假装平台支持。订单接口的令牌错误是失败，不返回全 0。

## 后果

- 新平台只加适配器和能力声明，不改三个入口的协议。
- 页面后续接入时只提交草稿和展示记录，不自己决定能否重试。
