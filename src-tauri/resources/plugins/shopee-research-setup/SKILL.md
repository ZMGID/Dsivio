---
name: shopee-research-setup
description: 准备虾皮调研的浏览器、店铺窗口、虾多拉和出表环境，首次使用或环境缺失时加载。
kivio-market-managed: true
---

# 设置虾皮商品调研

主skill位于`~/.kivio/skills/shopee-research`，用户配置位于`~/.kivio/shopee-research/config.json`。首次按以下顺序准备，已有配置只补缺项。

1. 先让用户选择“紫鸟（推荐）”或“Chrome”、Shopee站点和结果目录。紫鸟让用户打开用于调研的店铺浏览器窗口；Chrome沿用用户日常窗口和配置。
2. 加载`cua-driver`，确认宿主的CUA工具能列出目标窗口。工具未连接、权限不足或服务版本冲突时，报告具体问题，引导用户在“设置 → 电脑控制”处理，恢复后复查一次。
3. 在选定的浏览器窗口准备虾多拉：必须安装、启用并登录。已有可用插件直接复用；缺失时引导用户安装或登录。确认Shopee可搜索，虾多拉能显示关联商品及销量、GMV，才算环境准备完成；密码、验证码和系统授权由用户处理。
4. 运行一次随附脚本准备出表环境：

   ```bash
   python3 ~/.kivio/skills/shopee-research/scripts/setup_runtime.py
   ```

   用户请求安装或补齐环境时，调用shell工具按其契约传`allow_host_python_package_install: true`。脚本在`~/.kivio/shopee-research/runtime/venv`安装公开的openpyxl、Pillow、lxml，已齐全时直接复用；输出JSON中的`python`用于出表，并保存至`runtime/runtime.json`。Windows用可用的Python 3命令运行同一脚本。缺Python时引导安装Python 3.9以上；脚本失败就报告实际错误，修复后重试。检查限定为CUA窗口、浏览器插件和这条环境脚本，不扩展成全盘搜索、源码调查或临时改写出表程序。
5. 保存`browser`（`ziniao`或`chrome`）、`browserContext`（店铺名称／用户配置标识）、`site`、`outputDir`、`shopdoraStatus: ready`及`environmentCheckedAt`，保留其他配置字段。只记录窗口标识，不记录密码或令牌。全部通过后，在本setup的description行尾追加一次` [setup completed once]`；有调研请求就加载主skill继续，否则简短告知准备完成。

已有环境只补缺项：旧`shopdoraStatus: skipped`需补齐为`ready`；登录失效时恢复登录；出表依赖失效时重跑环境脚本。用户更换浏览器时在新窗口完成同样准备，要求记住才更新默认值。
