# Status — YouPDF

- **Last commit**: `c58ee9d` feat(edit): add corner resize handles for content elements (Task 4.2)
- **Branch**: `main` (clean, 7 commits ahead of `origin/main`)

## Verification Commands (authoritative)

```bash
cargo check --workspace --all-targets
cargo clippy -p youpdf-app -p youpdf-core --all-targets
cargo test --workspace
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

## Open Parity Tasks (see `docs/GAP.md`)

1. **Task 4.2** — Content element arrange: drag-to-move, z-order and corner
   resize handles are done; only a free-angle rotation handle remains.

## Done

- **Docs consolidation** (`c490b43`) — nine overlapping planning files folded into `PLAN.md`, `GAP.md`, `STATUS.md`.
- **Task 14.2 — i18n** (`f1a6f64`) — all user-facing strings now go through `t()`; 363-entry EN/FR dictionary; coverage gate in `scripts/i18n_extract.py`.
- **Task 12.2 — Compare view modes** (`98c6a9c`) — side-by-side view with shared page navigation; ignore-whitespace/case options; `compare_with_options()` in the engine.
- **Task 4.2 — Content element arrange** (`45d48f2`) — drag-to-move for images (`commit_image_move`) and text blocks (`commit_text_block_move`); `set_image_z_order()` in the engine; Bring to front / Send to back in the image inspector.
- **Task 4.2 — Canvas wiring** (`2c8888d`) — both drag gestures wired into the ScrollArea canvas using the file's take-and-commit-locally pattern; `commit_image_move` body repaired (was a single line of literal `\n` escapes).
- **Task 4.2 — Corner resize handles** (`c58ee9d`) — four corner handles drag-resize a selected image; the committed matrix is written via `commit_image_scale`, which reuses `set_image_matrix`.