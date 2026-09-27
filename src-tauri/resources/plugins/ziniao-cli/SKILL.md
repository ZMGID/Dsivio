---
name: ziniao-cli
description: Use the official Ziniao CLI and its domain Skills for stores, OpenAPI and Ziniao Browser tasks.
kivio-market-managed: true
---

# 紫鸟 CLI

This is the marketplace entry point. The official `ziniao-*` Skills installed by `ziniao-cli skills install --copy` own the domain commands and rules; read the matching Skill before acting. Start with `ziniao-shared` for shared conventions. Use `ziniao-store` and `ziniao-page` for the local Ziniao Browser, and `ziniao-openapi-explorer` when an endpoint is unclear.

On first use, check `ziniao-cli-setup`. If its description ends in ` [setup completed once]`, proceed with the requested task after a quick CLI availability check. Load setup when unmarked, the CLI or official Skills are missing, the user requests a recheck, or a command reports an auth/configuration problem. Fix that problem, then resume the original request.

The marketplace owns only this entry Skill and `ziniao-cli-setup` under `~/.kivio/skills`. Keep the official CLI, account configuration and official Skills in their original locations.
