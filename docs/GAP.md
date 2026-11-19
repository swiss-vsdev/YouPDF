# YouPDF — UPDF 2.5.6 Parity Gap Analysis

This document tracks YouPDF's feature parity against **UPDF 2.5.6** and the project feature inventory.

**Last updated:** November 2026
**Code state:** Phase 5 refactoring complete — `youpdf-core` split into `organize/`, `signature/`, `ocr/`, `redact/`, `forms/`, `edit/`, `watermark/`, `protect/`, `render/`, `annotate/` modules.

---

## Legend

- ✅ **Done** — Implemented and verified against the feature inventory
- 🟡 **Partial** — Core logic present; interface or edge cases missing
- ❌ **Missing** — Not implemented

---

## Parity Overview

| Feature Family | Status | Summary |
|---|---|---|
| **01. Window Chrome & Navigation** | ✅ | Tabs (pin/reorder/close), Home screen (Recents/Starred/Tools, grid/list, thumbnails), docked status bar, overflow `…` menu (macOS-native 9-menu bar not implemented) |
| **02. Left Rail & Side Panels** | ✅ | 8 rail tools (Organize, Search, Thumbnails, Bookmarks, Comments, Forms, Attachments, Aspect) |
| **03. Floating Inspectors & Mode Toolbars** | ✅ | Mode pills (Comment/Edit/Form/Redact), floating inspector (text, image, annot, form) |
| **04. Content Editing Engine** | ✅ | Caret navigation, typing, backspace, replace. Content arrange: drag-to-move (`commit_image_move`, `commit_text_block_move`), corner resize, free-angle rotation (`rotate_image`), z-order (`set_image_z_order`, Bring to front / Send to back) |
| **05. Commenting & Annotations** | 🟡 | Markup (4 types), smooth ink + 2-mode eraser, callout, 12 dynamic stamps, custom stamp, signature creator. **Missing:** Cloud, Sticker, Connected Lines, Squiggly, Object/Pixel dual eraser |
| **06. Form Builder & Filling** | 🟡 | Reader fill (text, check, radio, combo), Form Prep canvas, guides, 6-tab properties. **Missing:** push-button action dispatch (Reset/Submit/JavaScript), date picker, XFA forms, multiple form copies, full calculation-order editor |
| **07. Organize Pages Surface** | ✅ | Full-window grid, select odd/even/all, rotate, delete (confirmed), extract, insert, replace, split (by pages/size/bookmarks/custom range/tile) |
| **08. Page Operations & Decor** | 🟡 | Crop (4 margin boxes), tile split, text/image watermark (9 positions, opacity, page range, **tiling via `tile_gap`**, **removal via `clear_watermarks`**), 6-slot header/footer, Bates (batch only, prefix+start). **Missing:** page labels, page resize, interleaving/odd-even insertion, page exchange, PDF Portfolio, rotation randomness, templates/starred presets, AI-generated watermarks, PDF-file watermarks |
| **09. Security & Signatures** | ✅ | AES-256 R6, 2-phase redaction, PKCS#7 signing with certificate manager (create/import/self-sign), signature window, validation (`validate_signatures` called in UI). **Missing:** timestamp trust warning, PAdES/ETSI attribute, signature validity notification banner, signature creation canvas (mouse/trackpad/photo), cloud signature store |
| **10. Office Export & Conversion** | 🟡 | Word (DOCX framePr), Excel (XLSX), PPTX, PNG/JPEG/TIFF, long image, TXT/MD/HTML/CSV/XML/RTF, PDF/A-1b (declared-only, no ICC). **Missing:** Office import (.docx/.xlsx/.pptx → PDF), PDF/UA, PDF/E, PDF/X writers, Excel borderless-table recognition |
| **11. OCR Engine** | ✅ | Tesseract (48 languages), 3 output modes (Searchable, Editable, Text+Image), scanned PDF auto-detect banner, batch OCR. **Missing:** downloadable plugin model, borderless-table Excel export |
| **12. Measurement & Compare** | 🟡 | Distance/perimeter/area + azimuth/altitude drag, scale calibration, units, markup labels. Compare: text report, side-by-side, overlay blend. **Missing:** snap-to-path/endpoint/midpoint/intersection, rulers, embedded scale ratio, measurement info window |
| **13. Batch Processing** | ✅ | File queue table (progress, ETA, cancel), 12 operations (convert, OCR, combine, insert, watermark, background, header/footer, Bates, compress, encrypt, remove, print). **Missing:** Bates reverse/templates, interactive (non-batch) Bates |
| **14. Settings & Localization** | ✅ | Prefs window (General, Appearance, Document, AI). `t()` wired across whole UI (365-entry EN/FR dictionary, coverage gate in `scripts/i18n_extract.py`). **Missing:** 20 additional language slots (only 2 shipped), plugin manager, proxy settings, updater settings, passkey/Touch ID, Liquid Glass |

---

## Open Parity Gaps (High Priority)

### Tier 1 — High value, low effort

