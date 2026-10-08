---
id: docx
name: docx
description: Read, summarize, revise, and analyze Word DOC/DOCX attachments saved by Kivio Chat.
recommended-tools:
  - read
  - bash
---

# DOCX Skill

Use this skill when the user attaches or references a Word document (`.doc` or `.docx`) and asks to read, summarize, revise, extract, compare, translate, or answer questions about it.

## Inputs

Kivio stores each uploaded document as a safe local copy and includes its absolute path in the user message under `Kivio 安全副本路径`. Use that host path with `bash`. You can also process any local Word document discovered via `glob` / `read` (directory listing).

`read` does not parse binary Word files.

## Workflow

1. Identify the safe copy path from the attachment note.
2. Extract text with Dsivio's bundled Python 3.12, which ships python-docx and lxml (no system Python or `pip install` needed): `write` a script, then run it with `dsivio python <script>.py` in `bash`. OfficeCLI also works when installed.
3. A `.docx` is a zip of XML; without python-docx you can unzip `word/document.xml` and collect `w:t` nodes. Legacy `.doc` usually needs conversion to `.docx` (python-docx does not read it).
4. If `dsivio python` reports that the bundled runtime is missing (exit code 127) and no other extraction tool is installed, say so. Do not invent content that was not extracted.

## Output

- For summaries: group by headings when possible.
- For edits: state what changed and provide replacement text or a concise revision plan.
- For extraction: keep document order and mark unclear formatting honestly.
