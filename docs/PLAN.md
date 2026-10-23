# YouPDF — Plan

Free, clean-room Rust PDF editor with a UPDF-like workflow. Repo: `/Users/gabrielzeizer/Documents/GitHub/YouPDF` (LICENSE: MIT).
Boundary: no UPDF code/assets/strings copied; layout and interaction patterns only, own icons/logo/copy; no license system in YouPDF (nothing to crack, nothing to nag).

## Stack

- Render engine: **PDFium** via `pdfium-render` (BSD-3, prebuilt dylib from bblanchon/pdfium-binaries). App code 100% Rust; PDFium is C++ statically linked. Escape hatch: `DocumentBackend` trait so `hayro` (pure Rust) can swap in later.
- UI: **egui** + `wgpu` + `winit`. Immediate-mode, dark theme, custom titlebar (traffic lights inset), floating pill toolbars.
- Structural edits: `lopdf`. Forms: PDFium form API. Icons: Lucide/Phosphor (MIT) restyled; palette from ux-spec §6 (`#303030` / `#282828` / accent `#7A7EF6` / selected tint `#E8E8F8`).
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
  docs/GAP.md           # UPDF 2.5.6 vs YouPDF, per feature
  docs/STATUS.md        # last commit, verification, next step
  docs/rea/             # UPDF.app analysis, local reference (git-ignored)
  LICENSE               # MIT
```

## Reuse strategy (resolved)

Fresh scaffold + cherry-pick `youpdf-core` and `youpdf-ai` from the earlier workspace (same author, relicensed to MIT). `youpdf-ui`/`youpdf-app` rewritten from `ux-spec.md`. The earlier workspace stays on disk as reference, untouched.

## Phases (first release = 0.1-0.4, per decision)

- **v0.1 Viewer** — window + custom titlebar; tabs (open/close/+); drag-drop; async tiled rendering (worker pool, LRU cache, prefetch); smooth zoom/pan; fit-width default; left rail (8 tools); thumbnails panel; outline; search panel (find, next/prev, highlight hits); status/zoom; Cmd shortcuts (⌘O/⌘W/⌘F/⌘±/⌘0-9); dark theme.
- **v0.2 Read+** — text select/copy, links, rotate, page layouts (single/continuous/two-page/cover), fullscreen.
- **v0.3 Annotate** — highlight/underline/strikeout, sticky notes, text boxes, ink, shapes, stamps; select/move/delete; presets (color/width); incremental save; flatten. Toolbar mirrors ux-spec §5 Commentaire.
- **v0.4 Page ops + forms** — Organize Pages surface (tiles, multi-select, rotate, delete, insert, extract, replace, split, merge); watermark/header/footer/page numbers; AcroForm fill + flatten; images↔PDF; compress; export text/HTML; password protect.
- **v0.5 Content editing (spike first)** — text touch-up via content-stream surgery + font matching; image replace. Fragile on complex PDFs; spike before promising.
- **v0.6 Stretch** — OCR (bundled tesseract; quality below target's), redaction (two-phase mark→apply, same UX as target), PDF→Word (layout loss), signatures (PAdES), compare, optional local AI (Ollama, off by default).

## Snappy targets

Cold open < 500 ms; 60 fps scroll; no blank frame on zoom (low-res first, refine); render calls on worker pool; page textures cached; target installed size < 50 MB (vs 270 MB target).

## Packaging

`cargo-bundle` → YouPDF.app, embed pdfium dylib, ad-hoc codesign. macOS first (arm64), Windows/Linux later.

## Remaining tasks

The three open gaps, in `docs/GAP.md`:

1. **Task 4.2** — content element arrange: hit-test dispatch, resize/rotation handles, drag-to-move/scale, z-order. Foundation only so far.
2. **Task 12.2** — PDF comparison view modes.
3. **Task 14.2** — Full localization (i18n) wiring.

## Open items

1. Home/start page design (recents grid) — not inventoried; design fresh.
2. i18n framework choice (brief requires no hard-coded strings from first commit; 12+ languages, RTL-ready).
3. Verify the earlier workspace compiles as-is before porting (do this on the OLD copy, not in the new repo).
