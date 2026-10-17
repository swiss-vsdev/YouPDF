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

## Session log

- 2026-10-16 (UI pass 2): centered thumbnails; full vector icon set rewritten (rail, pills, tools)
  plus 20 new tool icons; per-mode toolbars (Comment/Edit/Form/Redact) that replace the mode pills and
  end with `Close`; `Tools` dropdown with mode entries (`^1`..`^4`) and Organize pages; search panel
  header with funnel, 4 action buttons, `History` row with clear-all; floating toolbars anchored to the
  canvas (they shift when a side panel opens, as in the reference); rail button 38 pt; panel 210 pt.
  Dev helper: `YOUPDF_MODE=0..3` opens the app with that mode active.


- 2026-10-16 (document tools pass): form fields can be created (widget + `/AcroForm` + appearance);
  the canvas tool lookup now runs for every mode, which had kept prepare-form and redaction drags from
  ever reaching the canvas; watermarks, headers and footers, page numbers, compression and password
  protection each have a window in the actions menu. Reference-app behaviour notes are in
  `docs/updf-behaviour.md` (labels, option sets, flows, and what could not be confirmed).

  Open questions from that pass:
  1. Header and footer share one set of slots and the band is drawn both at the top and at the bottom;
     the number joins the footer centre. The reference app has separate header and footer slots. Either
     extend `HeaderFooter` with a second set of slots, or make the position selectable.
  2. Compression offers no quality grades because the engine only deflates streams and prunes objects;
     image resampling and font subsetting are what the reference `Reduce File Size` grades actually do.
  3. Protection writes a copy only. The reference app also reopens protected files and asks for the
     password; the engine has no decrypt/password-check path yet.
  4. The form panel lists fields and fills them, but no properties editor exists yet (name, tooltip,
     required, read-only, options), and field names are auto-generated (`Field1`, `Field2`).
- 2026-10-16 (audit pass): two reviewers swept the workspace, one on the interface and one on
  the engine, and every finding was fixed in its own commit with a regression test where the
  behaviour was testable. The engine fixes: an odd-length `/Names` array no longer panics, a
  cyclic `/Kids` tree no longer multiplies the attachment list, an indirect `/Fields` array keeps
  the fields already declared (reading and writing), the multi-select flag uses bit 21, redaction
  on a turned or cropped page paints and places in the displayed frame, inserting "after the last
  page" appends instead of prepending, inserted pages point at the node that holds them,
  `sanitize` keeps attachments while removing JavaScript, the OCR text layer is written in
  WinAnsi, the tiled watermark is bounded on an oversized page, and PDF dates are read and written
  in the PDF format. The interface fixes: unsaved annotations are no longer reloaded away after a
  document-rewriting action, closing a modified tab asks first, per-document state (redaction
  marks, form field, selection, ticked pages, search results) is cleared on a tab switch,
  Backspace no longer deletes an annotation while typing, Command-F opens the search box, a filled
  form field refreshes its page, and compressed or encrypted copies carry the pending annotations.
- 2026-10-16 (missing-features pass 1): the reference app's menus were walked against YouPDF and the
  gap was filled in four engine commits plus one interface commit. `convert` gained CSV, XML and RTF
  text exports and per-page image export; `crop` gained crop and reset; `create` gained blank
  documents, PDFs built from images and document splicing; `watermark` gained page backgrounds
  (colour or image, painted behind the content) and Bates numbering; `protect` gained
  `file_is_encrypted`, `open_protected` and `save_decrypted`, so a protected file can be reopened
  and an unprotected copy written. The interface now reaches all of it from the tools menu and the
  overflow menu, with new windows for crop, background, Bates, compare, OCR and create, a password
  prompt for protected files, print through `lp`, reveal in the file manager, and image export at a
  chosen resolution. Verified with `cargo clippy --workspace --all-targets` (clean), 75 core tests,
  and a smoke run that opened every window (no error line, one Tesseract probe).

  Still missing after this pass, in rough priority order:
  1. Form field properties (name, tooltip, required, read-only, options) and FDF/XFDF/CSV/JSON
     form data import/export.
  2. Batch processing (the reference app's Traitement par lots: convert, OCR, combine, insert,
     create, watermark, background, header/footer, Bates, compress, encrypt, remove, print).
  3. Auto bookmarks from the text, and data extraction (tables, key/value pairs).
  4. Measure tools (distance, perimeter, area).
  5. PDF/A, PDF/X, PDF/E and image-PDF output; Word/Excel/PowerPoint export (needs a ZIP/DOCX
     writer, the `docx-rs` chain is still unresolvable).
  6. Themes beyond dark (light, mint, starry), slideshow, and an i18n layer — the reference app
     ships 12+ languages; YouPDF strings are English literals today.
  7. The optional AI panel (providers exist in `YouPDF_OLD`) and signature requests.
  Out of scope by design: cloud sync, eIDAS signature requests, screen capture, scanner import,
  dictation and emoji — they need a cloud service or OS facilities the brief forbids or excludes.
