# 公司 Skill 广场

这里存放经过维护者整理的内部技能包，每个子目录包含 SKILL.md 及其引用的脚本、模板和资源。
本目录随应用打包，但不会自动加入技能扫描。员工在 Skill 广场点击「加载」后才复制到个人技能目录并启用。

在 `src/chat/company-skills.json` 添加展示记录，例如：

```json
{
  "id": "weekly-report",
  "name": "周报整理",
  "description": "根据本周工作记录整理部门周报。",
  "category": "日常办公",
  "version": "1.0.0",
  "author": "内部团队",
  "details": "适用场景、需要准备的材料、产出说明和使用示例。支持 Markdown。",
  "directory": "weekly-report"
}
```

`id` 必须与后端解析 SKILL.md 得到的技能 ID 一致；`directory` 是本目录下的直接子目录名。
不要放入账号密钥、个人配置或未经确认可分发的材料。修改技能包或目录清单后，重新构建应用分发。
此清单仅由项目维护者编辑；不是员工自己的导入清单，也不接入公共商店。
