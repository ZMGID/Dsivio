# gen：一张或几张图

适用于普通生图、海报、换背景和少量改图，不建模板。

## 准备

- prompt 用英文写清主体、构图、光线、背景和文字；有参考图时先说明每张图的用途。
- 画幅按用户要求；壁纸但没说设备时只问横屏还是竖屏，其余情况默认 `1:1`。
- 分辨率默认 `1k`。用户明确要求 `2k` / `4k` 时才传 `--resolution`。
- 输出到本次对话工作目录的 `generated-images`，使用绝对路径。

## 执行

命令工具的 cwd 设为 Skill 末尾显示的 `Skill directory`，timeout 设为 `600000ms`：

```bash
python3 scripts/dsimage.py gen "<PROMPT>" --ratio <RATIO> --out "<ABS_OUTPUT_DIR>" --name <NAME> [--ref "<IMAGE>"] [--n <COUNT>] [--resolution 2k|4k]
```

Windows 使用 `python`。默认 1k 时省略 `--resolution`。

## 收口

1. 运行一条 `gen` 命令并等待结束。
2. 退出码为 0：确认命令打印的图片路径存在，打开检查，再把图片和路径交给用户。
3. 退出码非 0：把完整 stdout/stderr 返回给用户。
4. 工具超时或取消：说明请求状态未知以及恢复目录位置，本轮结束。

改图时从 `<out>/_dsimage/gen.jsonl` 取上一版 prompt，只改用户点名的部分，并换一个输出名。参考图顺序与 prompt 中的第一张、第二张一致。

同一产品需要 5 张以上或以后要重复使用时，改走 `design.md` 建模板。
