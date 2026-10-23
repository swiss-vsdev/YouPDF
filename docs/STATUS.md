# Status — YouPDF

- **Last commit**: `45d48f2` feat(edit): drag-to-move and z-order for content elements (Task 4.2)
- **Branch**: `main` (clean, 3 commits ahead of `origin/main`)

## Verification Commands (authoritative)

```bash
cargo check --workspace --all-targets
cargo clippy -p youpdf-app -p youpdf-core --all-targets
cargo test --workspace
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

## Open Parity Tasks (see `docs/GAP.md`)

1. **Task 4.2** — Content element arrange: drag-to-move and z-order are done;
   resize/rotation handles are still YAGNI.

## Done

- **Docs consolidation** (`c490b43`) — nine overlapping planning files folded into `PLAN.md`, `GAP.md`, `STATUS.md`.
- **Task 14.2 — i18n** (`f1a6f64`) — all user-facing strings now go through `t()`; 363-entry EN/FR dictionary; coverage gate in `scripts/i18n_extract.py`.
- **Task 12.2 — Compare view modes** (`98c6a9c`) — side-by-side view with shared page navigation; ignore-whitespace/case options; `compare_with_options()` in the engine.
- **Task 4.2 — Content element arrange** (`45d48f2`) — drag-to-move for images (`commit_image_move`) and text blocks (`commit_text_block_move`); `set_image_z_order()` in the engine; Bring to front / Send to back in the image inspector.