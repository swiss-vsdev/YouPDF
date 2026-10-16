# YouPDF — Plan

Free, clean-room Rust PDF editor with a UPDF-like workflow. Repo: `/Users/gabrielzeizer/Documents/GitHub/YouPDF` (LICENSE: MIT).
Boundary: no UPDF code/assets/strings copied; layout and interaction patterns only, own icons/logo/copy; no license system in YouPDF (nothing to crack, nothing to nag).

Inventory evidence: `./ux-spec.md` + screenshots + `menus-fr.txt` (this directory).

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
    youpdf-core/        # document model, PDFium wrapper, render pipeline, ops
    youpdf-app/         # egui UI, theme, panels, shortcuts
  assets/icons/         # own SVGs
  docs/ux-spec.md       # copy of inventory
  LICENSE               # MIT
```

## Phases (first release = 0.1-0.4, per decision)

- **v0.1 Viewer** — window + custom titlebar; tabs (open/close/+); drag-drop; async tiled rendering (worker pool, LRU cache, prefetch); smooth zoom/pan; fit-width default; left rail (8 tools, ux-spec §2); thumbnails panel; outline; search panel (find, next/prev, highlight hits); status/zoom; Cmd shortcuts (⌘O/⌘W/⌘F/⌘±/⌘0-9); dark theme.
- **v0.2 Read+** — text select/copy, links, rotate, page layouts (single/continuous/two-page/cover), fullscreen.
- **v0.3 Annotate** — highlight/underline/strikeout, sticky notes, text boxes, ink, shapes, stamps; select/move/delete; presets (color/width); incremental save; flatten. Toolbar mirrors ux-spec §5 Commentaire.
- **v0.4 Page ops + forms** — Organize Pages surface (tiles, multi-select, rotate, delete, insert, extract, replace, split, merge); watermark/header/footer/page numbers; AcroForm fill + flatten; images↔PDF; compress; export text/HTML; password protect.
- **v0.5 Content editing (spike first)** — text touch-up via content-stream surgery + font matching; image replace. Fragile on complex PDFs; spike before promising.
- **v0.6 Stretch** — OCR (bundled tesseract; quality below target's), redaction (two-phase mark→apply, same UX as target), PDF→Word (layout loss), signatures (PAdES), compare, optional local AI (Ollama, off by default).

## Snappy targets

Cold open < 500 ms; 60 fps scroll; no blank frame on zoom (low-res first, refine); render calls on worker pool; page textures cached; target installed size < 50 MB (vs 270 MB target).

## Packaging

`cargo-bundle` → YouPDF.app, embed pdfium dylib, ad-hoc codesign. macOS first (arm64), Windows/Linux later.

## Added context (found 2026-10-16, after initial plan)

Sources: `YouPDF/UPDF_Deep.md` (product research), `YouPDF_OLD/Target_Job.md` (full product brief), `YouPDF_OLD/` (existing Rust workspace, Oct 12-14).

### YouPDF_OLD — existing implementation to reuse or port

- 5-crate workspace matching current plan: `youpdf-shared` (errors, theme), `youpdf-core` (annotate, compare, convert→DOCX, document, edit, forms, metadata, ocr, protect RC4-128 + AES-256 rev6, redact destructive, render PDFium, search, signature scaffold), `youpdf-ai` (providers: openai_compat — covers OpenAI/Azure/OpenRouter/Groq/LM Studio/llama.cpp/Gemini-compat — anthropic, ollama; keyring = OS keychain; tasks/registry), `youpdf-ui` (app, icons, layout, annotation_layer, ai_worker), `youpdf-app` (eframe binary).
- Vendored `pdfium/libpdfium.dylib` + `scripts/install-pdfium.sh`.
- Tests: smoke, audit_fixes ×2. Deps: egui/eframe 0.29 (glow), lopdf 0.34, tokio, reqwest rustls.
- Known gaps (own docs): PKCS#7 signature not implemented; OCR only "searchable PDF" mode; visual signature image decoder missing; no cloud/mobile/WASM.
- License in old workspace: MIT OR Apache-2.0.

### Target_Job.md — scope and acceptance criteria that bind YouPDF

- Acceptance: 500-page PDF opens < 1.5 s; scroll > 55 fps; text edit > 95 % layout preserved; OCR > 98 % on 300 dpi FR scan; PDF→Word > 90 % formatting; no watermark; no limits; ≥ 8 AI providers incl. 2 local (Ollama, OpenAI-compatible); "all-local" mode makes zero network calls (packet-verified); works without account; 12+ UI languages incl. FR; zero assets/code from competitors.
- Absolutes: no copied icons/logos/fonts/code; no pixel-identical UI (`look and feel`) — replicate usage logic with original identity; no hidden telemetry; never lock PDF features behind AI or subscription; API keys never stored in plaintext.
- Briefed UI sketch: tool tabs (Lecteur/Édition/Annotation/Organisation/Conversion/OCR/Formulaires/Protection/IA/Signer), left sidebar (Vignettes/Signets/Recherche/Calques/IA), right contextual panel (Propriétés/IA Chat/Commentaires/Formulaires), status bar (page · zoom · language · local lock · active provider), 4 themes (Soft Green, Paper-like, Light, Dark) as design tokens.
- Full feature target lists (§5.1-5.12 of Target_Job.md): read, edit, annotate, organize, convert (14 formats), OCR 38 languages × 3 output modes, forms + FDF/XFDF/CSV/JSON, e-signature incl. requests/eIDAS, protection + redaction + metadata scrub, batch queue, measure tools, compare, bookmarks auto, data extraction, advanced printing; cloud sync phase 4; enterprise phase 5.

### UPDF_Deep.md — market intel usable as differentiators

- UPDF gaps to beat: no Linux support; broken Thai/CJK font handling; slow cold load (a complaint); free tier watermarks + 2 conversions/day; OCR is a separate 1.4-1.7 GB plugin; AI subset behind separate subscription; prey to cloud disconnects.
- Our edge: single lightweight binary, offline-first PDF tools, no watermark ever, OCR packs optional and small, user-chosen AI (cloud or local), Linux supported from day one.

### Stack decision update (resolved)

- Target_Job.md proposed Tauri 2 + React 19; the built workspace is pure-Rust **egui/eframe**. **Decision: keep egui/eframe** (full Rust, no webview, smaller, snappier). Recorded divergence from the original brief.
- Render binding: old code vendors `libpdfium.dylib` directly and works — keep that approach for the port; `pdfium-render` crate stays an option if the direct binding gets in the way.
- **License (resolved): MIT only.** Ported files from YouPDF_OLD (owned by the same author) get relicensed to MIT; strip any dual-license headers while porting.
- **Reuse strategy (resolved): fresh scaffold + cherry-pick `youpdf-core` and `youpdf-ai`** (plus the tiny `youpdf-shared` for error/theme types if it ports cleanly). `youpdf-ui`/`youpdf-app` are rewritten fresh against `ux-spec.md`. YouPDF_OLD stays on disk as reference, untouched.

## Build order (first session, after plan mode)

1. `git init` in `/Users/gabrielzeizer/Documents/GitHub/YouPDF`; add LICENSE (MIT), README, `.gitignore`, `rust-toolchain.toml`. First commit = scaffold.
2. Create workspace: `crates/youpdf-shared`, `youpdf-core`, `youpdf-ai`, `youpdf-app` (fewest crates that port cleanly; fold shared into core if it drags no deps).
3. Port core modules (document, render, search, metadata first; annotate, forms, redact, protect, edit, ocr, convert, compare, signature next), fix imports, `cargo check` + run old tests (`smoke`, `audit_fixes*`) adapted.
4. Port ai layer (providers, keyring, registry, tasks) with tokio worker.
5. Rewrite UI from `ux-spec.md`: window chrome, rail (8 tools), panels, mode toolbars, organize surface, theme tokens; i18n framework from first commit (no hard-coded strings).

## Open items

1. Home/start page design (recents grid) — not inventoried; design fresh.
2. i18n framework choice (brief requires no hard-coded strings from first commit; 12+ languages, RTL-ready).
3. Verify `YouPDF_OLD` compiles as-is before porting (do this on the OLD copy, not in the new repo).


