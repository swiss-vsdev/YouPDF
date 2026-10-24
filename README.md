# YouPDF

Clean-room Rust PDF editor with a UPDF-like workflow.

- **Fast & Lightweight**: Pure Rust + egui + PDFium C API. Cold boot < 500 ms, instant tiled rendering.
- **Privacy First**: 100% local processing. No telemetry, no forced account, optional local AI (Ollama).
- **Free Forever**: No watermark, no feature locks, MIT licensed.

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

## Build & Run

### Prerequisites
- Rust 1.78+ (`rustup default stable`)
- macOS arm64 / x86_64 (Windows/Linux support planned)

### Running
```bash
cargo run --bin youpdf-app
```

### Testing
```bash
cargo test --workspace
```

## Documentation

- [`docs/PLAN.md`](docs/PLAN.md) — Stack, architecture, and phase breakdown
- [`docs/GAP.md`](docs/GAP.md) — UPDF 2.5.6 feature parity checklist
- [`docs/STATUS.md`](docs/STATUS.md) — Current build status and verification commands

## License

MIT — see [LICENSE](LICENSE).