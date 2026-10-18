# YouPDF

Free, local-first PDF editor in pure Rust (egui + PDFium). No account, no
watermark, no telemetry, no artificial limits.

Inspired by the workflow of modern PDF suites; original identity, icons,
strings and code.

## Layout

```
crates/
  youpdf-shared/   types, errors, theme tokens
  youpdf-core/     PDF engine: parsing, render (PDFium), annotate, forms,
                   redact, protect (RC4/AES-256), edit, OCR, convert, search
  youpdf-ai/       multi-provider AI layer (OpenAI-compatible, Anthropic,
                   Ollama; API keys in the OS keychain)
  youpdf-app/      desktop app (eframe/egui)
pdfium/            vendored libpdfium.dylib
scripts/           helper scripts
```

## Build

```sh
cargo run -p youpdf-app            # debug
cargo build --release -p youpdf-app
```

PDFium runtime: `pdfium/libpdfium.dylib` ships in the repo. If it is missing
or quarantined:

```sh
scripts/install-pdfium.sh
xattr -d com.apple.quarantine pdfium/libpdfium.dylib
codesign --force --sign - pdfium/libpdfium.dylib
```

Override the search path with `YOUPDF_PDFIUM_DIR=/path/to/dir`.

## Status

Implemented: tabs, home/recents, thumbnails, search & replace, zoom,
continuous scroll, page layouts, slideshow, text selection and copy, in-place
text and image editing, annotations (markup, ink, shapes, stamps, signatures),
forms (fill and build), page organizing, crop, watermark, header/footer, Bates
numbering, compression, protection (RC4/AES-256), redaction and sanitizing,
OCR, measurement, document comparison, batch processing, Office export
(Word/Excel/PowerPoint, flow and exact layout), AI assistant.

In progress: 1:1 UPDF parity per `docs/UPDF_IMPLEMENTATION_PLAN.md`.

## License

MIT — see `LICENSE`.
