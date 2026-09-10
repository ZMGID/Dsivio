# UGC selection lost its intent — 2026-09-10

Reported task: `4638fd15-de41-43c8-b348-b99978ee71ed`.
Actual persisted inputs were UGC assistant, request `tk店铺展示视频`, backpack reference image, 5 seconds, speechMode auto, Grok 480p. No old template or selected concept. The persisted script explicitly prohibited people, hands and speech; the submitted prompt matched that script. The output was 480×848, about 5.04 seconds, with an audio stream. An audio stream does not establish spoken dialogue.

The selected assistant was correct. The shipped UGC prompt described UGC as an optional style, then appended common conservative rules about preserving scenes and not adding speech. This let the model choose a silent product-only shot for a short request. Prior live tests had explicitly supplied actors and dialogue and did not cover this default-intent case.

UGC now has its own cohesive prompt: selecting it establishes a presenter-and-speech intent for auto sound. A short request must yield an actual presenter action and a concrete short line in the selected language. Explicit no-person/no-speech requests and ambient/silent settings override the default. Five-second clips use one simple action and a short line; no speculative materials, capacities or convenience claims.

v11 migration only replaces the exact shipped v10 UGC prompt. User-customized prompts, models, installation/archive state and all other assistants are preserved. Tests cover both the upgrade and preservation cases.

`check_ugc.py` reproduced the host instruction and image request with the actual reported brief. Two rounds, three cases per round: original auto input, ambient mode, explicit no-people/no-speech. Initial revised outputs got the speaking intent right but guessed material/pocket benefits; the final prompt adds targeted evidence and brevity constraints. Final original-case output has a visible adult holding the backpack and saying “这款背包款式很日常，喜欢就看看。” The two override cases remain without speech. Outputs still contain some unnecessary specification/camera prose; these are prompt-level checks, not generated-video quality validation or end-to-end Tauri planning invocations.

20 video-related Rust tests passed, including migration regression, and the final targeted assistant tests/native build passed. The updated app was restarted and the saved UGC prompt compared equal to source; all 19 other assistants remained unchanged.

Created the unapproved `tk店铺展示视频 · 口播修正版` draft through the shared Python task service using the final model output. Original task hash is unchanged. No video-generation request was submitted; original output is retained. See `corrected-draft.json` and `runtime-check.json`.
