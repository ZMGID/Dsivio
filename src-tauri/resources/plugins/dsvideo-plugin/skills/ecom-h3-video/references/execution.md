# 运行与恢复

- 应用提供 `DSVIDEO_PYTHON`、`DSVIDEO_RUNTIME_ROOT`、`DSVIDEO_CONFIG_PATH`。脚本使用 `"$DSVIDEO_PYTHON" <脚本绝对路径>`；不要猜测系统 Python 环境、安装 Pillow 或搜索其他虚拟环境。
- 图片尺寸、补边、成片规格可用内置 FFmpeg / ffprobe（`$DSVIDEO_RUNTIME_ROOT/bin` 与 `analyzer/node_modules/ffmpeg-static`）；一般商品参考图不需要逐像素探测背景色。只在接口要求首帧比例时做必要的缩放补边。
- 带空格目录通过 bash 工具的 `cwd` 参数指定，路径逐项加引号。需要完整错误时直接运行命令，不用 `2>/dev/null`、`head`、`tail` 隐藏错误。长输出使用工具返回的日志文件路径按需读取。
- 普通 ComfyUI 出片顺序：`upload_file` → `prepare_workflow.py` → `run_workflow(wait:false)` → 已有 job 的状态/等待 → `fetch_outputs` → 成片检查。环境、节点与工作流由内置工具负责，不主动扫描全部模型或研究节点。
- 配置读取使用 `dsvideo_config.py show`，它会脱敏。若 MCP 报错，读取其中的服务地址和原因；配置与目标不一致时刷新连接。不要用临时 HTTP 脚本替代提交，不修改插件安装目录。
- 上传、转换失败且明确没有提交时，修正同一问题再试。已返回任务编号就只恢复那个任务；HTTP 500、连接中断或超时且没有回执时，不根据“没看到编号”推断未接单，先核查任务状态。不要重复提交来探测错误。
- Grok/MiniMax 使用各自 `generate` / `status` / `wait` 客户端，不临时写一套提交器或对客户端打运行时补丁。读到网关兼容错误时报告具体错误，不猜测余额、权限或已扣费用。
- 官方报价是参考估算；自定义网关以该网关的费率和账单为准。未经账单证据，不把估价写成实付，不宣称失败请求一定未计费。
- 检查成片的尺寸、时长和音轨；有口播要求时转录回听，抽帧检查商品一致性。实际规格不符应明确标记偏差并保留文件；不得直接称“规格验证通过”。不要自动重购来修正瑕疵。
