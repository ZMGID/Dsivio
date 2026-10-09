---
id: pdf
name: pdf
description: Read, extract, summarize, and analyze PDF attachments saved by Kivio Chat.
recommended-tools:
  - read
  - bash
---

# PDF Skill

Use this skill when the user attaches or references a PDF and asks to read, summarize, extract, compare, translate, inspect, or answer questions about it.

## Inputs

Kivio stores each uploaded file as a safe local copy and includes its absolute path in the user message under `Kivio 安全副本路径`. Use that host path with `bash`. You can also process any local PDF discovered via `glob` / `read` (directory listing).

`read` does not parse binary PDFs.

## Workflow

1. Identify the PDF path from the attachment note or a local search.
2. Extract text with Dsivio's bundled Python 3.12, which ships pypdf (no system Python or `pip install` needed): `write` a script, then run it with `dsivio python <script>.py` in `bash` — do not cram quoted code into `python -c`. `pdftotext` or OfficeCLI also work when installed.
3. If `dsivio python` reports that the bundled runtime is missing (exit code 127) and no other PDF tool is installed, say so and stop guessing. Do not invent document contents from the filename.
4. If extraction returns little text, the PDF may be scanned/image-only — ask for OCR or use Lens.
5. For long PDFs, extract page-level text first, then summarize by section/page before answering.

## Output

- For summaries: include the main points and mention any pages/sections you used when available.
- For extraction: preserve original order and tables/lists as much as practical.
- For analysis: quote only short snippets and explain conclusions separately.