- 2026-10-16 (missing-features pass 2): the five remaining engine gaps were implemented in parallel,
  each in its own file and its own commit, then wired into the interface. `forms` gained a properties
  editor (name, tooltip, Required, Read only, Multiline, choice options) and form data exchange in
  FDF, XFDF, CSV and JSON. `structure` gained `generate_outline`, which reads the heading sizes to
  rebuild the bookmark tree, and `clear_outline`. `extract` recognises aligned tables and key/value
  pairs and can write either set to CSV. `pdfa` declares PDF/A-2b, PDF/X-4 or PDF/E-1 on a copy
  (XMP, `/OutputIntents`, `/MarkInfo`, `/ID`) and writes an image-only PDF; the module says plainly
  that this is a declaration, not a validation, and that no ICC profile bytes are embedded.
  `measure` draws distance, perimeter and area measurements as page content with a labelled value.
  The interface also gained slideshow (F5), batch processing over a file list, and the
  displayed-frame/page-frame conversion below.

  Fix from that pass: the canvas works in the displayed frame (rotation applied, origin at the
  visible box) while the PDF is written in the page frame. On an unrotated page at the origin the
  two coincide, which hid the bug; on a rotated or cropped page every annotation, image, form field,
  measurement and text block was written to the wrong place. `PageView` now has the inverse
  conversions (`page_point`, `page_rect`, `display_point`) and the application converts at the
  boundaries. Covered by `tests/page_view.rs`.

  Still missing after this pass, in rough priority order:
  1. Word/Excel/PowerPoint export (needs a hand-rolled ZIP/DOCX writer; the `docx-rs` chain is still
     unresolvable).
  2. Themes beyond dark (light, mint, starry) and an i18n layer — the reference app ships 12+
     languages, YouPDF strings are English literals today, and the palette is compile-time consts.
  3. The optional AI panel (providers exist in `YouPDF_OLD`) and signature requests.
  4. Compression quality grades (image resampling, font subsetting) — the engine only deflates
     streams and prunes objects.
  5. Separate header and footer slot sets (today they share one set and the band is drawn twice).
  6. Embedded ICC profile bytes for the declared conformance, and a structure tree if tagged PDF
     is wanted.
  Out of scope by design: cloud sync, eIDAS signature requests, screen capture, scanner import,
  dictation and emoji — they need a cloud service or OS facilities the brief forbids or excludes.

- 2026-10-16 (audit of the two missing-features passes): an independent review of the ten commits
  found twelve defects; all twelve are fixed and each fix has its own commit. The frame conversion
  from the previous entry was incomplete — the two save paths, the search highlight and the
  redaction marks still mixed the displayed and page frames, and the redaction one meant that on a
  rotated page the black box landed on the wrong area while the sensitive text survived. The form
  data exchange lost every accent twice over: the FDF writer passed WinAnsi bytes through
  `from_utf8_lossy` and the reader did the same for `/V`, `/T`, `/TU` and `/Opt`. The field
  properties panel wrote an empty `/TU` and cleared the Required and Multiline bits because
  `FormField` did not carry them. Renaming truncated a top-level name at its last dot. An area
  measurement was divided by the scale instead of its square. Closing the Measure window left the
  tool armed, the Extract window kept the previous document's results, and the batch ran inside the
  egui frame, freezing the window for its whole duration. `export_form_data` and `import_form_data`
  now work on bytes rather than `String`.

  Also in that window of work, three features the previous list still counted as missing: the header
  and the footer have their own three slots instead of sharing one set drawn twice, the interface
  has four themes (Dark, Light, Mint, Starry) read from a runtime palette instead of compile-time
  constants, and View ▸ Theme switches between them.

