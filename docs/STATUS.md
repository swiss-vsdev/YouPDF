# Status — YouPDF

- **Last commit**: `f1a6f64` feat(i18n): wire the full interface through `t()`
- **Branch**: `main` (clean, 0 ahead of `origin/main`)

## Verification Commands (authoritative)

```bash
cargo check --workspace --all-targets
cargo clippy -p youpdf-app -p youpdf-core --all-targets
cargo test --workspace
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

## Open Parity Tasks (see `docs/GAP.md`)

1. **Task 12.2** — PDF comparison view modes. (In progress)
2. **Task 4.2** — Content element arrange (hit-test, resize/rotation handles, drag-to-move/scale, z-order).

## Done

- **Docs consolidation** (`c490b43`) — nine overlapping planning files folded into `PLAN.md`, `GAP.md`, `STATUS.md`.
- **Task 14.2 — i18n** (`f1a6f64`) — all user-facing strings now go through `t()`; 351-entry EN/FR dictionary; coverage gate in `scripts/i18n_extract.py`.