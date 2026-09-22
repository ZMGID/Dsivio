---
name: feishu-cli
description: Work with Feishu messages, documents, Base, sheets, calendar, tasks and other services through the official lark-cli and its embedded domain guidance.
kivio-market-managed: true
---

# 飞书 CLI

This is the marketplace entry point for Feishu. The official `lark-cli` binary embeds its `lark-*` domain Skills; retrieve them with `lark-cli skills list` and `lark-cli skills read` so guidance matches the installed CLI version.

1. On first use of the selected profile, load `feishu-cli-setup` to check and reuse the existing CLI, app and identity. If this conversation has already verified them, continue directly. If a later command reports a setup or permission problem, load setup again, repair only that problem, then resume the original request.
2. Match the user's request to the relevant embedded `lark-*` Skill and read it with `lark-cli skills read <name>` before using a domain command. Common routes: messages → `lark-im`; documents → `lark-doc`; cloud files → `lark-drive`; Base → `lark-base`; spreadsheets → `lark-sheets`; calendar → `lark-calendar`; tasks → `lark-task`; meetings → `lark-meeting`. Read references only as the selected Skill directs, using `lark-cli skills read <name> <relative-path>`.
3. Follow that Skill's directions for `lark-shared`, identity, permissions, and command syntax. Do not assume that a successful login grants every scope or administrator access.

The marketplace owns this entry Skill and `feishu-cli-setup` under `~/.kivio/skills`. Do not install another copy into `~/.agents/skills`.
