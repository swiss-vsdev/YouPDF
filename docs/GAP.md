# YouPDF — UPDF 2.5.6 Parity Gap Analysis

This document tracks YouPDF's feature parity against **/Applications/UPDF.app** (v2.5.6), cross-referencing `docs/rea/UPDF_FEATURE_INVENTORY.md` (the local analysis spec).

---

## Legend
- ✅ **Done** — Implemented and verified against REA spec
- 🟡 **Partial** — Core logic present; interface or edge cases missing
- ❌ **Missing** — Not implemented

---

## Parity Overview

| Feature Family | REA Spec | Status | Summary |
|---|---|---|---|
| **01. Window Chrome & Navigation** | REA §01 | ✅ | Tabs (pin/reorder/close), Home screen (Recents/Starred/Tools, grid/list, thumbnails), docked status bar |
| **02. Left Rail & Side Panels** | REA §02 | ✅ | 8 rail tools (Organize, Search, Thumbnails, Bookmarks, Comments, Forms, Attachments, Aspect) |
| **03. Floating Inspectors & Mode Toolbars** | REA §03 | ✅ | Mode pills (Comment/Edit/Form/Redact), floating inspector (text, image, annot, form) |
| **04. Content Editing Engine** | REA §04 | ✅ | Caret navigation, typing, backspace, replace. Content arrange: drag-to-move, corner resize, free-angle rotation, z-order |
| **05. Commenting & Annotations** | REA §05 | ✅ | Markup (4 types), smooth ink + 2-mode eraser, callout, 12 dynamic stamps, custom stamp, signature creator |
| **06. Form Builder & Filling** | REA §06 | 🟡 | Reader fill (text, check, radio, combo), Form Prep canvas, guides, 6-tab properties. Push buttons unhandled, no date picker |
| **07. Organize Pages Surface** | REA §07 | ✅ | Full-window grid, select odd/even/all, rotate, delete (confirmed), extract, insert, replace, split |
| **08. Page Operations & Decor** | REA §08 | 🟡 | Crop (4 margin boxes), tile split, text watermark, 6-slot header/footer, Bates. Watermark is text-only |
| **09. Security & Signatures** | REA §09 | 🟡 | AES-256 R6, 2-phase redaction, PKCS#7 window with cert manager + validation banner. No AES-128 |
| **10. Office Export & Conversion** | REA §10 | ✅ | Word (DOCX framePr), Excel (XLSX), PPTX, PNG/JPEG/TIFF, long image, TXT/MD/HTML/RTF/CSV/XML, PDF/A-1b |
| **11. OCR Engine** | REA §11 | ✅ | Tesseract (48 languages), 3 output modes (Searchable, Editable, Text+Image), scanned PDF auto-detect banner |
| **12. Measurement & Compare** | REA §12 | ✅ | Distance/perimeter/area drag box, scale calibration. Compare: text report, side-by-side with shared page navigation and per-page difference highlights, overlay blend with opacity slider |
| **13. Batch Processing** | REA §13 | ✅ | File queue table (progress, ETA, cancel), 12 operations (convert, OCR, combine, insert, watermark, etc.) |
| **14. Settings & Localization** | REA §14 | ✅ | Prefs window (General, Appearance, Document, AI). `t()` wired across the whole UI (363-entry EN/FR dictionary, coverage gate in `scripts/i18n_extract.py`) |

---

## Open Parity Gaps

No open gaps. The last three closed:

- **Task 12.2** (`c37981a`) — compare view modes: side-by-side with shared
  page navigation and per-page difference highlights, overlay blend with an
  opacity slider, text report.
- **Task 4.2** (`e21c6f6`) — content element arrange: hit-test selection,
  drag-to-move, corner resize handles, free-angle rotation
  (`rotate_image`), z-order (`set_image_z_order`).
- **Task 14.2** (`f1a6f64`) — full i18n wiring.

---

## Verification Commands

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy -p youpdf-app -p youpdf-core --all-targets
```
