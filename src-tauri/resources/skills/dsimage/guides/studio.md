# 聊天与图片页面共用

命令在本 Skill 目录运行。Windows 使用 `python`，macOS / Linux 使用 `python3`。保留原 dsimage 的 gen、replace、smart、design 流程；这里说明内置版的数据位置差异。

## 模板

先运行 `python scripts/dsimage.py studio paths`，它返回实际可写模板目录。所有指南中的 `templates/` 均指这个共享目录，不是 Skill 安装目录。`template list` 同时列出页面模板和聊天模板。

- 创建：沿用 `template init <名称> --blank --mode smart --slots 9` 或 `--from <样图目录>`，脚本直接写进共享目录。随后填好 `template.json`，执行 `template check <名称>`。
- 修改用户模板：通过 `core.find_template` / `template list` 定位同一份 `template.json`；只改需要改的字段，保留未知字段、相对参考图路径、分支及辅助素材。写文件用临时文件后原子替换，避免页面读到半份 JSON。不要修改 `record.json`，它只存页面身份元数据。
- 修改内置模板：目录名以 `builtin-` 开头的是只读内置模板。先运行 `studio copy-template <原模板名> <新名称>`，再改副本，避免应用更新覆盖。
- 完成后告知用户模板名称，可直接在「图片 → 模板库」使用；页面打开时数秒内自动更新，切回来也会刷新。不要要求用户再导入一遍。
- 页面创建、复制、冻结的模板也会写出标准 `template.json`，聊天可直接 `init --template <名称>` 使用。
- 同一模板不要同时在聊天与页面编辑。保存前重新读文件，保留另一边新加入的字段；发生冲突时让用户选择保留哪个版本。

## 配置与产物

配置统一走 `SETUP.md`。不要用 `setup env`、`setup model` 或 `.env` 建第二份配置；内置 Skill 随应用更新。

套图执行使用图片设置中的模型，模板旧的 `model` 字段保留为兼容信息，不覆盖应用配置。单张 `gen --model` 仍可按用户明确要求临时指定模型。

原脚本的批次格式仍是 `_dsimage/batch.json`，原图和交付文件留在用户指定的成图目录。页面任务使用 `tasks/*.json`；不要把两种任务 JSON 当成同一种格式直接覆盖。从已有结果复用模板，使用标准模板目录交换，或在原任务入口继续处理。

页面成图默认保存到系统「图片」目录下的 `Dsivio/Images/日期_任务名__任务ID/`，可在图片设置中更换根目录，只影响新任务。原图在 `originals/商品名__商品ID/`，交付图在 `deliveries/导出批次/商品名__商品ID/`。每次生成和导出保留独立文件；任务改名不移动目录。

`studio tasks` 返回每个页面任务的 `outputDirectory`，这是实际的绝对目录。解析任务 JSON 中的路径时：

- `outputs/<任务ID>/<相对路径>`：取该任务的 `outputDirectory`，再拼接相对路径；不能直接拼接 workspace。
- `assets/...`、`templates/...` 和旧版 `results/...`：继续以 `studio paths` 的 workspace 为根。
- 旧任务可能没有 `outputDirectory`，下一次保存或出图时分配目录，旧图保留原位。

不要自行移动页面任务目录或改写记录中的路径。配置与任务记录留在应用数据目录，不混入交付文件夹。
