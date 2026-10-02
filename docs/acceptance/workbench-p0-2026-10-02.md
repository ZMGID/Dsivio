# P0 real-app acceptance log (2026-10-02)

Build: baseline worktree `../Dsivio-p0` @04ac0bf0, `tauri dev` debug, real settings (image pool gpt-image-2.5-flare / grok-imagine-image-2.0, video grok-imagine-video-1.5, agent model gpt-6-luna).
Driver: macOS CGEvent clicks + screencapture against the real window.

| Page | Result | Evidence |
|---|---|---|
| 主图生成 main | PASS: submit → running → succeeded, 1 PNG, origin workbench/main; leave + reopen keeps history | media-tasks/9432721d… |
| 套图设计 set-design | Provider chain PASS: 3 images succeeded; state survives navigation. Baseline confirmation gate FAIL: start generated before confirmation (defect #4); updated gate awaits real-App retest | image-studio/tasks d8af3eb0… |
| 海报封面 poster | PASS succeeded | origin workbench/poster |
| 产品精修 retouch | PASS | workbench/retouch |
| 图片编辑 edit | PASS | workbench/edit |
| 万物迁移 migrate | PASS | workbench/migrate |
| 一键换装 dress | PASS | workbench/dress |
| 短视频生成 shorts | Generation PASS after fix #3; 1 real MP4. Preview race remains under repair, not a complete UI PASS | media-tasks/74acab7f-bc55-4cc5-b9cb-e547691fb216/output.mp4 |
| 自由生图 free-image | Generation PASS: 1 PNG, no reference required | media-tasks/05df4ae0-24d6-4d57-aab2-b9664557a101; /tmp/p0_43.png |
| 图片复刻 clone | Generation PASS: source layout + product image → 1 PNG visible in results; confirmation gate awaits retest | /tmp/p0_50.png |

## Defects found and fixed
1. Image project "Agent 模型" picker listed image-generation models (gpt-image-2.5-flare, grok-imagine-image-2.0); picking one fails planning. Fix: `studioModels.ts isVisionModel` excludes image-generation models (backend already rejects them). Test: studioModels.test.ts.
2. Image project form: the 「还可以把图片继续拖进来」 drop bar overflowed the materials column and overlapped 「更多设置」. Fix: `.if-materials` is a flex column; upload area flexes (imageFlow.css). Verified visually.
3. Short video submit: `MODEL_ARGUMENT_UNSUPPORTED: referenceAudios violates declared` — video_projects always sent `referenceVideos: []` / `referenceAudios: []`, violating ADR 0010 (absent optional media lists must be omitted). Fix in `video_projects/mod.rs generation_request`; test `absent_reference_media_lists_are_omitted_not_empty`.
4. Multi-step image `start` performed paid material preparation and sample generation before plan confirmation. Main now plans every product without paid material preparation; explicit sample/generate actions prepare required references and preserve edited prompts. UI opens the editable plan stage and exposes a confirmation button. UI regression passed; Rust reference-binding regression passed. Real-App retest pending.

## Environment notes
- The `claude-sonnet-5` channel on the Dsivio relay returned 503 model_not_found during planning; error surfaced correctly in the page ("本步骤未完成，已有结果已保存") and retry with another model succeeded.
