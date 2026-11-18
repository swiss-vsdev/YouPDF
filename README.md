# YouPDF

A clean-room, local-first **desktop PDF editor** written in Rust with egui/eframe and PDFium. No account, no telemetry, no cloud required. GPL-3.0.

[![CI](https://github.com/swiss-vsdev/YouPDF/actions/workflows/release.yml/badge.svg)](https://github.com/swiss-vsdev/YouPDF/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

## What it does

- **View & navigate** — tabs (pin/duplicate/reorder), smooth zoom/pan, thumbnails, outline, comments, search with replace, presenter view, read-aloud (macOS `say`), Open Quickly (⌘F).
- **Edit** — in-place text and image editing; highlight/underline/strikeout, ink, stamps, shapes, signatures, links; crop, split, merge, reorder, rotate, resize, page labels, Bates numbering, watermarks, headers/footers.
- **Forms & security** — AcroForm reader/editor, FDF/XFDF/CSV/JSON import/export, push-button actions; AES-256 R6 encryption, permissions, two-phase destructive redaction with code sets, PKCS#7/PAdES digital signatures with a validity banner.
- **Convert & OCR** — DOCX/XLSX/PPTX export, Office import via LibreOffice (`soffice`), images, text/HTML/CSV/XML, PDF/A-1b; optional Tesseract OCR (48 languages, 3 output modes); create a PDF from a screenshot, window, or clipboard.
- **Tools** — measurement suite, 12-operation batch queue, document comparison.
- **AI (off by default)** — bring your own provider (OpenAI, Anthropic, Gemini, Ollama, or any OpenAI-compatible endpoint). API keys live in the OS keyring, never in a config file. Document text is sent only when you press a button; no provider or network connection is created while the AI switch is off.

## Architecture

Four workspace crates, no server and no database:

| Crate | Responsibility |
|---|---|
| `youpdf-shared` | Error type, i18n, theme tokens, atomic file writes |
| `youpdf-core` | PDF engine: lopdf model, PDFium rendering, OCR, edit/redact/sign/organize/export |
| `youpdf-ai` | Provider-agnostic AI layer: endpoint hardening, keyring, tasks |
| `youpdf-app` | egui/eframe desktop UI (`main.rs` → `YouPdfApp` + panel modules under `src/app/`) |

Data flow: a file is opened into a `youpdf_core::Document`; edits mutate that document plus a per-tab annotation list; saving merges the annotations and writes the whole file atomically (temp file + `rename`). Settings live in the OS config directory; API keys live in the OS keyring; recents/stars live in `~/.youpdf` (`%USERPROFILE%` on Windows).

## Build & run

Prerequisites:
- Rust **stable** (see `rust-toolchain.toml`; workspace `rust-version` is 1.78) with `rustfmt` and `clippy`
- PDFium native library — **required for rendering** (the app falls back to a blank renderer without it)
- Optional: Tesseract with language data for OCR; LibreOffice (`soffice`) for Office import

```bash
# 1. Install PDFium into ./pdfium (macOS/Linux; Windows: .\\scripts\\install-pdfium.ps1)
./scripts/install-pdfium.sh

# 2. Build and run
cargo run --bin youpdf

# 3. Release artifacts (scripts handle icons and bundling)
bash scripts/build-macos-app.sh arm64      # or x86_64
bash scripts/build-linux-appimage.sh
.\scripts\build-windows-zip.sh
```

If PDFium is installed elsewhere, point `YOUPDF_PDFIUM_DIR` (or `PDFIUM_DIR`) at its directory. On macOS, a downloaded dylib may need its quarantine attribute removed and ad-hoc codesigning — `install-pdfium.sh` does both.

## Environment variables

Names only; no values are stored in this repository.

| Name | Purpose |
|---|---|
| `YOUPDF_PDFIUM_DIR`, `PDFIUM_DIR` | Override the directory containing the PDFium native library |
| `RUST_LOG` | `tracing` filter for diagnostics (default `youpdf=info,warn`) |
| `YOUPDF_MODE` | Development only: open directly in a given UI mode |
| `HOME` / `USERPROFILE` | Locate config, recents, and stars |
| `XDG_CONFIG_HOME` / `APPDATA` | Override the settings directory (Linux / Windows) |
| `PATH` | Used to discover `tesseract`, `soffice`, `say`, and `lp` |

No environment variable holds an API key: keys are stored in the OS keyring via the AI panel.

## Tests & checks

```bash
cargo check --workspace
cargo clippy --workspace --all-targets
cargo test --workspace --no-fail-fast
python3 scripts/i18n_extract.py --check
```

`cargo test --workspace` currently runs **414 tests, 0 failures**. Rendering-dependent tests skip themselves with a message when PDFium cannot be loaded, so the suite is green on a checkout without the native library; install PDFium to exercise them for real. Clippy reports pre-existing style warnings but no errors. `cargo fmt --check` is not currently clean across the legacy files.

## Known limitations

- macOS release binaries are ad-hoc signed; first launch requires right-click → Open.
- There is no auto-updater; updates are manual.
- AI chat history is in-memory for the current session (not persisted across restarts).
- Whole-PDF translation is approximate: multi-column layouts and tables are not reflowed.
- Some feature-parity gaps against UPDF 2.5.6 remain; see `docs/GAP.md` and `docs/visual-parity.md`.

## Documentation

- [`docs/AUDIT_REPORT.md`](docs/AUDIT_REPORT.md) — Phase 1 security/quality audit and proposed fixes
- [`docs/PLAN.md`](docs/PLAN.md) — stack and phase breakdown
- [`docs/GAP.md`](docs/GAP.md) — UPDF feature-parity checklist
- [`docs/STATUS.md`](docs/STATUS.md) — build and verification status
- [`docs/visual-parity.md`](docs/visual-parity.md) — appearance differences

## License

GPL-3.0 — see [LICENSE](LICENSE).
