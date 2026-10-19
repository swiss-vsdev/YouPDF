# YouPDF — UPDF Feature Parity Gap List

This document tracks the feature gap between YouPDF and UPDF.app v2.5.6, based on the audit of the UPDF.app bundle.

## Implementation progress

Cross-referenced with `docs/UPDF_IMPLEMENTATION_PLAN.md`. Gaps closed since the audit:

- **Phase 0.1** — Removed dead dependencies (`docx-rs`, `convert-docx` feature, `futures`, `uuid` from `youpdf-ai`); deleted `convert/docx.rs`, the `Converter` trait and `ConversionReport`.
- **Phase 0.2** — Added `youpdf-core/src/util.rs` (zlib compress/decompress, XML/CSV escaping) and routed `watermark`, `create`, `pdfa`, `office`, `forms`, `convert` through it.
- **Phase 0.3** — Added `YouPdfApp::show_modal` (centered, chrome_bg frame, 10 pt rounding) and converted 15 centered dialogs to it. The four resizable surfaces (organize, compare, extract, batch) and the activity popover keep their own window.

Unchecked below = still open. The plan document is the authoritative task list.

- **Phase 1.1** — Added the Home screen (`crates/youpdf-app/src/home.rs` + `home_screen`/`home_grid`/`home_card`/`home_list`): search filter, Recents / Starred / Tools tabs, grid and list views, page-1 thumbnails (cached), star toggle persisted to `~/.youpdf/starred.txt`, and the card context menu (open, open in new tab, star, reveal, remove from recents, delete).
- **Phase 1.2** — Tab bar: auto-width tabs (120–220 pt), close button on active/hover, middle-click close, drag-to-reorder with an insertion bar and active-tab tracking, and a right-click menu (close, close others/right/all, pin, duplicate, reveal, copy path). A pinned tab keeps its position.
- **Phase 1.3** — Docked bottom status bar (26 pt) replacing the floating zoom overlay: first/prev/next/last page, a page-number field, a layout picker, zoom out / preset / zoom in, a slideshow button, and a right cluster with the displayed page size, the render engine and a lock icon for encrypted files.
- **Phase 2.1** — The left rail already carried the eight tools with tooltips; the content side panel is now resizable (drag handle, 180–450 pt) and every panel sizes itself to the current width. The floating toolbars follow it.
- **Phase 2.2** — Search panel already had the action row (search / replace / delete / redact), the history list and the result list; the filter popover now also offers *Include bookmarks* and *Include comments*, which add synthetic results (bookmark titles and comment text) on top of the page-text matches.
- **Phase 2.3** — Thumbnail sidebar: click / ⌘-click / ⇧-click multi-selection, drag-and-drop page reordering (the page object itself moves, so outline entries, destinations and links keep pointing at it), a right-click menu (rotate CW/CCW, insert blank page after… with a size submenu, insert from file…, extract selected pages…, duplicate pages, page properties…, delete pages) and a bottom slider zooming thumbnails from 80 to 240 pt wide. Core gained `insert_blank_page_after`, `duplicate_pages`, `move_page` and `page_boxes`; `insert_kids` now descends through every subtree before treating an anchor as missing.
- **Phase 2.4** — Bookmarks sidebar: the outline is now read as a nested tree (`outline_tree`) and written back as one (`write_outline`, with `OutlineTarget` keeping the original `/Dest` or `/A` so a rename or a move never resets a view position). The panel has per-node and expand/collapse-all chevrons, double-click inline rename, a right-click menu (add bookmark, add sub-bookmark, set destination to this page, rename, delete), drag-and-drop to re-parent or re-order (right edge = become a child, elsewhere = become the next sibling) and an **Auto-Generate** button. Rewriting the outline frees the previous bookmark objects instead of orphaning them (mark-and-sweep from the PDF roots, so a bookmark still targeted by a named destination or a link survives), and a bookmark whose page has since been deleted loses its stale destination rather than keeping a dead reference.
- **Phase 2.5** — Comments sidebar: live search, filters by type, author, colour and review state (all / unchecked / accepted / rejected / cancelled / completed) and sorting by page, date or author. Each card shows the author's initial, the relative date, the type, the page badge, the quoted page text under a markup annotation (`Document::text_in_rect`), the comment body and its reply thread, with nested replies and a **Reply…** input (Enter or **Send**). The review state is stored as `/State` + `/StateModel Review`, replies as `/IRT` + `/RT R` pointing at the parent object. Deleting a comment deletes its replies too. **Export Comments** writes the filtered, sorted list as CSV, Markdown, XFDF or FDF (rectangles are converted back to page space, and dates to the PDF `D:…` form). The list shows YouPDF's own annotations: comments written by other tools are preserved untouched, not listed, because only those with a `youpdf-` name can be edited and rewritten.

