# Video prompt assistants — 2026-09-10

Implemented the video-generation assistant group and three editable built-in personas: 通用视频 (default), 电商产品展示, 电商 UGC 口播. The old fixed 15-second/four-shot persona is replaced by the default video assistant, retaining its stable ID.

Planning and revisions read the selected saved assistant prompt and model. Empty model settings inherit the current chat model. The host adds a small JSON/output contract and reference/parameter boundaries, not the old director skill or an additional style preset. Ordinary reference images do not become first frames; explicit modes label first/last frames without duplicating image inputs.

The video picker filters the video group, updates on assistant changes, and remembers the last explicit choice separately from task content. Custom assistants can be assigned to the group in the existing editor. Missing nondefault assistants require reselection for planning; direct original-prompt generation still works. All routes preserve the approved prompt through the host prepare operation without another model rewrite.

v10 only migrates the three video IDs. Unrelated assistants and the old video assistant's provider/model, installation/archive state, and creation time are preserved.

Verification:

- Frontend: 69 tests across the video components, preferences, validation, progress and assistant categories. Includes selection → plan → revision → remount and original-prompt bypass with a deleted assistant ID.
- Rust: 32 assistant storage tests and 19 video-related tests (one overlapping migration test).
- Python: 42 video workspace/service tests.
- TypeScript check, scoped ESLint, frontend production build, native debug build and diff whitespace check passed.
- No browser or Computer Use debugging. Component tests are not a screenshot/visual audit.
- `check_planning.py` makes three planning-only API calls with current saved prompts and instructions extracted from Rust. It does not submit media jobs. Its request construction is a controlled reproduction of the host input, not an end-to-end Tauri invocation.


Live checks completed after a single prompt refinement:

- All three final calls returned structured scripts for the supplied 8-second briefs. UGC preserved the supplied Mandarin dialogue verbatim.
- Initial outputs (`*-initial.json`) revealed speculative lighting details, repeated parameters and contradictory fixed/pushing camera language. The shared assistant instructions were tightened before the final calls. Final samples removed the speculative light-emitting base and the contradictory camera wording.
- Final outputs still sometimes repeat specifications or use more sections/negative constraints than desired. These small samples establish working prompts and improved adherence, not guaranteed model compliance or generated-video quality. Users can edit the assistants/model or use the original-prompt path.
- Six total text-model requests across two rounds, zero video-generation requests. Existing media tasks were not changed.
- Local app restarted with the final migration. The three video assistants are present, and all 17 unrelated assistant records compare equal to the pre-migration snapshot. Vite serves the new selector; startup log has no panic or assistant migration failure.
