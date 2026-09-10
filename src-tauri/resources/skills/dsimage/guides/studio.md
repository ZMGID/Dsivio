# 聊天与图片页面共用

## 内置聊天的任务入口（优先于后续脚本说明）

有 `studio` 工具时，所有任务操作使用 `{domain:"image", action, input}`，与页面同一执行服务；不要直接写 tasks JSON 或执行原脚本 gen/run。原来的流程选择和质量要求继续适用。

1. `bootstrap`，input `{}`：获得同一套模板、配置、任务。用户指定已有任务时 `get`，input `{id}`，沿用其 id/revision，不创建副本。
2. 接续页面未保存内容：`draft_get`，input `{entry:"main"}`。返回 `{revision,value}`；value 包含 brief、taskId、revision、plans。草稿 revision 与任务 revision 是独立版本号。先读取最新任务，再合并用户明确要求的修改，不能拿旧草稿覆盖新任务。
3. 素材导入：`import`，input `{paths:[绝对路径],asProducts:true}`，返回 products。无需让用户标正反面。没有商品图时 products 可为空。
4. `save`，input `{brief, id?, revision?}`。brief 必须完整：`{feature:"gen",name:"任务名",requirement:"用户要求",language:"zh-CN",platform:"",ratio:"1:1",resolution:"1k",count:1,style:"",templateId:null,products:[]}`。feature 还可用 replace/smart/design/client/workflow；套图从 bootstrap 选择 templateId。工作流额外参数按现有任务的 brief 保留。
5. 普通出图与页面一致，用 `action`，input `{id,revision,action:{kind:"start"}}` 自动整理素材、规划并开始单张或样张生成；用户已明确要求出图时，不额外增加方案确认。用户只要求规划时用 kind plan，等待完成后展示 plans。`save_plans` input `{id,revision,plans}` 可修改现有方案；单张后续生成用 kind generate，套图先 sample，验收后 approve/bulk（group 取商品分组）。返回 running 后立即调用 `wait`，input `{id,timeoutMs:60000}`。它在当前步骤完成或失败时立即返回，不需要 sleep；wait.state=timeout 时仍在处理，再次 wait 即可，不重新提交生成。沿用页面的样张关卡，不跳过。
6. 成图和错误均从 `wait` / `get` 返回的 results/status 读取，保留同一任务 id。不要从聊天记录推测完成。
7. `draft_save` input `{entry:"main",revision:草稿版本,value:{brief,taskId:任务id,revision:任务版本,plans:null}}` 可以把当前任务交给页面。版本冲突时重新读取并合并，不自动覆盖。
8. 模型配置使用 `config`，input `{config:完整配置对象}`，先从 bootstrap 读取后修改；图片凭据仍由应用供应商设置管理。模板使用 template_import `{path}` / template_save `{template}` 或下面的共享标准目录。禁止新建第二套配置。

普通单图直接采用用户原始要求和参考图，不再经过第二个模型自动扩写；需要润色时由用户主动使用“优化要求”。套图仍按模板生成方案。图片要求按用户原意整理，参考图须区分商品、版式、风格和待修改原图。不要逐项猜写复杂图案，不把商品旁边的道具自动带入成图。单图默认无新增文字；选择语言不代表要求文案。只换背景或局部改图时保留其余内容，拍摄指南中的人物、道具、价格、卖点只是可选示例。

旧脚本生成的 `_dsimage/batch.json` 是历史批次，不能假装是页面任务。新内置聊天全部走以上服务；历史批次仍可在原脚本中继续，模板可以复用。


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
