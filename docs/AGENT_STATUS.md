# Status for the next agent — YouPDF parity work

- **Date**: 2026-10-23 (continuing).
- **Rule**: continue the unfinished parity work; complete every task the subagents could not finish and check everything. Commit on `main`. Do not touch `docs/rea` (git-ignored, must stay out of commits).

## Where things stand

### Committed work (most recent first)
- `c221097` feat(app): twelve dynamic stamps, a custom stamp and a signature creator
- `219536a` feat(app): give the batch queue a table, progress, ETA and cancel
- `69e9225` feat(app): a window for the signing engine
- `fefa014` feat(app): confirm page deletes, and paste an image into the document

### UNCOMMITTED (must be committed before continuing)
- `crates/youpdf-app/src/fonts.rs` (new) — SF Pro font install with renderability pre-check.
- `crates/youpdf-app/src/main.rs` — `mod fonts;`.
- `crates/youpdf-app/src/app.rs` — call site now `crate::fonts::install(&cc.egui_ctx)`; old `install_fonts` removed.

### Verification commands (authoritative)
- `cargo check -p youpdf-app`
- `cargo clippy -p youpdf-app -p youpdf-core --all-targets` (clean)
- `cargo test -p youpdf-app` → ok. 69 passed
- `cargo test -p youpdf-core --test stamps` → ok. 13 passed (new)
- `cargo test --workspace` → all green

## Open parity tasks (docs/COMPLETE_PARITY_PLAN.md; 4 remaining)

1. **Task 4.2** — content element arrange: hit-test dispatch, resize/rotation handles, drag-to-move/scale, z-order. Foundation only so far.
2. **Task 5.5** — Stamps & Visual Signatures. **IN PROGRESS** (now largely landed this session; plan entry is stale).
3. **Task 12.2** — PDF comparison view modes.
4. **Task 14.2** — Full localization (i18n) wiring.

## Known gap: Task 5.5 now implemented

- New core types in `crates/youpdf-core/src/annotate.rs`:
  - `StampTemplate { id, label, pdf_name, color, dynamic }`.
  - `STAMP_TEMPLATES: &[StampTemplate]` — 12 entries with English labels (`Approved`, `Not Approved`, `Draft`, `Final`, `Confidential`, `Expired`, `For Comment`, `Sign Here`, `Initial Here`, `Witness`, `Reviewed`, `Void`).
  - `stamp_template(id)`, `stamp_id_for_pdf_name(name)`, `dynamic_stamp_text(template, author, date)`, const `STAMP_FONT_SIZE`.
  - `Annotation::stamp(page, rect, template_id, author, date) -> Annotation` (Stamp kind, dynamic content, author default).
  - `Annotation::is_dynamic_stamp()`, `Annotation::refresh_stamp(author, date)`.
  - `Annotation.stamp: Option<String>` field (serialized `#[serde(default)]`).
- Stamp appearance draws multi-line (label, author, date) in `appearance_content`.
- `/Name` written from the template id when present, else `stamp_name(&content)`.
- App: new module `crates/youpdf-app/src/app/marks_ui.rs` with `MarksUi` holding the Custom Stamp window (`stamp_window`) and Signature Creator (`signature_window`).
- App: stamp palette grid in preset popup (`tool == 8`) now iterates all `STAMP_TEMPLATES` with a Custom… row.
- App: `open_draft` for `DraftKind::Signature` opens the creator and records `marks.signature_at = Some((page, point))`.
- App: `place_signature_png` uses `marks.signature_at` when present else centres on the page.
- App: `ToolsEntry::{CreateSignature, CustomStamp}` added to the Tools menu.

## Open questions / follow-ups

- `egui_phosphor` icon font: verify `install_fonts` replacement keeps icons rendering (the agent added fonts.rs but must confirm the call).
- Stamps: the drawn-stamp stroke colour/width and the "Insert image" canvas path still use old colours.
- The two clippy fixes in marks_ui.rs (`for channel in 0..3` → `for (channel, over) in color.iter().enumerate()`, alpha formula) — verify they are applied (they were).

## Next step

Continue with Task 4.2 (content element arrange) next, then Task 12.2, then Task 14.2, then the force-push of the rewritten main (approved previously, not yet done).
