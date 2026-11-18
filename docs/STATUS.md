# Status — YouPDF

- **Branch:** `main`
- **Phase:** Phase 1 audit (`docs/AUDIT_REPORT.md`) and Phase 3 fixes (security → data → compute → code health → UI) are landed. UPDF 2.5.6 parity work from earlier waves remains; see `docs/GAP.md` and `docs/visual-parity.md`.

## Verification Commands (authoritative)

```bash
cargo check --workspace
cargo clippy --workspace --all-targets
cargo test --workspace --no-fail-fast
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

`cargo check --workspace` → clean. `cargo test --workspace --no-fail-fast` → **414 tests, 0 failures**; PDFium-dependent tests skip themselves with a message when the native library cannot be loaded (see `README.md`). `cargo clippy --workspace --all-targets` → pre-existing style warnings, no errors. `cargo fmt --all -- --check` → not clean across legacy files (no bulk reformat performed). `python3 scripts/i18n_extract.py --check` → exit 0.

## Parity Summary (see `docs/GAP.md`)

Tier 1 table now carries a `Status` column: **7 ✅ Closed, 2 🟡 Partial**.

| Tier 1 gap | Status |
|---|---|
| Digital signature UI polish (validity banner, invalidation warning) | 🟡 Partial — engine + window + cert manager + status banner done; timestamp trust note pending |
| Read-aloud / TTS | ✅ Closed — `readaloud_ui.rs` + `core/capture/tts.rs` |
| Word count / document stats | ✅ Closed — `app/windows_ui.rs:329` `DocumentStats` |
| Link appearance editor | ✅ Closed — visible/width/dash/colour in `inspector_ui.rs` |
| Redaction extras (code sets, repeat-across-pages, auto-size) | ✅ Closed |
| Measure extras (azimuth, altitude, markup labels) | 🟡 Partial — done; snap/rulers/embedded-scale deferred |
| Security properties matrix | ✅ Closed — `Protect ▸ Security Properties…` |
| Open Quickly launcher | ✅ Closed — `home_ui.rs` `open_quickly_panel` (⌘F) |
| Presenter view | ✅ Closed — `presenter_window` |

## Recently Closed

- **Wave 5 parity** (`e8b3a8ea`) — PDF-file watermarks, page labels editor, permissions matrix, forms cleanup.
- **Measure extras** — `measure_ui.rs`: azimuth (90° east) and altitude (90° vertical) drags, markup labels, 8 tests.
- **Redaction extras** — `core/redact/codesets.rs` (`DEFAULT_CODE_SETS` incl. U.S. Privacy Act / FOIA), repeat-mark-across-pages, auto-size-to-region (`fit_block`).
- **Open Quickly / Presenter / Read-aloud** — `home_ui.rs` ⌘F launcher; `slideshow_ui.rs` `presenter_window`; `capture/tts.rs` + `readaloud_ui.rs`.
- **AI writing tools & Check Pages** — 5 prompt tasks (Polish/Shorten/Expand/Proofread/Tone) + 7 defect detectors (`detect_page_defects`).
- **Whole-PDF bilingual translation** — `redact.rs` `TranslationOptions`/`translate_page`, `ai.rs` `translate_pdf`.
- **Office import** — shell to `soffice --convert-to pdf`.
- **Create from capture** — `core/capture` `capture_region_interactive`.

## Open High-Priority Gaps

See `docs/GAP.md` §"Open Parity Gaps" and `docs/visual-parity.md` (appearance-only).

1. **Native macOS 9-menu bar** — DONE (`menu.rs` `NSMenu` via `objc2`; §1.1–1.3) + full Go history (`⌘[`/`⌘]`, §7.3). §8 hardcoded-colour sweep → palette tokens DONE (6 new tokens).
2. **Visual parity (1-to-1)** — `docs/visual-parity.md`: toolbar layout presets, paper style, Liquid Glass, traffic-light inset, thumbnail aspect, mode-pill chrome, status-bar mirror, home thumbnail per-page aspect.
3. **Tier 2 leftovers** — element filter panel, watermark flatten, AI bookmark/image generation, mind-map, web AI.
4. **Tier 3** — PDF/UA / PDF/E / PDF/X writers, XFA, JavaScript, passkey/Touch ID, updater, cloud/account.

## Notes

- **Palette** (`crates/youpdf-shared/src/theme.rs`): 4 themes (Dark/Light/Mint/Starry), `Palette` with 15 fields. Tokens: CAPTION 10.5 / LABEL 11 / BODY 12 / TITLE 13 / MONO 11; radii TINY 3/XS 2/S 4/M 6/L 8/XL 10; spacing TINY 1/XS 2/S 4/M 6/L 8.
- **Phase 5 refactoring:** `app.rs` split into 20+ panel modules (`toolbar_ui.rs`, `chrome_ui.rs`, `aspect_ui.rs`, `decor_ui.rs`, `home_ui.rs`, `inspector_ui.rs`, `annots_ui.rs`, `pages_ui.rs`, `windows_ui.rs`, `forms_editor.rs`, `slideshow_ui.rs`, `measure_ui.rs`, `readaloud_ui.rs`, `redact_ui.rs`, `canvas_ui.rs`, `canvas_tools_ui.rs`, `organize_ui.rs`, `batch_ui.rs`, `export_ui.rs`, `prefs_ui.rs`).
- **i18n:** 365-entry EN/FR dictionary, coverage gate in `scripts/i18n_extract.py`.