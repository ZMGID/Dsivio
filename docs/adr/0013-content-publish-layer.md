# 内容平台发布只有一套实现

**状态：接受（2026-10-02）。** 依据 [Workbench PRD](../prd/workbench-prd.md) 的 P4，并服从 [ADR 0012](0012-one-implementation-three-entries.md)。

## 背景

视频要发到 TikTok 和 YouTube。两个平台的授权、分片上传和状态词都不一样，但用户看到的是同一件事：选账号、交一条视频、看每条账号的结果、失败后再试、平台没给确定结果时只查询。如果页面、对话和命令行各写一遍，重复上传和「结果不确定」会散掉。

## 决策

1. **一个模块**：`src-tauri/src/workbench/publish` 是内容发布的唯一实现。TikTok 在 `tiktok.rs`（Login Kit OAuth v2 + PKCE、Content Posting API 直发：`/v2/post/publish/video/init/` 的 `FILE_UPLOAD` 分片 PUT、`/v2/post/publish/status/fetch/`、`creator_info`，统计走 video query）。YouTube 在 `youtube.rs`（已安装应用的本机回环 OAuth 2.0 + PKCE、`videos.insert` 的 `uploadType=resumable`、`videos.list` 的 `status,statistics,processingDetails`）。
2. **凭据与记录分开**：用户自备的 client id / secret 和令牌进系统凭据库，服务名 `Dsivio.PublishAccount`。账号和发布记录进 `{app_data}/workbench/publish.sqlite3`。密钥不进 sqlite，也不进命令行进程。
3. **一次提交、一个账号一条记录**：同一 `groupId` 里每个账号至多一条。`published`、`processing`、`uploading`、`uncertain` 不会再次上传。上传已经开始之后传输失败，记为 `uncertain`，之后只能查询。`rejected` 和 `failed` 可以重试，并增加 `attempts`。
4. **统计不把「没有」写成 0**：`VideoStats.values` 只放平台真实返回的指标；没有的键进 `unsupported`。YouTube Data API 没有分享数，因此 `shares` 在不支持列表里。
5. **传输可替换**：平台 HTTP 走 `Transport`。生产用 reqwest；测试回放与官方文档同形的响应，不访问外网。
6. **入口**：工作台走 `publish_*` Tauri 命令和 `src/api/tauri.ts` 的 `api.publish*`。`dsivio publish` 与 `app_cli` 的 `service: "publish"` 调同一个 `handle`。对话里的原生工具 `publish` 也只调这个模块；`submit` 和 `retry` 会上传，`bypasses_approval` 为 false，必须经过批准。已发布或结果不确定的记录只查询，不重新上传。

## 后果

- 新平台加一个适配文件，并在发布流程里 `match`，不复制提交、防重复和不确定分类。
- 页面只组 `PublishRequest` 和展示记录；不直接 `invoke`。
- 没有真实账号时，用录制响应验收授权、上传、状态、统计、防重复和不确定分类。
