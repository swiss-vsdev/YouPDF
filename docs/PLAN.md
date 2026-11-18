# YouPDF — Plan

Free, clean-room Rust PDF editor with a UPDF-like workflow. Repo: `~/YouPDF` (GPL-3.0, v0.1.0).
Boundary: no UPDF code/assets/strings copied; layout and interaction patterns only, own icons/logo/copy; no license system in YouPDF (nothing to crack, nothing to nag).

## Stack

- Render engine: **PDFium** via `pdfium-render` (BSD-3, prebuilt dylib from bblanchon/pdfium-binaries). App code 100% Rust; PDFium is C++ statically linked. Escape hatch: `DocumentBackend` trait so `hayro` (pure Rust) can swap in later.
- UI: **egui** + `wgpu` + `winit`. Immediate-mode, custom titlebar (traffic lights inset), floating pill toolbars.
- Structural edits: `lopdf`. Forms: PDFium form API. Icons: Lucide/Phosphor (MIT) restyled; palette from ux-spec §6.
- No telemetry, no auto-updater, no Firebase (unlike target).
- Deps all MIT/Apache/BSD.

## Repo layout

```
YouPDF/
  Cargo.toml            # workspace
  crates/
    youpdf-shared/      # errors, theme, i18n
    youpdf-core/        # document model, PDFium wrapper, render pipeline, ops
    youpdf-ai/          # providers, keyring, tasks registry
    youpdf-app/         # egui UI, theme, panels, shortcuts
  assets/icons/         # own SVGs
  docs/PLAN.md          # this file
  docs/GAP.md           # UPDF 2.5.6 vs YouPDF, per feature (reference material)
  docs/visual-parity.md # 1-to-1 appearance parity (pixel diff list)
  docs/STATUS.md        # last commit, verification, next step
  LICENSE               # GPL-3.0
```

## Phases (first release = 0.1-0.4, per decision)

- **v0.1 Viewer** — window + custom titlebar; tabs (open/close/+); drag-drop; async tiled rendering (worker pool, LRU cache, prefetch); smooth zoom/pan; fit-width default; left rail (8 tools); thumbnails panel; outline; search panel (find, next/prev, highlight hits); status/zoom; Cmd shortcuts (⌘O/⌘W/⌘F/⌘±/⌘0-9); dark theme.
- **v0.2 Read+** — text select/copy, links, rotate, page layouts (single/continuous/two-page/cover), fullscreen.
- **v0.3 Annotate** — highlight/underline/strikeout, sticky notes, text boxes, ink, shapes, stamps; select/move/delete; presets (color/width); incremental save; flatten.
- **v0.4 Page ops + forms** — Organize Pages surface (tiles, multi-select, rotate, delete, insert, extract, replace, split, merge); watermark/header/footer/page numbers; AcroForm fill + flatten; images↔PDF; compress; export text/HTML; password protect.
- **v0.5 Content editing** — text touch-up via content-stream surgery + font matching; image replace. Fragile on complex PDFs; spike before promising.
- **v0.6 Stretch** — OCR (bundled tesseract), redaction (two-phase mark→apply), PDF→Word (layout loss), signatures (PAdES), compare, optional local AI (Ollama, off by default).

## Shipped beyond the phase plan (Tier 1/2 round, e8b3a8ea)

- **Tier 1 (7/9 closed):** Open Quickly (⌘F), presenter view, read-aloud TTS, document stats, security-properties matrix, link appearance, redaction code sets + repeat-across-pages + auto-size. Partial: digital-signature validity banner (done) + timestamp trust note (pending); measure azimuth/altitude (done), snap/rulers/embedded-scale (deferred).
- **Tier 2:** page labels (`organize/labels.rs`), page resize (`organize/resize.rs`), interleave/exchange (`organize/exchange.rs`), AI writing tools + AI Check Pages, whole-PDF bilingual translation, Office import via `soffice`, create-from-capture.

## Remaining tasks

All Tier 1 gaps tracked in `docs/GAP.md` are closed or partial. The open backlog:

1. **1-to-1 visual parity** — `docs/visual-parity.md` §10 has the remaining pixel diffs: home thumbnail per-page aspect. (Native 9-menu bar, toolbar auto-hide §2.1, paper style §3.1, page shadow §3.2, Liquid Glass toggle §3.5, traffic-light inset §4.1, mode-pill capsule chrome §5.1–5.3, status-bar layout §6.1, back/forward page history §7.3, cover-page-alone §7.4, and the hardcoded-colour sweep are DONE.)
2. **Tier 2 leftovers** (`docs/GAP.md`): element filter panel, AI bookmark generation, AI image generation, mind-map, web AI, deep thinking.
3. **Tier 3** (`docs/GAP.md`): PDF/UA, PDF/E, PDF/X writers; validating PDF/A (ICC); XFA; PDF JavaScript; push-button Submit action; cloud/account/subscription.

## Next up

A visual diff from `docs/visual-parity.md` §10. The native 9-menu bar (§1.1–1.3) and the Go ▸ Back/Forward page-history stack (§7.3, `⌘[`/`⌘]`) are both in (`crates/youpdf-app/src/app/menu.rs`, `app.rs`, `shortcuts_ui.rs`) — the Go menu now matches the full `reference notes` set. Next candidates: §5.1 capsule pills or §3.1 paper style, both of which need a UPDF screenshot baseline before guessing pixel values.

## Open items

1. i18n is wired (365-entry EN/FR dictionary, coverage gate); shipping 12+ target languages is a translation task, not a code task.
2. Cloud/account/subscription surface — a product decision, not a sprint (see `docs/GAP.md` Tier 3).