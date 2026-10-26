# YouPDF — Contributing

## Subagent Wave Conventions

Subagents implementing parity gaps follow these rules to avoid merge conflicts.

### File Ownership

- Each agent owns **exactly one file** in `crates/youpdf-core/src/` or `crates/youpdf-app/src/app/`.
- **Never edit** `toolbar_ui.rs` or `i18n.rs` — these are frozen during a wave.
- To add a menu entry, report it and have the wave lead wire it in one clean commit.

### i18n

- Agents write raw strings through `t("English")` — **never** edit `i18n.rs` directly.
- Run `python3 scripts/i18n_extract.py --check` **once at the end of each wave** (not per-agent) to regenerate the dictionary. The extractor is append-only; it adds missing entries and removes dead ones.
- Dynamic text goes through `format!()` around translated fragments:
  ```rust
  format!("{} {}", t("Page"), page)
  ```
  Never invent this pattern — the extractor's regex won't match it.

### Engine + Test

- Wave 1 agents ship a core function and **one test** in `crates/youpdf-core/tests/<name>.rs`.
- The test is the ponytail check: it fails if the branch/loop logic breaks.

### Lightweight Defaults

- Prefer **platform binaries** over new dependencies: `say` for TTS, `screencapture` for screenshots, `soffice` for Office import.
- Prefer **reusing existing machinery** over new abstractions: `cm`-prepend scaling, `SearchIndex` text, `redact_marks` burn.
- Mark deliberate ceilings with `// ponytail:` comments naming the upgrade path.

### Verification

Each wave lands green:

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy -p youpdf-app -p youpdf-core --all-targets
python3 scripts/i18n_extract.py --check
```