---
name: hypit-setup
description: Check Dsivio integration and repair Hypit Skill, CLI, video tools, and provider setup on first use or when something is missing.
kivio-market-managed: true
---

# Hypit setup

The marketplace installs the Hypit Skill in `~/.kivio/skills/hypit`. On first use or when setup is requested, check what actually works, install missing runtime components using the current official instructions, and check again.

1. Check the Dsivio connection: the current chat can load `hypit` and `hypit-setup`, and `/hypit-market:check` is registered. Check that `~/.kivio/skills/hypit/SKILL.md` and its referenced files are readable. A file on disk alone does not prove the chat can use it. If a market-owned Skill or command is missing, use the marketplace install/repair action; do not copy it into `~/.agents/skills`. This package has no bundled MCP component, so do not create one just to mark setup complete.
2. Check whether the `hypit` executable is available in the environment used by the Dsivio chat, not only an unrelated terminal. If missing, read the current [official README](https://github.com/hypit-ai/hypit), perform its documented CLI installation, and verify the executable from the same environment. Keep the Skill in `~/.kivio/skills`.
3. Check video tools and provider configuration needed for the user's task. Install missing local dependencies when the official instructions identify them; ask for credentials or account authorization only when required. Run the documented health check or a small safe probe through the path the chat will use. Report Dsivio component registration and runtime readiness separately; marketplace “installed” means the packaged components are present, not that a provider task has succeeded.
