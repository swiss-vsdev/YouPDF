# Status — YouPDF

- **Last commit**: `c37981a` feat(compare): overlay blend view and per-page difference highlights (Task 12.2)
- **Branch**: `main` (clean, 13 commits ahead of `origin/main`)

## Verification Commands (authoritative)

```bash
cargo check --workspace --all-targets
cargo clippy -p youpdf-app -p youpdf-core --all-targets
cargo test --workspace
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

## Open Parity Tasks (see `docs/GAP.md`)

None — all three tracked gaps (4.2, 12.2, 14.2) are closed.

## Done

- **Docs consolidation** (`c490b43`) — nine overlapping planning files folded into `PLAN.md`, `GAP.md`, `STATUS.md`.
- **Task 14.2 — i18n** (`f1a6f64`) — all user-facing strings now go through `t()`; 365-entry EN/FR dictionary; coverage gate in `scripts/i18n_extract.py`.
- **Task 12.2 — Compare view modes** (`98c6a9c`, `c37981a`) — side-by-side view with shared page navigation; ignore-whitespace/case options; `compare_with_options()` in the engine; overlay blend with an opacity slider; per-page difference highlights (amber frame + counter) fed by the report.
- **Task 4.2 — Content element arrange** (`45d48f2`) — drag-to-move for images (`commit_image_move`) and text blocks (`commit_text_block_move`); `set_image_z_order()` in the engine; Bring to front / Send to back in the image inspector.
- **Task 4.2 — Canvas wiring** (`2c8888d`) — both drag gestures wired into the ScrollArea canvas using the file's take-and-commit-locally pattern; `commit_image_move` body repaired (was a single line of literal `\n` escapes).
- **Task 4.2 — Completed (`e21c6f6`)** — canvas drag-to-move wiring, corner resize handles and a free-angle rotation grip (`rotate_image` in the engine: the drawing matrix turns around the box centre, pixels never re-encoded). The engine test `rotation_libre_fait_tourner_autour_du_centre` renders a 45° turn and checks the halves swap vertically.

## Notes

- The canvas code cannot call `self` while it borrows `renderer`/`tabs`: take state out before the `show_viewport` closure, commit into `pending_*` locals, write after the closure.