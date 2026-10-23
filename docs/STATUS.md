# Status — YouPDF

- **Last commit**: `e21c6f6` feat(edit): free-angle rotation handle for images (Task 4.2)
- **Branch**: `main` (clean, 10 commits ahead of `origin/main`)

## Verification Commands (authoritative)

```bash
cargo check --workspace --all-targets
cargo clippy -p youpdf-app -p youpdf-core --all-targets
cargo test --workspace
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

## Open Parity Tasks (see `docs/GAP.md`)

1. **Task 12.2** — Compare view modes: side-by-side with shared page
   navigation is done; synchronized scroll, overlay blend and difference
   highlights remain.

## Done

- **Docs consolidation** (`c490b43`) — nine overlapping planning files folded into `PLAN.md`, `GAP.md`, `STATUS.md`.
- **Task 14.2 — i18n** (`f1a6f64`) — all user-facing strings now go through `t()`; 363-entry EN/FR dictionary; coverage gate in `scripts/i18n_extract.py`.
- **Task 12.2 — Compare view modes** (`98c6a9c`) — side-by-side view with shared page navigation; ignore-whitespace/case options; `compare_with_options()` in the engine.
- **Task 4.2 — Content element arrange** (`45d48f2`) — drag-to-move for images (`commit_image_move`) and text blocks (`commit_text_block_move`); `set_image_z_order()` in the engine; Bring to front / Send to back in the image inspector.
- **Task 4.2 — Canvas wiring** (`2c8888d`) — both drag gestures wired into the ScrollArea canvas using the file's take-and-commit-locally pattern; `commit_image_move` body repaired (was a single line of literal `\n` escapes).
- **Task 4.2 — Completed (`e21c6f6`)** — canvas drag-to-move wiring, corner resize handles and a free-angle rotation grip (`rotate_image` in the engine: the drawing matrix turns around the box centre, pixels never re-encoded). The engine test `rotation_libre_fait_tourner_autour_du_centre` renders a 45° turn and checks the halves swap vertically.

## Notes

- The canvas code cannot call `self` while it borrows `renderer`/`tabs`: take state out before the `show_viewport` closure, commit into `pending_*` locals, write after the closure.