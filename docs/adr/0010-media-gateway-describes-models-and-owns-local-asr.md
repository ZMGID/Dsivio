# 媒体网关描述真实参数，并拥有本地转写生命周期

**状态：接受。** 本 ADR 是架构决策，不是所有阶段能力已上线的声明；W1 实现图像/视频参数描述与接收，Speech、ASR 和取消须各自通过实机验收才公开。

## 背景

ADR 0009 已确定 App 媒体生成只有一套实现，CLI 是本机客户端。图片/视频的固定输入与能力表尚不能表达语音、逐字转写、扩展参数及取消。本地 WhisperX 有安装、模型准备、队列和进程生命周期，不能散落到设置页、插件与 CLI 各自监督。

## 决策

1. **描述与输入**：`media_generation` 是 App 唯一媒体任务负责人。`media models` 为每个有效连接/模型公开版本化参数描述；校验、默认与实际可发送参数来自同一 route 描述。`model_parameters` 解释有限规则，route owner 负责生成事实及白名单编码；外层请求继续拒绝未知字段，内部参数只接受已声明 key，不允许任意 vendor body 透传。CLI、页面、对话和工作流走同一校验，preview 也不建立第二套参数规则。`factsRevision` 是移除 revision 本身后、按 RFC 8785 / JCS 规范化 UTF-8 JSON 的 SHA256；指定 revision 不符时，付费提交前拒绝并要求重新计划。旧 capabilities 仅投影描述。
2. **Speech**：首批 MiniMax TTS/授权样本 clone→TTS、OpenAI TTS 进入同一 task 生命周期。凭证由 App 持有，模型协议/产品地址显式配置；聊天/视频 key 不推定有语音权限。clone 子步骤持久化，结果不确定不重交；OpenAI 样本 clone、声音设计与 Volcengine 语音不在本次实现范围。音频输出必须验证真实格式。
3. **本地服务与 ASR**：App 拥有 WhisperX 的安装、队列、child、关闭和取消。源码唯一在 `dsivio-video/services/asr`，Host 发行复制同 revision/hash 的资源快照。手动安装与首次真实需要共用单航班入口，plan 不安装；只在准备阶段下载，推理离线。安装失败不替换旧环境，不自动改为云端。首批云 ASR 仅显式启用的 OpenAI whisper-1。
4. **取消事实**：取消返回 confirmed/requested/unsupported/too-late，并分 local/remote 与费用事实。abort 客户端不代表远程取消/退款。取消不删除回执或产物，迟到结果不能覆盖 confirmed cancelled。存在取消/删除混合竞态的接口不公布 remote cancel。
5. **抠像延期**：静态去背景与人像视频 alpha 输出必须有真实 provider/产品权限/验证才公开；本次不发布无执行者的 matting 能力。导入透明素材不是生成抠像完成。
6. **插件边界**：Dsivio 模式插件不读 App key、不直连厂商、不启动第二套本地 ASR。standalone 是用户明确选择的无 App 模式，在插件侧使用用户自有 env/私有文件 key；不能作为一个已选 Dsivio Build 的执行失败 fallback。backend 在 plan 固定。

## 后果与验收

任务/状态只有一个 App owner，设置保存沿用既有 CAS；描述新增参数时必须同时实现输入校验和 route 编码。新增语音/ASR/取消状态的已有调用者与任务 UI 同批迁移。实机证明本地自动/手动安装、退出/取消清理；真实付费图/视频/语音与云 ASR 证明产物，不用 mock 替代。用户自有明文 key 的安全限制如实说明，不声称加密。

图像/视频通用参数用 `--options-json` 或互斥的 `--options-file` 传入；规范名与旧 flag/alias 重复必须拒绝。未提供的可选 `mediaList` 端口省略，不能用 `[]` 伪装已提供的至少一个条目；已有参考素材页面在仅视频/音频参考时不再发送空的 `referenceImages`。`background`/`outputFormat` 仅在真正发送这些字段的 OpenAI 图像路线公开，JSON 与 multipart 一起映射。不支持的参数、模型规则和 revision 错误先于 task 创建和厂商请求。未知模型只公布基本提示词路线，不凭名称猜任意扩展参数。不能根据供应商存在就声称全部事实完整或价格为零。

## 对 ADR 0009 的关系

扩展其能力与任务终态，不改变 App 内单一实现、CLI 无密钥和不确定提交不重交原则。standalone 不读取 App 设置，因此不是 0009 范围内的平行协议实现；旧 CLI 公共 flags 保留，旧能力输出从新 description 投影，不另维护旧参数校验器。