| Gap | Status |
|---|---|
| **Digital signature UI polish** | 🟡 Partial — Engine + window + cert manager done. Remaining: validity banner, invalidation warning, timestamp trust note. |
| **Read-aloud / TTS** | ✅ Closed — `readaloud_ui.rs` + `core/capture/tts.rs`: voice list, speed slider (wpm), play/stop. |
| **Word count / document stats** | ✅ Closed — `app/windows_ui.rs:329` `DocumentStats`: words, CJK chars, spaces, total. |
| **Link appearance editor** | ✅ Closed — `LinkAppearance` (visible-rect, width, colour) + line-style (solid/dashed/dotted) wired in `inspector_ui.rs`. Deferred: inset/outline shading. |
| **Redaction extras** | ✅ Closed — code sets (Privacy Act / FOIA / SSN / credit card / IBAN), repeat-across-pages, auto-size-to-region, fill colour all wired. |
| **Measure extras** | 🟡 Partial — Azimuth/Altitude, markup label done. Deferred: snap-to-path/endpoint/midpoint/intersection, rulers, embedded scale ratio. |
| **Security properties matrix** | ✅ Closed — `Protect ▸ Security Properties…` decodes `/Encrypt` `/P` matrix. |
| **Open Quickly launcher** | ✅ Closed — `home_ui.rs` `open_quickly_panel` on ⌘F. |
| **Presenter view** | ✅ Closed — `presenter_window` with layout, time bar, next-page preview, page nav. |

### Tier 2 — High value, medium effort

| Gap | Notes |
|---|---|
| **Element arrangement UI** | Align/arrange panel, element filter (Text/Image/Link/Path/Form). |
| **Page labels** | `/PageLabels` reader + writer (roman/arabic/custom). |
| **Page resize** | Scale pages to target dimensions. |
| **Interleaving / odd-even insertion** | Interleaved, odd-pages-only, even-pages-only, swap odd/even. |
| **Page exchange** | Customized swap of two page ranges. |
| **Image + page redaction** | Redact images, redact whole pages, find-and-redact. |
| **Watermark completeness** | Tiling (`tile_gap`) and removal (`clear_watermarks`) are done. Still missing: PDF-file watermarks, rotation randomness, templates/starred presets, AI-generated watermarks. |
| **AI Check Pages** | Detect blank/invalid/rotated/non-consecutive/comments/dimension-mismatch/scanned pages. |
| **AI writing tools** | Polish, shorten, expand, proofread, tone. |
| **AI bookmark generation** | Model-driven structure extraction (not just outline from headings). |

### Tier 3 — High value, high effort (or product decisions)

| Gap | Notes |
|---|---|
| **Cloud + account + subscription** | UPDF Cloud (1–100 GB), sign-in, redeem, device binding, enterprise accounts. Requires server. |
| **External cloud sources** | iCloud, Google Drive, Dropbox, OneDrive, FTP, WebDAV. |
| **Share by email / link / secret link** | Copy link, secret link, email share. |
| **Office import** | .docx/.xlsx/.pptx/.doc/.caj/.ofd → PDF. |
| **PDF Portfolio** | Collection of embedded files as one document. |
| **XFA forms** | `_LoadXFA`, `_GetXFAPacketContent`. |
| **PDF JavaScript actions** | `_PDFJSInterpreter`, `_PDFJS`. |
| **Push-button form actions** | Reset/Submit/JavaScript dispatch. |
| **Convert to PDF/UA + validating PDF/A** | Real PDF/A writer with ICC, PDF/UA tag generation. |
| **Create-from-capture** | Screen/window/selection capture, clipboard, scanner, ID-card. |
| **Whole-PDF bilingual translation** | Layout-preserving rewrite, bilingual output mode. |
| **AI image generation** | Watermark/background/sticker/stamp from text prompt. |
| **Deep thinking (DeepSeek-R1 class)** | Reasoning-model task. |
| **Fapiao / Chinese-invoice assistant** | 249-string, 15-file vertical. Narrow audience. |
| **Updater, plugin manager, passkey/Touch ID** | Polish items, individually small. |
| **Localization reach** | Dictionary has 365 entries, 2 languages. Expand to 12+ if needed. |

---

## Closed Gaps (Since Last Audit)

| Gap | Commit | Notes |
|---|---|---|
| **Task 4.2 — Content element arrange** | `e21c6f6` | Drag-to-move, corner resize, free-angle rotation, z-order. |
| **Task 12.2 — Compare view modes** | `c37981a` | Side-by-side, overlay blend, per-page highlights. |
| **Task 14.2 — i18n wiring** | `f1a6f64` | 365-entry EN/FR dictionary, `t()` throughout UI. |
| **Digital signature window + cert manager** | — | `sign_ui.rs`: `sign_window`, `certificate_panel`, `validate_signatures`. |
| **Redaction extras** | — | `app/redact_ui.rs` + `app/annots_ui.rs`: code sets (8 `DEFAULT_CODE_SETS` incl. U.S. Privacy Act / FOIA), `mark_code_matches`, repeat-mark-across-pages, auto-size-to-region via `redact::fit_block`. |
| **Measure extras** | — | `core/measure.rs`: `MeasureKind::{Azimuth,Altitude}` (degrees) + `Measurement::label`; `app/measure_ui.rs` + `app/canvas_commit.rs` wired. Angle label `90.0°`. Snap-to-path/rulers deferred. |
| **Link appearance line style** | — | `app/inspector_ui.rs`: solid/dashed/dotted via `LinkAppearance.dash`; visible-rect, width, colour already wired. |
| **Security Properties matrix** | — | `app/pages_ui.rs` `security_properties_window` + `Protect ▸ Security Properties…` menu item; decodes `/Encrypt` `/P` flags via `core/protect/permissions.rs`. |
| **Open Quickly launcher** | — | Already present: `app/home_ui.rs` `open_quickly_panel` (⌘F). |
| **Presenter view** | — | Already present: `app/slideshow_ui.rs` `presenter_window` (layout + time bar + next-page preview). |
| **Word count / stats, Read Aloud** | — | Already present: `app/windows_ui.rs:329` `DocumentStats` (words/CJK/spaces/total); `app/readaloud_ui.rs` + `core/capture/tts.rs` (voice list, speed slider, play/stop). |


---

## Verification Commands

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy -p youpdf-app -p youpdf-core --all-targets
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

---
