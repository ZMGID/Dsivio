---
name: minimax-h3-api
description: Generate, monitor, recover, and download MiniMax-H3 videos through MiniMax's official Video Generation V2 API. Use for paid H3 text-to-video, first/last-frame video, or multimodal reference video; do not use for local ComfyUI generation or legacy Hailuo models.
---

# MiniMax H3 Official API

Use the bundled `scripts/minimax_h3.py` client. It has no third-party Python dependencies and fixes the model to `MiniMax-H3` on the official `/v2/video_generation` API.

Before a paid request, read [references/h3-api.md](references/h3-api.md) for credentials, input constraints, task recovery, and failure handling.

Before presenting the API route for a new video, run `quote` for both resolution estimates and `balance` for the current pay-as-you-go balance. These commands do not create a video task. The bundled quote is a CNY estimate for `cn` accounts only; never present it as a USD or `global` estimate.

## Required Rules

1. Preserve the current task’s agreed script, product, language and audio across follow-ups. Consolidate missing choices and cost confirmation into one question. A user instruction to generate the displayed plan or a specific variant is authorization for that variant; do not ask again for unchanged choices. Newly invented scripts or an unconfirmed paid cost require confirmation. Preparation and free dry runs may run before confirmation.
2. Use the Pay-as-you-go API key saved under `minimax` in the current user's `dsvideo/providers.json`; the legacy `MINIMAX_API_KEY` variable remains an override. Never print, repeat, or place a literal key in a command.
3. Select `global` or `cn` with `MINIMAX_REGION`. This plugin defaults to `cn`, using the official `https://api.minimaxi.com` endpoint; use `global` only when the user explicitly has an international API account.
4. Obtain an explicit `768P` or `2K` choice from the user for the current task. Never infer, default, upgrade, or downgrade the resolution. Reuse the user’s existing choice unless they change it.
5. Show the current balance with the currency returned by `balance` and the estimated cost before asking the user to choose the API route. If balance lookup fails, report that it is unavailable and still show the estimate. For `global`, stop before paid creation unless a reliable current international estimate is available; never reuse the bundled CNY quote. Never treat an estimate as the final charge.
6. Run `--dry-run` first and check `model`, `resolution`, `duration`, `ratio`, and mode against the request. The paid command prints the same billable request summary immediately before its single POST.
7. Run exactly one `generate` command when the user wants a completed file. The script submits once, prints the task ID immediately, polls that task, verifies the returned resolution and duration, and then downloads its result.
8. If the terminal remains active, wait on that exact execution session. Do not submit another task.
9. After a timeout or interruption, recover with `status` or `wait` and the existing task ID. Pass the originally requested resolution and duration to `wait` so the result contract is verified before download. Never create a replacement merely because polling or downloading stopped.
10. Use `submit` only when the user explicitly wants an asynchronous task ID without waiting for a file.
11. A dry run does not need credentials and cannot incur a charge, but it is not proof that the API key works or that a video was generated.

## Resolve The Client

Resolve this skill's directory, then invoke its bundled script with an available Python 3 interpreter:

```text
python <skill-directory>/scripts/minimax_h3.py --help
```

Do not install `mmx-cli`, Node packages, or Python packages for this client.

## Completed Video

```text
python <skill-directory>/scripts/minimax_h3.py --region <global-or-cn> generate \
  --prompt "<video prompt>" \
  --resolution <768P-or-2K> \
  --duration <4-15> \
  --ratio <ratio> \
  --output <output.mp4> \
  --poll-interval 10 \
  --timeout 1800
```

For frame-based generation, add `--first-frame`, `--last-frame`, or both. For reference generation, repeat `--reference-image`, `--reference-video`, and `--reference-audio` once per input. Frame inputs and reference inputs cannot be mixed.

## Balance And Estimate

```text
python <skill-directory>/scripts/minimax_h3.py balance
python <skill-directory>/scripts/minimax_h3.py quote --duration <4-15> --reference-image-count <count> --reference-video-seconds <total-seconds>
```

`quote` returns both `768P` and `2K` estimates in CNY from the pricing snapshot documented in `references/h3-api.md`. It makes no network request and creates no task. `balance` performs only the official read-only account balance request and requires the saved MiniMax key or the legacy `MINIMAX_API_KEY` override.

The client converts supported local files to Data URIs. Public `http(s)` URLs, existing Data URIs, and `mm_file://` IDs are passed through. Use URLs or file IDs when Base64 would exceed the 64 MB request limit.

## Existing Tasks

```text
python <skill-directory>/scripts/minimax_h3.py status <task-id>
python <skill-directory>/scripts/minimax_h3.py wait <task-id> --expect-resolution <768P-or-2K> --expect-duration <4-15> --output <output.mp4>
python <skill-directory>/scripts/minimax_h3.py list --page 1 --page-size 20 --status succeeded --model MiniMax-H3
```

V2 tasks remain queryable through the list endpoint for seven days. Retain the task ID in the user-visible result whenever the requested outcome is not yet downloaded and verified.

## Delivery

For a completed request, verify that the reported local MP4 exists and is non-empty, then return its path, task ID, input mode, verified resolution, verified duration, ratio, and that the user selected the paid API route. Do not describe submission or a running task as a completed video.

## Dsivio 内置版

对话使用本 Skill 的原始脚本和 MCP。查找、保存或修改模板时读 [共享模板说明](../../TEMPLATES.md)，直接读写共享文件夹；不创建页面任务或同步草稿。运行环境由应用提供，内置副本随应用更新。

## 视频配置在哪

视频页面和本插件共用一份 `providers.json`，页面保存后下次脚本调用直接生效，不需要重复配置。路径优先取 `DSVIDEO_CONFIG_PATH`；否则 Windows 为 `%APPDATA%/dsvideo/providers.json`，macOS/Linux 为 `${XDG_CONFIG_HOME:-~/.config}/dsvideo/providers.json`。
用户问模型或配置时，运行 `python <插件根目录>/scripts/dsvideo_config.py show`（密钥脱敏），查看 `providers.grok` / `providers.minimax` / `providers.comfy` 的 `model` 和 `base_url`。MiniMax-H3 路线模型固定为 MiniMax-H3；ComfyUI 模型由工作流决定。旧环境变量显式覆盖时说明来源。生成仍使用插件自己的脚本和 MCP。

## Execution context

Read [运行与恢复](../ecom-h3-video/references/execution.md) before running commands. Keep requested speech and its language when switching routes or shortening a video. Official price estimates are not a custom gateway invoice.
