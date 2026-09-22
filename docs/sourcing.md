# 智能选品

智能选品只保留「同款找货」「选品库」。视频榜单已从工作台注册表移除；旧榜单路径回工作台首页。

## 1688 同款找货

在「同款找货 → 1688 接口设置」填写 [1688 Skill Hub](https://clawhub.1688.com/) 签发、具有商品搜索权限的完整 AK。配置使用现有 settingsCache / 后端设置保存流程，字段为 `sourcing.alibabaAk`，保存在本机应用设置中，不内置共享密钥。不读取其他工具的凭据文件。

图片通过 Rust 原生 HTTP 直接发送给 `https://gateway.1688.com/api/alibaba.1688.find.product/1.0.0/github`。协议参考 [1688-product-find 图片搜索文档](https://github.com/next-1688/1688-product-find/blob/main/references/capabilities/image_search.md) 及该项目 `2c7d8ee` 的签名协议；使用独立 Rust 实现，不打包其 Python 源码或埋点脚本，不要求用户安装 Python。

- 接受 PNG/JPEG/WebP，最多 10MB / 4000 万像素；后端等比缩至 800×800 内，透明背景转白色 JPEG。
- 请求字段：`imgBase64`、`pageSize`、`purchaseAmount`、`scoreLevel=high`、`tags=4306497`，排序可选 `price_asc`、`price_desc`、`sold_desc`。默认品池标签与参考项目一致，不承诺覆盖全部 1688 商品。
- 认证：AK 解码为 Secret / ID，正文 MD5、时间戳、nonce、自定义头和路径参与 HMAC-SHA256 签名；不随重定向发送凭据。
- 45 秒请求超时，不自动重试。同一进程只允许一个正在执行的 1688 搜索，跨窗口或重新进入页面也不会并发重复提交。异常和空商品列表分别展示。未配置 AK 时在上传之前拒绝请求。
- 原样保留返回的报价、供应商、SKU、起批量、销量、库存；缺失指标保持空，不补零，不推算价格。相关性只是候选排序依据，不承诺确定同款。
- 当前每次最多展示请求的前 30 条商品候选。上游参考协议未声明分页，因此没有伪造“下一页”请求；结果内关键词筛选在本地完成，不额外调用接口。
- 成功搜索保存最多 100 份结果快照。历史入口重开已保存结果，不重新调用接口，不保存原始上传图片或 AK。历史刷新独立于搜索结果；本地历史写入失败时仍返回真实商品，并显示未保存提示，不自动重复请求。

AK 费用、额度、权限取决于平台账号；AK 有有效期，到期后需更新。2026-09-22 使用已授权账号和项目背包样图，通过运行中的桌面后端成功取得 10 条真实商品，包含报价、供应商、SKU、起批量、销量、库存和商品链接，并验证搜索快照持久化。此结果证明当次账号和接口可用，不代表所有图片都能找到完全同款。

## 选品库与调用归属

`页面 → src/api/tauri.ts → src-tauri/src/sourcing`。Rust sourcing 模块拥有协议、图片处理和持久化规则；搜索 hook 管理重复提交和离开页面后的迟到结果，选品库页面绑定 CRUD 意图。

本机数据库位于应用数据目录的 `sourcing.sqlite3`。收藏以 1688 商品 ID + SKU ID 去重，重复收藏保留用户编辑的备注和状态。手动添加、编辑、关键词/状态筛选、分页和删除均走同一数据库。写入使用 SQLite 事务；编辑和删除附带版本号，拒绝过期写入。

## 验证

- `cargo test --manifest-path src-tauri/Cargo.toml --lib sourcing::`
- `npx vitest run src/chat/workbench/sourcing src/chat/workbench/registry.test.ts src/chat/workbench/workbenchPages.test.ts src/chat/workbench/workbenchFeatures.test.ts`
- `npm run typecheck`、局部 ESLint、`npm run architecture:check`
- 启动更新后的 debug 桌面端，再执行 `node scripts/probe-sourcing.mjs`：调用真实桌面命令，验证保存、重复收藏、跨连接读取、编辑与状态筛选、拒绝过期编辑、删除清理，以及无 AK 时的错误；不会发送图片或消费搜索额度。

开发态线上验收可通过同一 debug probe 提交 `sourcingLive: { imagePath, akPath? }`；显式指定图片才会发送一次真实请求。可选 AK 文件只经现有 `update_settings` 负责人写入设置，探测结果不输出密钥；使用后删除临时 AK 文件。生产构建不包含该测试通道。
