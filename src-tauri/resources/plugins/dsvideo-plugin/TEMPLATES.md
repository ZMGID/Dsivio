# Dsivio 视频模板

一份模板一个 JSON，直接保存在共享目录，聊天和视频页面读取同一份文件。不要写入插件安装目录；安装目录内的 templates 只用于读取格式样例。

| 系统 | 共享目录 |
| --- | --- |
| Windows | `%APPDATA%/com.zmair.kivio/video-studio/templates` |
| macOS | `~/Library/Application Support/com.zmair.kivio/video-studio/templates` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/com.zmair.kivio/video-studio/templates` |

文件名用容易辨认的名字，如 `product-closeup.json`。不要求 UUID；原版模板中的简短 id 可以保留，没有 id 时页面使用文件名。新模板不要覆盖已有文件；修改已有模板就修改原文件。

## 保存什么

- **对话中整理的方案**：最少保存 `name` 和 `script`。未实测方案就是参考模板，无需先出视频。
- **参考视频拆解**：沿用 `skills/video-reference-analysis/templates/_template.json`，保留 `reference_template`、`full_video_prompt`、`shot_breakdown` 等原字段。
- **用户确认成片可用的模板**：沿用 `skills/ecom-h3-video/templates/_template.json`，保留 `shots`、`prompt_pattern` 和 `validated_from`。

页面兼容以上格式，不必另写 script/shots 副本或额外 kind 字段。不知道时长、画幅就不填；不要猜。

最简单的对话模板：

```json
{
  "name": "商品近景展示",
  "script": "先展示商品全貌，再切到材质细节，最后回到完整商品。"
}
```

用户要求保存时写入即可；同一版内容已有明确保存指令，无需再次确认。不要为了保存模板创建页面任务。只保留可复用的规则，不写凭据、任务编号或本机素材绝对路径。写完使用临时文件原子替换；页面刷新后即可看到。
