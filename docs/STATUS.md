# Status — YouPDF

- **Last commit**: `9bda5e1` feat(app): switch the UI font to SF Pro
- **Branch**: `main` (clean, 0 ahead of `origin/main`)

## Verification Commands (authoritative)

```bash
cargo check --workspace --all-targets
cargo clippy -p youpdf-app -p youpdf-core --all-targets
cargo test --workspace
```

## Open Parity Tasks (see `docs/GAP.md`)

1. **Task 14.2** — Full localization (i18n) wiring. (In progress)
2. **Task 12.2** — PDF comparison view modes.
3. **Task 4.2** — Content element arrange (hit-test, resize/rotation handles, drag-to-move/scale, z-order).
