# YouPDF

Clean-room Rust PDF editor with a UPDF-like workflow.

- **Fast & Lightweight**: Pure Rust + egui + PDFium C API. Cold boot < 500 ms, instant tiled rendering.
- **Privacy First**: 100% local processing. No telemetry, no forced account, optional local AI (Ollama).
- **Free Forever**: No watermark, no feature locks, [GPL-3.0](LICENSE) licensed.

[![CI](https://github.com/swiss-vsdev/YouPDF/actions/workflows/release.yml/badge.svg)](https://github.com/swiss-vsdev/YouPDF/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![macOS](https://img.shields.io/badge/macOS-arm64%20%7C%20x64-green.svg)](https://github.com/swiss-vsdev/YouPDF/releases)
[![Linux](https://img.shields.io/badge/Linux-x64-green.svg)](https://github.com/swiss-vsdev/YouPDF/releases)
[![Windows](https://img.shields.io/badge/Windows-x64-green.svg)](https://github.com/swiss-vsdev/YouPDF/releases)
[![Version](https://img.shields.io/badge/v0.1.0-blue.svg)](https://github.com/swiss-vsdev/YouPDF/releases)

## Downloads

| Platform | File |
|----------|------|
| macOS Apple Silicon | `YouPDF-macos-arm64-0.1.0.zip` |
| macOS Intel | `YouPDF-macos-x64-0.1.0.zip` |
| Linux x86_64 | `YouPDF-linux-x64-0.1.0.tar.gz` |
| Windows x86_64 | `YouPDF-win64-0.1.0.zip` |

Each archive is **fully self-contained**: pdfium and optional Tesseract OCR are bundled alongside the binary. No system install required.

## App Icon

![YouPDF icon](assets/icon.png)

## Screenshots

### Home screen

![Home](docs/screenshots/home.png)

### Document view

![Document view](docs/screenshots/document-view.png)

### Document editor

![Document editor](docs/screenshots/document-editor.png)

## Features

- **Viewer**: Tab bar (pin/duplicate/reorder), Home screen (Recents, Starred, Tools), smooth zoom/pan, docked status bar.
- **Navigation**: 8-tool left rail — Thumbnails, Outline, Comments, Search, Forms, Attachments, Aspect/Display tints.
- **In-Place Editing**: Edit text blocks with precise caret navigation, font/style options, and image replacements/rotations.
- **Annotations**: Highlight/underline/strikeout, sticky notes, callouts, smooth ink + segment/stroke eraser, 12 dynamic stamps, signature creator.
- **Page Ops**: Full-window Organize Pages grid, crop (pt/mm/cm/in), tile split, text watermarks, 6-slot header/footer, Bates numbering.
- **Forms**: AcroForm reader & preparation mode, guide snapping, 6-tab properties, FDF/XFDF/CSV/JSON import/export.
- **Security**: AES-256 R6, 2-phase destructive redaction (burn-in), PKCS#7 digital signatures with certificate creator & validator.
- **Export & OCR**: DOCX/XLSX/PPTX, PNG/JPEG/TIFF, long image, TXT/MD/HTML/RTF/CSV/XML, PDF/A-1b. Tesseract OCR (48 languages, 3 output modes).
- **Tools**: Measurement suite (distance/perimeter/area), batch processing queue (12 operations), document comparison.

## Installation

### macOS
1. Download the zip for your architecture (Intel or Apple Silicon).
2. Unzip it.
3. Right-click → **Open** the `.app` (first run only — macOS blocks ad-hoc signed apps by default).

### Linux
1. Download `YouPDF-linux-x64-0.1.0.tar.gz`.
2. Extract it: `tar xzf YouPDF-linux-x64-0.1.0.tar.gz`
3. Run: `./YouPDF-linux-x64/youpdf`

### Windows
1. Download `YouPDF-win64-0.1.0.zip`.
2. Unzip it.
3. Run `youpdf.exe` directly (no installation needed).

## Build & Run

### Prerequisites
- Rust 1.78+ (`rustup default stable`)
- macOS arm64 / x86_64 (tested)
- Linux x86_64 (requires `libxkbcommon-dev`, `libwayland-dev`, `libgl1-mesa-dev`, `libfontconfig1-dev` at build time)
- Windows x86_64 (requires MSVC build tools)

### Running
```bash
cargo run --bin youpdf
```

### Testing
```bash
cargo test --workspace
```

### Building release artifacts
```bash
# macOS
bash scripts/install-pdfium.sh
bash scripts/build-macos-app.sh

# Linux
bash scripts/install-pdfium.sh
bash scripts/build-linux-appimage.sh

# Windows
.\scripts\install-pdfium.ps1
.\scripts\build-windows-zip.sh
```

## Documentation

- [`docs/PLAN.md`](docs/PLAN.md) — Stack, architecture, and phase breakdown
- [`docs/GAP.md`](docs/GAP.md) — UPDF 2.5.6 feature parity checklist
- [`docs/STATUS.md`](docs/STATUS.md) — Current build status and verification commands

## License

GPL-3.0 — see [LICENSE](LICENSE).