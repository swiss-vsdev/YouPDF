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
bookmarks and attachments sidebars, a comment list with filters, threads,
review states and FDF/XFDF/CSV/Markdown export, canvas themes and page tints,
forms (fill and build), page organizing, crop with box targets and auto-crop,
tile splitting, watermark with anchoring/tiling/layering, header/footer
macros, Bates numbering, compression presets with pruning and size estimate,
protection (RC4/AES-256, permissions, remove-protection), redaction and
sanitizing, OCR (searchable, text+image, editable) with scanned-page
detection, measurement with calibration, document comparison with a
difference navigator, batch processing (convert, OCR, insert, remove,
watermark, background, header/footer, Bates, compress, protect, flatten,
combine, print), Office export (Word/Excel/PowerPoint, flow and exact layout,
Excel column detection, Word image embedding), preferences with auto-save,
English/French chrome, and the AI assistant.

In progress: 1:1 UPDF parity per `docs/UPDF_IMPLEMENTATION_PLAN.md`. Known
gaps: PAdES/PKCS#7 signing (visual signatures only) and full localization
beyond the menus, tools, common buttons and preferences.

## License

MIT — see `LICENSE`.
