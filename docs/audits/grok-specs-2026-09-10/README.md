# Grok video dimensions and sound — 2026-09-10

## Evidence

Compared the bundled client with ZMGID/dsvideo-plugin HEAD
`33af713d96d7d5d171080e23a618ed4fb37a6d8e`. Both send the same
`POST /v1/videos/generations` fields: model, prompt, duration, aspect_ratio,
resolution, generate_audio and image.url. This was not a missing audio flag.
Current configured compatible gateway: ybw-ai.com; model: grok-imagine-video-1.5.

- Backpack task c938c028: selected 9:16 / 720p, actual 960×960 / 5.04s.
  AAC audio exists but mean volume is -53.9 dB. The generated prompt explicitly
  prohibited dialogue, narration and music.
- Reference remake 916efdae: script described 9:16 but brief selected 16:9 / 480p;
  actual output 544×544. Mean audio volume -59.0 dB. The prompt requested faint
  ambience and no speech/music. Saved reference templates discarded measured
  aspect ratio, and unavailable transcription was treated as a reason for no speech.
- Logo task d9a2cab4: selected 16:9 / 720p, actual 960×960; audible AAC (-19.4 dB).

## Changes

- Prepare single-image first frames on the selected canvas, fitting the product
  without distortion and filling remaining canvas white. Reference-image mode is
  unchanged. This targets gateways that continue to follow input dimensions despite
  aspect_ratio. No output resizing, upscaling or added audio disguises a bad response.
- Preserve explicit aspect_ratio and generate_audio in the submitted specification.
- Probe downloaded media before marking success. Report ratio, resolution, duration
  and missing-audio mismatches; preserve the returned file and remote job. Show
  measured properties in the preview. Audio-track presence is checked; semantic
  accuracy of spoken dialogue and near-silent audio are not automatically verified.
- Auto sound planning must design audible sound and must not silently add speech/music
  prohibitions. Prompt conversion preserves approved sound choices. Reference templates
  keep measured ratio and source audio metadata; a 20s reference is not silently assigned
  to a model with a 15s limit.

## Live regression

One paid request, existing configured gateway/model, no resubmission:
`fab6821f-adcb-94c1-a4cd-c4fb381c42db`.

Requested 3 seconds, 9:16, 720p, generate_audio=true, product image and Mandarin
speech. Prepared first-frame bytes and all non-image request fields were verified
identical to the new workspace request builder. Resumed the same job through the
workspace poll/download/verification implementation.

Result: **720×1280, 3.041667s, AAC stereo, mean -13.2 dB / peak -2.7 dB**.
Workspace verification: succeeded. Audio loudness was measured, not speech-transcribed.
Official reference estimate $0.43; gateway billing is authoritative.
Local evidence: ~/Pictures/Dsivio/Diagnostics/2026-09-10/grok-spec-fix/verification.json.

Regression coverage: source frame fit (720p portrait / 1080p landscape), original
image preservation, media mismatch detection, preserving nonconforming downloads,
explicit silent requests, reference-template metadata and shared video workflows.

Sources checked:
- https://docs.x.ai/developers/model-capabilities/video/generation
- https://docs.x.ai/developers/model-capabilities/video/image-to-video
- https://raw.githubusercontent.com/ZMGID/dsvideo-plugin/main/skills/grok-video-api/scripts/grok_video.py

The official contract supports configurable aspect ratio/resolution and audio by
default. Observed gateway behavior differs for square first frames. Only 720p portrait
was verified with a live paid request; 1080p landscape preparation was tested locally.

Validation: Python 33 tests, frontend 33 tests, Rust video service 12 tests passed.
TypeScript and scoped ESLint passed.
