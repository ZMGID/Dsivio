# 聊天与图片、视频页面：仅共享模板

核对日期：2026-09-11。

## 使用边界

dsimage 是随 Dsivio 打包的内置 Skill，dsvideo 是内置插件；启动时自动发现/注册，用户无需另行安装。

聊天中的 dsimage 按原版 Skill 调用 gen/init/run 等脚本；dsvideo 按原版六个 Skill 使用生成脚本和 MCP。多轮讨论、分镜、提示词和生成结果留在对话工作目录，不创建页面任务，不同步页面草稿。页面仍保留自己的表单、任务与运行流程，历史任务和输出不迁移或删除。

聊天不提供 studio 或模板专用工具。Skill 和页面直接读写同一个模板目录。保存模板无需创建任务，也无需先生成成片。

## 模板共享

- 图片：dsimage 的 `template list/init/freeze` 使用应用数据目录 `image-studio/templates`，格式仍是 `template.json` 加参考图和 assets。页面扫描同一目录。模板直接保存到该目录即可。
- 视频：每个 JSON 一个模板，直接保存为 `video-studio/templates/易读名称.json`。最少 name + script；兼容上游 reference_template/full_video_prompt/shot_breakdown 和 template/prompt_pattern/shots 格式。无需 UUID 或额外 kind。页面只在读取时适配展示字段，不重写文件。统一保存说明在插件根目录 TEMPLATES.md。
- 所有 Agent 都按 Skill 中的平台路径读写同一个模板目录；不需要中间接口。
- 两个页面在聚焦及定时刷新时读取模板。新任务使用模板快照，后续改模板不改写已有任务。
- 凭据不进入模板。dsimage 默认直接读取图片页面 config.json 和 settings.json 中选中的模型、协议、供应商凭据；显式 `--env-file` 可独立覆盖。dsvideo 页面和插件共用上游 providers.json。

## 上游与应用适配

本次从 GitHub 重新拉取并固定：

- https://github.com/ZMGID/dsimage — `bc83321d19cf51f694b1aa06efbc4cdcc7effd34`
- https://github.com/ZMGID/dsvideo-plugin — `33af713d96d7d5d171080e23a618ed4fb37a6d8e`

聊天生成脚本恢复上游。dsimage 适配共享模板目录和页面配置读取，并在 Skill 中标明配置位置；dsvideo Skill 仅追加模板共享说明，保留可搬迁的 MCP 运行环境配置和启动器。页面定制过的视频客户端、worker、工作流资产在 `resources/video-studio`，不再覆盖聊天插件。应用升级删除旧内置包中的 STUDIO.md/studio.py 桥接文件，保留用户数据和启用状态。

## 页面内部持久化

`studio_draft`、`useSharedDraft` 的版本与恢复机制仍供页面使用。任务库、删除锁、后台执行和历史输出仍保持；这些不是聊天的共享接口。视频同一任务重新生成仍替换自己的 video.mp4。

## 验证

重点检查原版脚本测试、模板文件读写与页面发现、原生工具注册、图片/视频页面回归，以及内置 Python/MCP 启动。涉及付费生成的实际出图需单独验证，不能以静态或模拟测试替代。
