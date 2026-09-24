# 国内电商平台店铺绑定授权资料（2026-09-24）

为店铺绑定（`src-tauri/src/workbench/shops/`）接入 5 个国内平台收集的接入协议资料。官方文档站（op.jinritemai.com、open.kwaixiaodian.com、open.pinduoduo.com、open.taobao.com）均为 JS 渲染的 SPA，无法直接抓取正文，以下端点以多个开源 SDK 实现与开发者文档互相印证；标注「需实测」处首绑时可能要按平台实际返回微调。

统一约定：授权页模式（抖店/快手/淘宝/拼多多）复用现有 `shop_begin → 浏览器授权 → 粘贴回调 → shop_complete` 流程；微信小店无授权页，走 AppID+Secret 直接绑定。

## 抖店（douyin）

- 授权链接（工具型应用）：`https://op.jinritemai.com/open/authorize?app_key={app_key}&state={state}`。回调地址在开放平台应用后台登记，授权后回调带 `code`、`state`。`app_id` 与 `app_key` 是同一概念。
- 换 token：POST `https://openapi-fxg.jinritemai.com/token/create`，query 公共参数 `app_key`、`method=token.create`、`timestamp`（秒）、`v=2`、`sign_method=hmac-sha256`、`sign`；body 为 `param_json={"code":..,"grant_type":"authorization_code"}`。
  - 签名：`hmac_sha256_hex(app_secret, app_secret + "app_key"+app_key + "method"+method + "param_json"+param_json + "timestamp"+ts + "v2" + app_secret)`（键值直接相连，首尾包 app_secret）。[param_json 键序不影响：服务端解析后重排]
  - 响应：`{"code":10000,"msg":"success","data":{"access_token","expires_in"（秒，默认 7 天）,"refresh_token"（14 天）,"shop_id","shop_name"}}`。**code==10000 才是成功**，与现有 `platform_ok`（判 0）不同。
- 刷新：同网关 `token/refresh`，`param_json={"grant_type":"refresh_token","refresh_token":..}`，响应同结构。
- 身份：token 响应自带 `shop_id`/`shop_name`，无需再查店铺接口。
- 依据：zsmhub/doudian-sdk（Go，2024）`core/create_token_request.go`、`core/refresh_token_request.go`、`utils/sign_util.go`、`core/doudian_op_api_base_response.go`；uicky/open-jinritemai（Java，2026）配置 `gateway-url: https://openapi-fxg.jinritemai.com`；cnJun/sdk4-jinritemai；op.jinritemai.com 文档标题索引（「工具型应用店铺授权流程」「使用refresh_token刷新access_token」）。

## 快手小店（kuaishou）

- 网关：`https://openapi.kwaixiaodian.com`（open.kwaixiaodian.com 为文档/控制台）。
- 授权链接：`https://openapi.kwaixiaodian.com/oauth2/authorize?response_type=code&client_id={app_id}&redirect_uri={redirect}&state={state}`（scope 可选）。**此拼接来自 Python SDK，未见官方原文，需实测。**
- 换 token：GET `https://openapi.kwaixiaodian.com/oauth2/access_token?app_id={app_key}&app_secret={secret}&grant_type=code&code={code}`。注意 `grant_type=code`（非标准 authorization_code），以 G-YDG PHP SDK（被多个商用集成引用）为准；AndersonBY Python SDK 用 POST + `grant_type=authorization_code`，两源不一致——实现取 GET+`code`，若实测失败换 POST。响应：`access_token`、`expires_in`（秒）、`refresh_token`、`refresh_token_expires_in`；错误体含 `error`/`error_description`。
- 刷新：POST `/oauth2/refresh_token`，form：`grant_type=refresh_token&refresh_token&app_id&app_secret`（两 SDK 一致）。
- 店铺身份：API `open.user.seller.get`（GET）。公共参数 `method`、`appkey`、`access_token`、`version=1`、`signMethod=HMAC-SHA256`、`timestamp`（毫秒）、`param`（业务参数 JSON，空则不参与），URL 为网关 + method 点转斜杠（`/open/user/seller/get`）。响应 `data`：`name`、`sellerId`（数字）、`openId`、`head` 等。
  - 签名：`base64(hmac_sha256(app_secret, sortedParams 以 k=v& 连接 + "signSecret=" + app_secret))`；MD5 备选（hex）。选 HMAC-SHA256 避免引入 md5 依赖。
  - API 成功判定：响应含 `result`（==1 成功）——需实测确认错误结构。
- 依据：AndersonBY/kwaixiaodian-python-sdk `auth/oauth.py`、`auth/types.py`、`auth/signature.py`、`models/user.py`；G-YDG/kwaixiaodian-sdk `src/Oauth/Oauth.php`、`src/KwaixiaodianApi.php`、`src/Api/User/User.php`。

## 微信小店（wechat）

