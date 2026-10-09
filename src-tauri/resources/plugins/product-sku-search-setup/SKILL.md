---
name: product-sku-search-setup
description: 首次准备商品搜款的现有浏览器、平台关键词搜索和Excel保存环境，已有配置只补缺项。
kivio-market-managed: true
---

# 设置商品搜款

主skill在`~/.kivio/skills/product-sku-search`，配置在`~/.kivio/product-sku-search/config.json`。已有可用环境直接复用。

1. 让用户选择当前紫鸟（推荐）或Chrome窗口、默认平台站点和结果保存目录；本次已指定的直接采用，默认虾皮巴西站。使用已可用的CUA或紫鸟CLI通道；CUA加载`cua-driver`并确认窗口，紫鸟CLI加载对应紫鸟skill并确认店铺。CUA缺工具或权限时到“设置 → 电脑控制”补齐。
2. 确认目标站点能搜索并查看商品；虾皮已有虾多拉时检查登录与数据展示。默认识图后用关键词搜索。用户选择AiPrice图搜时才检查扩展、登录和实际支持的目标平台。登录、验证码和系统授权由用户操作。
3. 准备出表Python：配置已有`reportPython`先验证`import openpyxl,PIL,lxml`；其次读取`~/.kivio/shopee-research/runtime/runtime.json`中的`python`并验证，成功直接复用。均不可用才用Dsivio内置Python执行（bash和PowerShell通用）：

   ```bash
   dsivio python "$HOME/.kivio/skills/product-sku-search/scripts/setup_runtime.py" --root "$HOME/.kivio/product-sku-search/runtime"
   ```

   `dsivio python`是随Dsivio安装的Python 3.12，已带openpyxl、Pillow、lxml，不需要系统Python、pip或联网；脚本直接记录该解释器。仅当`dsivio python`报告内置运行时缺失（退出码127）时，才用系统Python 3.9以上（Windows用可用Python 3命令）运行同一脚本：用户请求安装或补环境时，shell工具按契约传`allow_host_python_package_install: true`，脚本只在私有环境安装openpyxl、Pillow、lxml；缺Python时建议先修复或重装Dsivio，再引导安装Python 3.9以上。失败报告实际错误。
4. 保存`accessMethod`（cua／ziniao-cli）、`browser`、`browserContext`（窗口／配置标识）、`platform`、`site`、`outputDir`、`reportPython`及`environmentCheckedAt`，选用AiPrice且检查通过时加`aipriceStatus: ready`。保留其他配置，不保存密码、令牌。全部通过后在本setup的description行尾追加一次` [setup completed once]`；有任务则加载主skill继续。

以后直接读取配置启动，环境失效只补对应缺项；用户临时指定的站点和目录仅用于本次，要求记住时再改默认值。