- 2026-10-17 (AI as bring-your-own, and a fix pass over two audits): the AI panel is now wired
  into the interface and the whole AI layer is opt-in. `AppSettings.ai_enabled` defaults to false;
  while it is false nothing at all happens: no provider is built, the OS keyring is never read, no
  thread is spawned, no tokio runtime exists and no socket is opened. The worker thread is created
  on the first request rather than at startup, and the provider is built *inside* that thread so
  the keyring is never touched on the UI thread. Turning the switch off drops the channels and
  bumps a generation counter; the worker compares each request's enqueue-time generation and
  abandons anything not yet dispatched. One limitation is stated plainly in the code and here: a
  request already inside the HTTP call cannot be recalled.

  The panel itself: a master switch, a provider list (Ollama local, OpenAI, Anthropic, OpenRouter,
  Gemini), a per-provider base URL and model id, an API key stored in the system keyring and never
  in `settings.json`, a model list, and a task picker (chat, summarize, key points, translate,
  explain, legal review, financial review). Document text is extracted once per tab, capped at 40
  pages and 60 000 characters, and is only sent when the user presses a button. A configured base
  URL is honoured for OpenAI too; a base URL containing credentials is rejected and its password
  is redacted from the error text; HTTP is only accepted on the loopback interface.

  Two features the plan still listed as missing are now reachable: Office export (Word, Excel,
  PowerPoint) and compression quality grades (Screen 72 dpi/q55, Ebook 150/q75, Print 300/q88),
  both hand-rolled in core and wired to the Export and Compress windows. Embedded-image extraction
  is reachable too (one PNG per embedded image, via `extract_image_png`).

  Two audit rounds ran over the tree (one diff reviewer, one whole-repo auditor, then a
  verification round). Defects found and fixed, each in a commit:

  - Redaction destroyed page content but left the in-memory annotations, so the next save wrote an
    annotation back over a redacted area.
  - Closing a tab recomputed the active index only past the end, so closing a tab below the current
    one selected the wrong document; and forcing `last_active` skipped the session reset, so the
    new tab inherited the old tab's redaction marks and search results.
  - Undo always set the dirty flag, so undoing back to the saved state still prompted on close.
    A separate structural flag and an annotation baseline now decide.
  - The theme was hardcoded to Dark and never persisted; the dead per-provider `enabled` field is
    gone; `AppSettings` derives `Default` as a whole so a partial settings file parses.
  - Sizes derived from a PDF are validated or computed in `usize`: `width*height*4` overflowed in
    `u32`; the pdfium page index truncated at 65535 pages; the diff LCS table could allocate
    hundreds of megabytes; a `/FlateDecode` stream could inflate without bound; a JPEG header could
    claim huge dimensions and allocate before failing; `/Width` and `/Height` were cast from `i64`
    unchecked; `load_image_rgb` had no limits.
  - `compress`/`compress_with` mutated the document outside `inner_mut`, leaving the serialization
    cache stale so `save` could write pre-change bytes.
  - `decode_stream_content` treated a stream with no `/Filter` as an error, silently skipping every
    unfiltered image in the compression pass.
  - `directories` (which pulls `option-ext`, MPL-2.0) is gone in favour of a hand-rolled config
    path, so the dependency tree stays MIT/Apache/BSD.

  Still open, in rough priority order:
  1. An i18n layer. `AppSettings.locale` is stored but unused; strings are English literals and the
     reference app ships 12+ languages.
  2. Embedded ICC profile bytes for the declared PDF/A, PDF/X and PDF/E output, and a structure
     tree if tagged PDF is wanted. The conformance markers are still a declaration, not a
     validation.
  3. Font subsetting in the compression pass; today only images are resampled.
  Out of scope by design, unchanged: cloud sync, eIDAS signature requests, screen capture, scanner
  import, dictation and emoji.
