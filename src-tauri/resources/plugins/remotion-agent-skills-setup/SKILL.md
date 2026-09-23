---
name: remotion-agent-skills-setup
description: Check Dsivio integration and repair installed Remotion Skills plus the current project's Node.js and Remotion environment.
kivio-market-managed: true
---

# Remotion Agent Skills setup

The marketplace installs the official Remotion Agent Skills in `~/.kivio/skills`. Use this Skill on first use or when setup is requested to check the current machine and project, fill in missing dependencies, and check again.

1. Check the Dsivio connection: the current chat can load `remotion-agent-skills-setup`, `remotion-best-practices` and the companion Remotion Skills, and `/remotion-market:check` is registered. Confirm their files are readable in `~/.kivio/skills`; a directory alone does not prove chat discovery. If a market-owned component is missing, use the marketplace install/repair action. No MCP component ships with this package; do not add an empty server configuration.
2. Check Node.js and the Remotion dependencies in the project directory used by this Dsivio conversation. Read the current [Remotion Skills README](https://github.com/remotion-dev/skills) and relevant Remotion documentation; install only missing project dependencies using that project's package manager. If there is no project yet, prepare one only when the user's requested task needs it. Keep the Skills in `~/.kivio/skills`, not `~/.agents/skills`.
3. Run a small project-specific check through the same working directory and tools the chat will use. Report Dsivio component registration separately from project runtime readiness; marketplace “installed” does not prove a render succeeds.

After the Dsivio connection and the current project's check succeed, append ` [setup completed once]` exactly once to the YAML `description` line in the installed `~/.kivio/skills/remotion-agent-skills-setup/SKILL.md`; leave the rest of the Skill unchanged. Do not mark an incomplete check. The marker records a past success, not readiness for every project; rerun setup when requested or when the project or environment changes, without duplicating it.
