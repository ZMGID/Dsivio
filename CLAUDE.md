# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Read first

`AGENTS.md` is authoritative: before adding features, fixing bugs, changing call flow or reviewing architecture, read `docs/engineering-standards.md` (the single maintained rules file — don't create a parallel one). Domain vocabulary is in `CONTEXT.md`; read the relevant `docs/adr/` entries before touching a domain, and flag conflicts rather than silently overriding them. Docs and UI copy are primarily Chinese.

## Project

Dsivio (package name `dsivio`, formerly/also "Kivio") is a Tauri 2 desktop app (macOS + Windows): React 18 + Vite + TypeScript frontend in `src/`, Rust backend in `src-tauri/`. One window has two sidebar "forms" (形态): the **conversation** form (Dsivio Agent chat — the only built-in runtime) and the **workbench** form (e-commerce content production). Separate from the window, hotkeys drive translation, OCR and Lens (screen visual Q&A).

## Commands

```bash
npm install
npm run dev            # full desktop app (builds Swift sidecar on macOS first)
npm run dev:ui         # UI only, http://localhost:5713
npm run lint           # eslint, max-warnings 0
npm run typecheck      # runs protocol:check, then tsc --noEmit
npm run architecture:check
npm test               # vitest run
npx vitest run src/chat/ArtifactsCenter.test.tsx   # single test file
npx vitest run -t "name"                           # single test by name
cargo test --manifest-path src-tauri/Cargo.toml    # Rust tests (Windows: powershell -File scripts/win-cargo-test.ps1)
cargo test --manifest-path src-tauri/Cargo.toml <filter>
npm run protocol:generate   # regenerate TS chat-protocol types from Rust after changing the protocol
npm run build:video-runtime / verify:video-runtime   # bundled plugin deps (needs uv); required before `npm run build`
```

CI (macOS) runs, in order: video-runtime build/verify, icons:check, package:check + test:packaging, build:swift, lint, architecture:check, typecheck, test, cargo test. `src/generated/` is generated from Rust — never hand-edit; `protocol:check` catches drift.

## Architecture

**Dependency boundaries.** `architecture-boundaries.json` declares frontend modules (`chat`, `settings`, `lens`, `onboarding`, `api`, `components`, `data`, `utils`, …) and roles (composition / feature / adapter / shared-ui / foundation). `npm run architecture:check` enforces them: cross-feature imports must go through the target's `public/*` module (e.g. `src/chat/public/*`, `src/settings/public/*`, small contracts, not barrels), except at composition roots (`App.tsx`, `Lens.tsx`, `main.tsx`). Unmapped source paths fail. See `docs/ARCHITECTURE.md`.

**Coordinator gets no new features.** `src/chat/Chat.tsx` is the window-level coordinator with an ESLint `max-lines` ratchet in `.eslintrc.cjs` (the cap may only go down). New state goes to the owning pane / hook (`src/chat/hooks/`) / page component under `src/chat/<domain>/`; at most a route check plus a render branch lands in `Chat.tsx`. Sidebar-form state lives in `ChatSidebarPane` (ADR 0006), not `Chat.tsx`.

**Routing.** `src/chat/routeContract.json` is the shared route vocabulary consumed by both `routeCodec.ts` (TS) and Rust (persisted window restoration). Pages like images/video/automation open in both forms via the same routes.

**Workbench registry (ADR 0007).** `src/chat/workbench/registry.ts` is the sole authority for workbench features (`{id, group, label, icon, load}`); route types, sidebar, home and search derive from it. Adding a feature = page file + one registry row + two i18n labels.

**Headless AI (ADR 0008).** Workbench/automation AI calls go through one entry, `run_ai_task` (→ `run_agent_loop`, `mode: once | agent`), session id `wb_{taskId}`; no sidebar conversation created.

**Backend (`src-tauri/src/`).** `AppState` is the composition root over chat runtime/protocol/interaction state, provider runtime, Lens, automation, MCP and the settings persistence gate. Key areas: `chat/` (agent loop, sub_agent, storage, protocol, ask_user, goal), `workbench/`, `automation/`, `generation_workflow/`, `media_generation/`, `mcp/`, `skills/`, `plugins/`, `sourcing/`, `web_search/`, OCR (`macos_ocr.rs`, `windows_ocr.rs`, `rapidocr.rs` + Swift sidecar in `src-tauri/swift`). Persistent settings defaults/migration/normalization belong to the backend; the frontend only validates for input feedback.

**IPC.** The frontend must not add bare `invoke` calls or its own protocol decoding in pages — use the existing typed command adapters (`src/api/`, `src/chat/api.ts`). Same action from button/shortcut/other window should converge on one business entry.

**UI reuse (enforced by review).** Use global components/tokens: `Button`/`IconButton` from `src/components/Button.tsx`, controls via `src/settings/public/controls.ts`, `custom-scrollbar` for scroll containers (never set `scrollbar-width`/`scrollbar-color`), `kv-modal` for dialogs. Don't override a shared component's size/color/border/hover/focus in page CSS. Stylesheet composition root is `src/styles/app.css`; global tokens in `src/index.css`.

## Conventions worth knowing

- Bug fixes start with a failing regression test; for flows cover normal, failure, retry, concurrency/late results and leave-and-reopen.
- New abstraction layers must pass the "delete test" (see standards §3); no pass-through wrappers. Keep locks/CAS/atomic-replace/cancellation guards — they're correctness, not clutter.
- Imported CLI conversations are view-only snapshots, never resumable (ADR 0001/0002/0003). Subagents retain identity across tasks (ADR 0005). Sidebar group order is manual (ADR 0004).
- Desktop-flow changes need real-app verification; state explicitly if not done.
- `packages/` holds bundled plugin catalog/templates (`packages/catalog.json`); `website/` is the marketing site.
