# YouPDF — UPDF 2.5.6 Parity Gap Analysis

This document tracks YouPDF's feature parity against **/Applications/UPDF.app** (v2.5.6), cross-referencing `docs/rea/UPDF_FEATURE_INVENTORY.md` (the REA static analysis spec).

**Last updated:** October 2026  
**Code state:** Phase 5 refactoring complete — `youpdf-core` split into `organize/`, `signature/`, `ocr/`, `redact/`, `forms/`, `edit/`, `watermark/`, `protect/`, `render/`, `annotate/` modules.

---

## Legend

- ✅ **Done** — Implemented and verified against REA spec
- 🟡 **Partial** — Core logic present; interface or edge cases missing
- ❌ **Missing** — Not implemented

---

## Parity Overview

| Feature Family | REA Spec | Status | Summary |
|---|---|---|---|
| **01. Window Chrome & Navigation** | REA §A1, §B1 | ✅ | Tabs (pin/reorder/close), Home screen (Recents/Starred/Tools, grid/list, thumbnails), docked status bar, overflow `…` menu (macOS-native 9-menu bar not implemented) |
| **02. Left Rail & Side Panels** | REA §A2, §B1 | ✅ | 8 rail tools (Organize, Search, Thumbnails, Bookmarks, Comments, Forms, Attachments, Aspect) |
| **03. Floating Inspectors & Mode Toolbars** | REA §A2, §B2 | ✅ | Mode pills (Comment/Edit/Form/Redact), floating inspector (text, image, annot, form) |
| **04. Content Editing Engine** | REA §A6, §B2 | ✅ | Caret navigation, typing, backspace, replace. Content arrange: drag-to-move (`commit_image_move`, `commit_text_block_move`), corner resize, free-angle rotation (`rotate_image`), z-order (`set_image_z_order`, Bring to front / Send to back) |
| **05. Commenting & Annotations** | REA §A7, §B2 | 🟡 | Markup (4 types), smooth ink + 2-mode eraser, callout, 12 dynamic stamps, custom stamp, signature creator. **Missing:** Cloud, Sticker, Connected Lines, Squiggly, Object/Pixel dual eraser |
| **06. Form Builder & Filling** | REA §A8, §B2 | 🟡 | Reader fill (text, check, radio, combo), Form Prep canvas, guides, 6-tab properties. **Missing:** push-button action dispatch (Reset/Submit/JavaScript), date picker, XFA forms, multiple form copies, full calculation-order editor |
| **07. Organize Pages Surface** | REA §A11, §B3 | ✅ | Full-window grid, select odd/even/all, rotate, delete (confirmed), extract, insert, replace, split (by pages/size/bookmarks/custom range/tile) |
| **08. Page Operations & Decor** | REA §A11, §B3 | 🟡 | Crop (4 margin boxes), tile split, text/image watermark (9 positions, opacity, page range, **tiling via `tile_gap`**, **removal via `clear_watermarks`**), 6-slot header/footer, Bates (batch only, prefix+start). **Missing:** page labels, page resize, interleaving/odd-even insertion, page exchange, PDF Portfolio, rotation randomness, templates/starred presets, AI-generated watermarks, PDF-file watermarks |
| **09. Security & Signatures** | REA §A10, §A12, §B5 | ✅ | AES-256 R6, 2-phase redaction, PKCS#7 signing with certificate manager (create/import/self-sign), signature window, validation (`validate_signatures` called in UI). **Missing:** timestamp trust warning, PAdES/ETSI attribute, signature validity notification banner, signature creation canvas (mouse/trackpad/photo), cloud signature store |
| **10. Office Export & Conversion** | REA §A13, §B3 | 🟡 | Word (DOCX framePr), Excel (XLSX), PPTX, PNG/JPEG/TIFF, long image, TXT/MD/HTML/CSV/XML/RTF, PDF/A-1b (declared-only, no ICC). **Missing:** Office import (.docx/.xlsx/.pptx → PDF), PDF/UA, PDF/E, PDF/X writers, Excel borderless-table recognition |
| **11. OCR Engine** | REA §A9, §B4 | ✅ | Tesseract (48 languages), 3 output modes (Searchable, Editable, Text+Image), scanned PDF auto-detect banner, batch OCR. **Missing:** downloadable plugin model, borderless-table Excel export |
| **12. Measurement & Compare** | REA §A7, §A16, §B2, §B6 | ✅ | Distance/perimeter/area drag box, scale calibration, units. Compare: text report, side-by-side with shared page navigation, per-page difference highlights, overlay blend with opacity slider. **Missing:** azimuth/altitude, embedded scale ratio, snap-to-path/endpoint/midpoint/intersection, rulers, markup labels, measurement info window |
| **13. Batch Processing** | REA §A16, §B3 | ✅ | File queue table (progress, ETA, cancel), 12 operations (convert, OCR, combine, insert, watermark, background, header/footer, Bates, compress, encrypt, remove, print). **Missing:** Bates reverse/templates, interactive (non-batch) Bates |
| **14. Settings & Localization** | REA §A17, §B8 | ✅ | Prefs window (General, Appearance, Document, AI). `t()` wired across whole UI (365-entry EN/FR dictionary, coverage gate in `scripts/i18n_extract.py`). **Missing:** 20 additional language slots (only 2 shipped), plugin manager, proxy settings, updater settings, passkey/Touch ID, Liquid Glass |

