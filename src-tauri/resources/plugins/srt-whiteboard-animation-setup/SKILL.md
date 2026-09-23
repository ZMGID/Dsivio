---
name: srt-whiteboard-animation-setup
description: Check Dsivio integration and repair the installed SRT whiteboard Skill and its dedicated Python environment.
kivio-market-managed: true
---

# SRT whiteboard animation setup

The marketplace installs the Skill, scripts, and assets in `~/.kivio/skills/srt-whiteboard-animation`. Use this Skill on first use or for repair: inspect, install missing dependencies, then verify.

1. Check the Dsivio connection: the current chat can load `srt-whiteboard-animation` and `srt-whiteboard-animation-setup`, and `/srt-whiteboard-market:check` is registered. Confirm `SKILL.md`, `scripts/prepare_env.py`, `scripts/render_stream_whiteboard.py`, and `assets/preview.html` exist in `~/.kivio/skills/srt-whiteboard-animation`. If a market-owned component is missing, use the marketplace install/repair action; do not write to `~/.agents/skills`. This package does not ship an MCP server.
2. From the Skill directory in the environment used by the Dsivio chat, run `python scripts/prepare_env.py --check`. If requirements are missing, follow the [project README](https://github.com/geeklee/srt-whiteboard-animation) and run `python scripts/prepare_env.py` to prepare its dedicated environment, then check again.
3. Check that the chat can resolve the Skill's scripts and assets using its actual working directory; use paths anchored to the installed Skill rather than assuming the conversation cwd is the Skill directory. Report Dsivio component registration separately from Python/render readiness. Keep user SRT files, artwork, and exported videos outside the Skill directory.

After the Dsivio connection, `prepare_env.py --check`, and script/asset path checks succeed, append ` [setup completed once]` exactly once to the YAML `description` line in the installed `~/.kivio/skills/srt-whiteboard-animation-setup/SKILL.md`; leave the rest of the Skill unchanged. Do not mark an incomplete check. The marker records a past success only; rerun setup when requested or when the Python environment changes, without duplicating it.