- 模式：自营店铺无网页授权流程，小店 AppID（即小店 ID）+ AppSecret 直接取 token。
- token：POST `https://api.weixin.qq.com/cgi-bin/stable_token`，JSON `{"grant_type":"client_credential","appid","secret","force_refresh":false}` → `{"access_token","expires_in":7200,"errcode":0,"errmsg":"ok"}`（stable_token 不会顶掉并发获取的旧 token）。备用 GET `/cgi-bin/token?grant_type=client_credential&appid&secret`。
- 店铺信息：GET `https://api.weixin.qq.com/channels/ec/basics/info/get?access_token=..` → `{"errcode":0,"info":{"nickname","headimg_url","subject_type"}}`。
- 身份：`remote_id` = 小店 AppID；名称 = `info.nickname`。无 refresh_token，过期后重新获取即可。
- 流程差异：UI 不打开浏览器授权页，凭证填完直接「完成绑定」；`shop_begin` 返回空 URL，`shop_complete` 跳过回调校验。
- 依据：zsmhub/wx-channels-sdk `apis/基础-获取access_token.go`、`apis/基础-获取店铺基本信息.go`（官方文档 developers.weixin.qq.com/doc/channels/API/basics/getaccesstoken.html）。

## 淘宝（taobao）

- 授权链接：`https://oauth.taobao.com/authorize?response_type=code&client_id={app_key}&redirect_uri={urlencode}&state={state}&view=web`（redirect_uri 必须与应用登记一致）。
- 换 token：POST `https://oauth.taobao.com/token`，form：`grant_type=authorization_code&code&client_id&client_secret&redirect_uri&view=web`。响应：`access_token`、`expires_in`（秒）、`refresh_token`、`re_expires_in`/`refresh_token_expires_in`、`w1_expires_in`、`taobao_user_id`、`taobao_user_nick`；失败 HTTP 4xx 且体含 `error`/`error_description`（现有 `response()` 已能提取）。
- 刷新：POST 同端点，`grant_type=refresh_token&refresh_token&client_id&client_secret&view=web`。
- 身份：token 响应自带 `taobao_user_id`/`taobao_user_nick`，无需网关 API（也就不需要 TOP 的 MD5 签名）。
- 依据：open.taobao.com 文档《淘宝OAuth2.0服务》(docId=118)、《用户授权介绍》(docV3 docId=102635) 的搜索摘要与多篇实战文章（腾讯云 2019、阿里云 2023 等），字段名 `taobao_user_id/taobao_user_nick/w1_expires_in` 在 taobao.com 技术文档《登录认证》中逐一列出。

## 拼多多（pinduoduo）

- 授权链接（商家 WEB 端正式环境）：`https://mms.pinduoduo.com/open.html?response_type=code&client_id={client_id}&redirect_uri={urlencode}&state={state}`（旧文档 mms.pinduoduo.com/open-auth.html 已被 open.html 取代；justmd5 SDK 用 fuwu.pinduoduo.com/service-market/auth 为服务市场变体，取官方文档口径 open.html）。
- 换 token：POST `https://open-api.pinduoduo.com/oauth/token`，JSON `{"grant_type":"authorization_code","code","client_id","client_secret"}`。响应：`access_token`、`expires_in`（秒）、`refresh_token`、`refresh_token_expires_in`、`owner_id`（商家店铺 id）、`owner_name`（商家账号名）、`scope`。等价网关 API `pdd.pop.auth.token.create` 返回同构字段（dcsunny Go SDK 逐字段印证）。
- 刷新：POST 同端点，`{"grant_type":"refresh_token","refresh_token","client_id","client_secret"}`。网关 API `pdd.pop.auth.token.refresh` 等价。
- 身份：token 响应自带 `owner_id`/`owner_name`，无需调 `pdd.mall.info.get`（也就不需要网关 MD5 签名）。
- 依据：mai.pinduoduo.com《用户授权介绍》搜索摘要（授权入口 open.html + oauth/token 示例）；dcsunny/pinduoduo-sdk `auth/access_token.go`；justmd5/pinduoduo-sdk `src/Oauth/PreAuth.php`；简道云/CSDN 接口清单（pdd.mall.info.get 存在但不需要）。

## 实现影响汇总

- `Platform` 增 5 个变体（serde lowercase 与前端 id 对齐：douyin/kuaishou/wechat/taobao/pinduoduo）。
- 微信小店需要「无授权页」分支：`authorize_url` 返回空串、`callback_params` 放行、前端跳过打开浏览器与粘贴回调步骤。
- 抖店响应成功码 10000，微信错误字段 errcode/errmsg，拼多多错误体 error_response——`platform_ok`/错误提取需按平台分支。
- 快手需要 API 签名（HMAC-SHA256/base64），其余 4 家凭证流均无网关签名需求。
- 所有 token 有效期均以响应内 `expires_in`（秒）为准，复用现有 300 秒提前刷新。
