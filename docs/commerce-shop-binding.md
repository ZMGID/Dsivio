# Workbench 店铺绑定

首期只覆盖 Shopee、SHEIN、TikTok Shop、Mercado Libre 的授权绑定、店铺信息、连接状态检查与本地解绑。店铺概览、商品档案和上架流程尚未接入这些平台的真实业务数据。

## 使用前准备

1. 在对应平台创建开发者应用，取得应用 ID / Key 和密钥，并登记 HTTPS 回调地址。TikTok Shop 还需从 Partner Center 复制包含 `service_id` 的**卖家**授权链接；Mercado Libre 需选择目标国家或地区，并按应用后台设置选择是否启用 PKCE。
2. 在 Workbench 的「店铺绑定」选择平台，填写应用信息并打开授权页。卖家完成授权后，复制浏览器地址栏中完整的回调 URL，粘贴回应用完成绑定。授权码通常短时且只能使用一次，失败时重新发起授权。
3. 店铺列表展示上次连接检查的结果和时间。点击「刷新」才会向平台重新查询，并在需要时刷新令牌。TikTok Shop 一次授权可能返回多家店铺。
4. 「解绑」删除本机记录和系统凭据库中的密钥；若要撤销平台侧授权，需要到对应卖家中心操作。

系统凭据库保存应用密钥、平台令牌及 SHEIN 商家密钥；`workbench/shops.sqlite3` 仅保存店铺 ID、名称、地区、状态和检查时间。授权中的应用密钥只暂存在进程内存，未完成授权不会入库；应用重启后需重新发起未完成的授权。

## 协议依据

- [Shopee Open API 授权、换取及刷新令牌](https://cdngarenanow-a.akamaihd.net/shopee/seller/seller_cms/c575929f948611337e1249564c2b8ff6/%5BTW%5D%5BOpen%20API%5DAPI%20v1_v2%E6%8E%88%E6%AC%8A%E6%96%B9%E6%B3%95%20%282020_09%29_newnew.pdf)
- [SHEIN 用 tempToken 换取店铺密钥](https://open.sheincorp.com/documents/apidoc/detail/3001520-1000012)、[查询店铺信息](https://open.sheincorp.com/pt/documents/apidoc/detail/3001499)、[官方 SDK 签名与解密实现](https://github.com/sheinsight/open-sdk-js)
- [TikTok Shop 卖家授权](https://partner.tiktokshop.com/docv2/page/seller-authorization-guide)、[查询已授权店铺](https://partner.tiktokshop.com/docv2/page/call-get-authorized-shops)
- [Mercado Libre 授权与令牌](https://developers.mercadolibre.com.mx/en_us/authentication-and-authorization)

没有平台应用凭据和卖家测试店铺时，仓库内只能验证本地回调校验、签名、存储和编译；真实授权与状态查询须使用对应平台的测试或正式账号做端到端验收。