---

## Open Parity Gaps (High Priority)

### Tier 1 — High value, low effort

| Gap | REA Ref | Notes |
|---|---|---|
| **Digital signature UI polish** | REA §B5 | Engine + window + cert manager done. Add validity banner, invalidation warning, timestamp trust note. |
| **Read-aloud / TTS** | REA §A5, §B8 | Voice selection + speed. Pure macOS AVSpeechSynthesizer wrapper. |
| **Word count / document stats** | REA §A18, §B8 | Words, CJK chars, spaces, total. |
| **Link appearance editor** | REA §A6, §B2 | Invisible/visible rect, border style, thickness, colour. |
| **Redaction extras** | REA §A12, §B2 | Code sets (U.S. Privacy Act / FOIA), repeat-mark-across-pages, fill colour, auto-size to region. |
| **Measure extras** | REA §A7, §B2 | Azimuth/altitude, snap-to-path/endpoint/midpoint/intersection, rulers, markup labels. |
| **Security properties matrix** | REA §A5, §B1 | Printing, changing, assembly, copying, extraction, commenting, signing permissions UI. |
| **Open Quickly launcher** | REA §A1, §B1 | ⌘F fuzzy file/document launcher. |
| **Presenter view** | REA §A5, §B1 | Slide layout, time bar, next-slide preview. |

### Tier 2 — High value, medium effort

| Gap | REA Ref | Notes |
|---|---|---|
| **Element arrangement UI** | REA §A6, §B2 | Align/arrange panel, element filter (Text/Image/Link/Path/Form). |
| **Page labels** | REA §A5, §B3 | `/PageLabels` reader + writer (roman/arabic/custom). |
| **Page resize** | REA §A6, §B3 | Scale pages to target dimensions. |
| **Interleaving / odd-even insertion** | REA §A6, §B3 | Interleaved, odd-pages-only, even-pages-only, swap odd/even. |
| **Page exchange** | REA §A6, §B3 | Customized swap of two page ranges. |
| **Image + page redaction** | REA §A12, §B3 | Redact images, redact whole pages, find-and-redact. |
| **Watermark completeness** | REA §A11, §B3 | Tiling (`tile_gap`) and removal (`clear_watermarks`) are done. Still missing: PDF-file watermarks, rotation randomness, templates/starred presets, AI-generated watermarks. |
| **AI Check Pages** | REA §A14, §B6 | Detect blank/invalid/rotated/non-consecutive/comments/dimension-mismatch/scanned pages. |
| **AI writing tools** | REA §A14, §B6 | Polish, shorten, expand, proofread, tone. |
| **AI bookmark generation** | REA §A14, §B6 | Model-driven structure extraction (not just outline from headings). |