- **Phase 2.6** — Attachments sidebar: files embedded in `/Names /EmbeddedFiles` are listed with their name, size and description; **Add Attachment** embeds any file (a `/Filespec` plus its `/EF` stream, replacing an entry with the same name), double-click or the right-click menu opens one in the system default application through a private temp directory (unique name plus `O_EXCL`, and file names are sanitised, so a name or a pre-planted symlink cannot escape it), **Save As…** writes it out, and **Delete** removes the entry along with its filespec and stream objects. Core gained `add_attachment` / `remove_attachment` over a single `embedded_file_entries` walk that also accepts inline filespec dictionaries and streams shared by several descriptors, so the list, the extraction and the removal always agree on the index; the mark-and-sweep that already cleaned replaced bookmarks now also reclaims replaced attachments and trees (`purge_unreachable`).
- **Phase 2.7** — Aspect panel: canvas background follows the reference set — *Auto* (the application theme) plus dark `#303030`, light `#ECECEE`, mint `#DFEEE7` and starry blue `#080A18` — page display tints (none, light green, parchment, sepia, dark night, rose, sky blue) are painted as a translucent layer over the rendered page, with a preview swatch per tint in the panel, plus a 1 pt page border toggle next to the page-shadow toggle and the thumbnail scale slider. The tint colours the view only: the document itself is untouched.
- **Phase 3.1** — Mode pill: `Ctrl+1`…`Ctrl+4` open Comment, Edit, Form and Redact, the floating toolbar stays centered above the canvas with the reference tool sets, and `Close` leaves for Reader Mode (no mode = text selection stays live on the canvas).
- **Phase 3.2** — Floating attributes inspector (`crates/youpdf-app/src/inspector.rs`): `InspectorTarget` covers a text selection, a text block, an image, an annotation and a form field, each with its screen bounds. `place_inspector` centres the panel over the target, clamps it inside the window (shrinking it when the window is narrower), and flips it *below* the target with an upward arrow when the element sits against the top edge; the panel measures its own content, so a long author name cannot spill past its background. Wired today to a selected annotation (mode Comment, with page, author, delete and close) and to the text block being rewritten and to a dragged text selection; the text and image control rows themselves are tasks 3.3 and 3.4.
- **Phase 3.3** — In-place text inspector: the floating panel over a text block being rewritten carries font family (Helvetica, Times, Courier), font size, font colour (24-swatch quick palette plus `#RRGGBB` entry), bold, italic, underline, strikethrough, alignment (including a real justify: words are spread across the box, not letters), line spacing and character spacing. The controls edit `TextStyle` on the draft, the canvas previews size, colour, Courier and the two rules live, and the commit writes the style into the PDF: a per-variant base-14 font resource (Helvetica keeps the historical `YouPdfF1` name, and a document that already carries that name on another face gets a suffixed one instead of an overwrite), `Tc` only when character spacing is set, one `BT…ET` per line with the mask grown to cover the extra lines, and underline/strikeout drawn as rules. Widths used for centring are estimated from per-family average glyph widths: the standard-14 metric tables are not in the file, so alignment is close but not exact.
- **Phase 3.4** — In-place image inspector: in Edit mode the Select tool picks the image under the pointer (hit-tested through its display box, so rotated pages work) and the floating panel offers Replace (new file, same object so the resource name and drawing matrix are untouched), Extract (original pixels as PNG), Rotate 90° CW/CCW, Flip horizontal/vertical, Crop with four draggable handles drawn over the picture, an opacity slider and Delete. Rotations and flips rewrite only the `cm` that draws the image, about the centre of its box, so they are lossless and instant; the crop trims the pixels and shrinks the box by the same amount, taking its side fractions from the drawing matrix so a crop after a rotation still removes what is displayed. Opacity uses a per-image `/ExtGState` (`/ca` + `/CA`) reused across slider moves. Deleting removes the whole `q … cm … Q` group and unbinds the `/XObject` name. A page that inherits its `/Resources` now gets the full inherited copy before any of this writes, and `insert_image_bitmap` picks a free `SigIm n` name so two images on one page stay separately editable.

