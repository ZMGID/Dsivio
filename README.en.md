# Dsivio

Dsivio is a desktop app for macOS and Windows. One window has two modes, switched from the top of the sidebar:

- **Dsivio**: talk to Dsivio Agent. Conversations live under Recent, Sets, or Projects. The agent can use tools, sub-agents, Skills, MCP, and a knowledge base.
- **Workbench**: e-commerce content in one navigation, from sourcing and shops through copy, images, and video, to publishing and the asset library.

Outside the window, hotkeys translate what you type or select, OCR a screen region, and open Lens to ask about what's on screen. You bring your own model keys. Conversations stay on this machine.

## Chat

Dsivio Agent is the built-in runtime for chat, tools, and sub-agents. A conversation belongs to a set or a project, not both. A project has a working directory the agent uses for files. A set holds a persona and a default assistant, and has no directory of its own.

A dock on the right opens files, Git, a terminal, and tasks. Settings hold models, Skills, MCP, the knowledge base, and local tools.

## Workbench

Switching mode changes the left navigation. Image and video pages open from either mode.

| Group | What it covers |
| --- | --- |
| Commerce | Shop accounts, overview, product files, listing, listing check, workflows |
| Sourcing | Lookalike search, pick library |
| Copy | Graphic posts, seed articles |
| Images | Main image, detail pages, posters, retouch, outfit change, template sets, free generation |
| Video | Shorts, avatar, drama, clone, edit, subtitles, analysis |
| Publish | Publish, accounts, logs, stats |
| Library | Image templates, video templates, roles, assets |
| Stats | Usage |

## Development

You need Node.js, npm, Rust, and the [Tauri 2](https://tauri.app/) dependencies for your OS. macOS also needs Swift for the OCR sidecar.

```bash
npm install
npm run dev          # desktop window; builds the Swift sidecar on macOS
npm run dev:ui       # UI only, http://localhost:5713
npm run lint
npm run typecheck
npm test
```

Rust tests:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

On Windows, use `powershell -File scripts/win-cargo-test.ps1`. Chat protocol TypeScript types are generated from Rust. After a protocol change, run `npm run protocol:generate`, then `npm run protocol:check`.

Conventions: [docs/engineering-standards.md](docs/engineering-standards.md). Domain language: [CONTEXT.md](CONTEXT.md).

## License

[GPL-3.0-or-later](LICENSE)