### Tier 3 — High value, high effort (or product decisions)

| Gap | REA Ref | Notes |
|---|---|---|
| **Cloud + account + subscription** | REA §A15, §B7 | UPDF Cloud (1–100 GB), sign-in, redeem, device binding, enterprise accounts. Requires server. |
| **External cloud sources** | REA §A15, §B7 | iCloud, Google Drive, Dropbox, OneDrive, FTP, WebDAV. |
| **Share by email / link / secret link** | REA §A15, §B7 | Copy link, secret link, email share. |
| **Office import** | REA §A13, §B3 | .docx/.xlsx/.pptx/.doc/.caj/.ofd → PDF. |
| **PDF Portfolio** | REA §A11, §B3 | Collection of embedded files as one document. |
| **XFA forms** | REA §A18, §B2 | `_LoadXFA`, `_GetXFAPacketContent`. |
| **PDF JavaScript actions** | REA §A18, §B2 | `_PDFJSInterpreter`, `_PDFJS`. |
| **Push-button form actions** | REA §A8, §B2 | Reset/Submit/JavaScript dispatch. |
| **Convert to PDF/UA + validating PDF/A** | REA §A13, §B3 | Real PDF/A writer with ICC, PDF/UA tag generation. |
| **Create-from-capture** | REA §A13, §B3 | Screen/window/selection capture, clipboard, scanner, ID-card. |
| **Whole-PDF bilingual translation** | REA §A14, §B6 | Layout-preserving rewrite, bilingual output mode. |
| **AI image generation** | REA §A14, §B6 | Watermark/background/sticker/stamp from text prompt. |
| **Deep thinking (DeepSeek-R1 class)** | REA §A14, §B6 | Reasoning-model task. |
| **Fapiao / Chinese-invoice assistant** | REA §A18, §B8 | 249-string, 15-file vertical. Narrow audience. |
| **Updater, plugin manager, passkey/Touch ID** | REA §A15, §A17, §B7, §B8 | Polish items, individually small. |
| **Localization reach** | REA §A17, §B8 | Dictionary has 365 entries, 2 languages. Expand to 12+ if needed. |

---

## Closed Gaps (Since Last Audit)

| Gap | Commit | Notes |
|---|---|---|
| **Task 4.2 — Content element arrange** | `e21c6f6` | Drag-to-move, corner resize, free-angle rotation, z-order. |
| **Task 12.2 — Compare view modes** | `c37981a` | Side-by-side, overlay blend, per-page highlights. |
| **Task 14.2 — i18n wiring** | `f1a6f64` | 365-entry EN/FR dictionary, `t()` throughout UI. |
| **Digital signature window + cert manager** | — | `sign_ui.rs`: `sign_window`, `certificate_panel`, `validate_signatures`. |

---

## Verification Commands

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy -p youpdf-app -p youpdf-core --all-targets
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

---

## Notes on REA Evidence

The REA analysis (`docs/rea/UPDF_FEATURE_INVENTORY.md`) was **static-only** — no decompiler was available (Ghidra/Hopper/IDA not configured). Every claim rests on strings, symbols, class names, menu structures, and file paths — i.e., on *what the app names*, not on *what it does*. The localisation is shared across platforms (macOS/Windows/iOS), so some strings describe Windows-only features ("Enable PDF thumbnail previews in Windows Explorer", "Tap with Three Fingers"). Treat those as *inferred* for macOS.

The inventory may **under-count** UPDF, never over-count it. A feature that exists in UPDF but whose evidence we could not reach would show up as **absent from §(a)** of the inventory.
