# Status — YouPDF

- **Last commit:** `d5151d04` — Fix: remove all compiler warnings (unused imports, dead code, private types in public interfaces)
- **Branch:** `main` (up to date with `origin/main`; only these docs modified)
- **Phase:** Phase 5 refactoring complete — `youpdf-core` split into modular subdirectories (`organize/`, `signature/`, `ocr/`, `redact/`, `forms/`, `edit/`, `watermark/`, `protect/`, `render/`, `annotate/`).

## Verification Commands (authoritative)

```bash
cargo check --workspace --all-targets
cargo clippy -p youpdf-app -p youpdf-core --all-targets
cargo test --workspace
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

## Parity Summary (see `docs/GAP.md`)

| Verdict | Count |
|---|---|
| ✅ Done | 10 |
| 🟡 Partial | 4 |

**14 feature families tracked.** All three originally tracked gaps (4.2, 12.2, 14.2) are closed.

## Recently Closed

- **Task 4.2 — Content element arrange** (`e21c6f6`) — drag-to-move (`commit_image_move`, `commit_text_block_move`), corner resize, free-angle rotation (`rotate_image`), z-order (`set_image_z_order`, Bring to front / Send to back in image inspector).
- **Task 12.2 — Compare view modes** (`c37981a`) — side-by-side with shared page navigation, overlay blend with opacity slider, per-page difference highlights (amber frame + counter) fed by report.
- **Task 14.2 — i18n wiring** (`f1a6f64`) — 365-entry EN/FR dictionary, `t()` wired throughout UI, coverage gate in `scripts/i18n_extract.py`.
- **Digital signature UI** — `sign_ui.rs`: `sign_window()`, `certificate_panel()` (create/import/self-sign), `validate_signatures()` called from UI.

## Open High-Priority Gaps

See `docs/GAP.md` §"Open Parity Gaps" for Tier 1/2/3 backlog. Top 5 by value:

1. **Digital signature polish** — validity banner, invalidation warning, timestamp trust note.
2. **Read-aloud / TTS** — macOS AVSpeechSynthesizer wrapper, voice + speed.
3. **Word count / document stats** — words, CJK chars, spaces, total.
4. **Link appearance editor** — invisible/visible rect, border style, thickness, colour.
5. **Redaction extras** — code sets (U.S. Privacy Act / FOIA), repeat-mark-across-pages, fill colour, auto-size.

## Notes

- **Phase 5 refactoring:** `app.rs` reduced from ~7000 lines to ~2946 lines via 20+ panel module extractions. `youpdf-core` split into 10+ submodules for maintainability.
- **i18n:** 365 dictionary entries, 2 languages (EN/FR), 290 `t()` call sites (365-entry EN/FR dictionary). Coverage gate ensures new UI strings are added to dictionary.
- **Signature engine:** `crates/youpdf-core/src/signature/` (6 files: `identity.rs`, `mod.rs`, `sign.rs`, `util.rs`, `verify.rs`, `visual.rs`) — detached RSA-SHA-256, PKCS#12/PEM import, `validate_signatures`, visual signature rendering.
- **Canvas borrow pattern:** Canvas code cannot call `self` while it borrows `renderer`/`tabs`: take state out before `show_viewport` closure, commit into `pending_*` locals, write after closure.