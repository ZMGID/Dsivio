---
name: davinci-resolve
description: Edit the open DaVinci Resolve Studio project through its official MCP server. Organize media, mark timelines, and queue renders. Do not grade by taste or invent unsupported actions.
kivio-market-managed: true
---

# DaVinci Resolve

This is the marketplace entry for DaVinci Resolve Studio 21.1's official MCP server. Resolve remains the application that edits. Dsivio only calls the tools that server exposes.

1. On first use, check the advertised description of `davinci-resolve-setup`. If it has ` [setup completed once]`, continue to the requested task without routinely loading setup again. Load setup if unmarked, Resolve is not running, the MCP server is missing, the user asks for a recheck, or a later call fails.
2. Keep Resolve Studio open with the intended project loaded. Name timelines, bins, and clips explicitly. "This timeline" is ambiguous.
3. For anything that changes the project, say what will change and wait for confirmation. Work on a duplicated timeline unless the user names the original. Do not render, delete, or overwrite delivery files until they confirm the target.
4. Use only tools the connected Resolve MCP server actually lists. Do not invent scripting calls. A failed or missing tool stops the task; do not fall back to an unofficial bridge.

The marketplace owns this entry Skill and `davinci-resolve-setup` under `~/.kivio/skills`. Do not install another copy into `~/.agents/skills`.