## 1. Window Chrome & Layout
*   **UPDF:** Tabbed interface with `+` new tab, Home house button, traffic lights, avatar, notifications.
*   **YouPDF:** Basic tab support, no Home/start screen, no notifications, no cloud/account/avatar integration.
*   **Action:** Implement `Home` screen (recents grid, file browser UI) and notification hub stub; replace traffic light inset with system-standard or custom-integrated titlebar.

## 2. Mode Toolbars & Inspectors
*   **UPDF:** Context-aware floating pill inspectors (`TextOptionsView`, `AttributesInspector`) for in-place editing.
*   **YouPDF:** Simple toolbar, mostly modal windows or panel toggles.
*   **Action:** Implement floating, non-modal `FloatingInspector` that binds to selected elements, supporting `Font`, `Font Style`, `Font Size`, `Color` (with picker), `Alignment`, and style attributes (Bold/Italic/etc).

## 3. Left Rail & Side Panels
*   **UPDF:** 8-tool rail (Organize, Search, Thumbnails, Bookmarks, Comments, Forms, Attachments, Appearance).
*   **YouPDF:** Mostly implemented, but `Appearance` panel is a toggle, not a full settings/palette picker.
*   **Action:** Refine `Appearance` panel to map UPDF’s background/theme setting (Themes: Light, Dark, Mint, Starry, Auto; Backgrounds: 9 colors).

## 4. File & Batch Operations
*   **UPDF:** `Batch Processing` tool (Convert, OCR, Combine, Insert, Create, Watermark, Background, Header/Footer, Bates, Compress, Encrypt, Remove, Print).
*   **YouPDF:** Basic batch interface implemented; lacks comprehensive sub-tool coverage.
*   **Action:** Extend batch infrastructure to support all UPDF batch operations, specifically adding `Combine` (multi-PDF), `Insert`, `Remove`, and `Print`.

## 5. Advanced Tools
*   **OCR:** UPDF offers 38 languages, 3 output modes (Editable, Searchable, Text/Pictures).
*   **YouPDF:** Tesseract-based, searchable-only.
*   **Action:** Upgrade OCR engine to support all 3 modes and provide UI for language selection; enable `PDF/UA` tag generation.
*   **Forms:** UPDF provides properties editors (name, tooltip, required, read-only).
*   **YouPDF:** Basic creation; lacks property editor.
*   **Action:** Create `FormFieldInspector` for `PropertiesViews` (General, Appearance, Options, Actions, Calculate, Format).
*   **Compression:** UPDF has quality grades (Screen 72dpi, Ebook 150dpi, Print 300dpi).
*   **YouPDF:** Minimalist deflating.
*   **Action:** Implement image resampling (Lanczos) for the three grades.
*   **Protection:** UPDF supports 128/256 AES, owner/user passwords.
*   **YouPDF:** AES implementation present.
*   **Action:** Add UI for `EncryptionLevel` selector and permission toggles (Print, Copy, etc.).

## 6. AI Integration
*   **UPDF:** Integrated chat, summarize, translate, explain, legal/financial review.
*   **YouPDF:** Provider architecture implemented; AI tasks (chat, summarize, translate, etc.) need prompt implementation.
*   **Action:** Complete the prompt engineering for `LegalReview` and `FinancialReview` modes in `crates/youpdf-ai/src/tasks.rs`.

## 7. Missing UI Parity
*   **Slideshow:** Needs implementing (F5 triggered).
*   **Measure Tools:** Needs `Perimeter`/`Area` markup rendering.
*   **Export Formats:** Needs DOCX/XLSX/PPTX writer implementations.
*   **Localization:** Needs complete i18n framework (replace all literals).
