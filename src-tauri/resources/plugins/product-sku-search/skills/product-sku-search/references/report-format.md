# 商品搜款表

一份XLSX、一个工作表：标题、简短摘要、一段分析、商品表。白底细边框、换行居中、图片统一大小，链接可点击。直接生成文件。

摘要写目标商品、平台站点、采集日期、搜索范围；分析写匹配情况、价位、套装差异和选款判断。字段按需求精简，默认`source/name/image/sku/features/package/color/price/rating/reviews/match/url`。评价缺失留空；销量仅有真实可见数据时选`sales`列并填写`salesPeriod`。

任务目录的`report.json`示例（示意数据，正式任务用当前观察值）：

```json
{
  "title": "电动化妆刷清洗机搜款",
  "summary": "Amazon美国站；2026-10-08；按原图匹配，已核对1款。",
  "analysis": "透明杯身、灰色网盖和金色圈相近，按外观同款记录。",
  "currency": "USD",
  "columns": ["name", "image", "sku", "features", "package", "color", "price", "rating", "reviews", "match", "url"],
  "candidates": [{
    "id": "B0D4VX3RF2", "sku": "B0D4VX3RF2；White",
    "name": "CINGALOOK电动洗刷机", "features": "透明杯身；网格盖；电动清洗",
    "qty": 1, "unit": "台", "salesUnit": "件", "color": "白色",
    "price": 11.00, "rating": 3.9, "reviews": 51,
    "match": "外观同款；目标图未确认品牌",
    "url": "https://www.amazon.com/dp/B0D4VX3RF2", "image": "B0D4VX3RF2.png"
  }]
}
```

文件夹任务每行用`source`保留原文件名／货号，`sourcePath`保留相对输入目录的原路径（仅写本地JSON）；同一平台候选对应多个目标图时分别记录，行`id`加目标货号保证唯一。

`id`唯一且仅用英文字母、数字、下划线、连字符；实际SKU／变体写`sku`。`qty`是所选价格包含的基础商品数量，`unit`是台／件／双等，`salesUnit`是件／套等；套装内容可加`packContents`。商品图出现多件不等于套装数量，以所选SKU和描述判断。金额存数字，评价数存整数；仅展示“1k”等约数时存入本地`reviewsDisplay`，不转换为精确评价数。图片路径相对任务目录或使用绝对路径。

可选字段还包括`size/material/platform/seller/unitPrice/sales`，`unitPrice`由脚本按价格除数量计算。跨币种分开出表。缺厘米尺寸直接省略，图文可判断的功能、颜色、材质写实用结论；关键价格、数量、链接和图片必须完整。

在主skill目录执行：

```bash
"<config.json中的reportPython>" scripts/build_report.py <任务目录>/report.json <结果文件>.xlsx
```

脚本压缩图片为320×320 JPEG、每张最多40KB，同图复用；固定156×156图框并随单元格移动，保护图片对象、文字数据可编辑。这是标准锚定图片。检查最终XLSX图片数量、位置、超链接和价格后替换结果文件，失败保留原成品。状态与检查结果写`progress.json.report`。

查看生成的`report-layout.html`复核图文排版，再交付Excel；预览不等于Excel／WPS软件实测。内部数据留在任务目录，不添加原始样本页。
