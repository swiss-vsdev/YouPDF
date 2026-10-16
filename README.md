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

v0.1 — viewer: tabs, thumbnails, search, zoom, continuous scroll.
Planned: annotation, page organizing, forms, redaction, protection, OCR,
conversion, batch, AI assistant. See `docs/` in the project history for the
UX spec and plan.

## License

MIT — see `LICENSE`.
