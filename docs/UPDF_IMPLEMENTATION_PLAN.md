# YouPDF — Full 1:1 UPDF Parity Master Implementation Plan

This document contains the step-by-step, granular specification for achieving 1-to-1 feature, UX, and behavioral parity with **/Applications/UPDF.app** (v2.5.6) in **YouPDF**.

Every task is designed to be an atomic, bounded unit of work suitable for autonomous implementation with zero guesswork: exact file paths, data structures, UI coordinates, behaviors, and verification tests are specified.

---

## Architecture Overview & Standards

- **Language & Harness**: Rust 2021, Edition 1.78+, egui 0.29 + eframe (glow backend), PDFium rendering.
- **Licensing & Clean-room**: 100% MIT/Apache-2.0 compatible clean-room implementation. No proprietary code, binaries, or licensed strings from UPDF. Replicate interaction design, UI logic, workflow patterns, and visual feedback.
- **Workspace Layout**:
  - `crates/youpdf-shared`: Common types, error types (`Error`, `Result`), design tokens (`Theme`, `Palette`).
  - `crates/youpdf-core`: Low-level PDF engine (parsing via `lopdf`, rasterization via PDFium C API / `pdfium-render`, OOXML serialization via `zip`, crypto, image processing).
  - `crates/youpdf-ai`: Bring-your-own AI layer (Ollama, OpenAI, Anthropic, Gemini) with OS keyring security.
  - `crates/youpdf-app`: Native desktop application UI, canvas compositor, floating inspectors, window management.

---

# Phase 0: Workspace Cleanup & Housekeeping

### Task 0.1: Remove dead dependencies and optional features
- **File**: `crates/youpdf-ai/Cargo.toml`
  - Remove `futures = { workspace = true }` and `uuid = { workspace = true }` from `[dependencies]`.
  - Move `tokio` to `[dev-dependencies]` since runtime is only used in tests and `youpdf-app` runs its own runtime.
- **File**: `crates/youpdf-core/Cargo.toml`
  - Remove optional dependency `docx-rs` and feature `convert-docx = ["dep:docx-rs"]`.
  - Delete file `crates/youpdf-core/src/convert/docx.rs`.
  - Remove `pub trait Converter` in `crates/youpdf-core/src/convert.rs` (replaced completely by `crates/youpdf-core/src/office.rs`).
- **File**: `crates/youpdf-core/src/lib.rs`
  - Remove exports of `docx` and `Converter`.
- **Verification**: Run `cargo check --workspace --all-targets` and verify clean compile.

### Task 0.2: Standardize shared helpers into a utility module
- **File**: `crates/youpdf-core/src/util.rs` (New file)
  - Create `pub fn flate_compress(data: &[u8]) -> Result<Vec<u8>>` using `flate2::write::ZlibEncoder`.
  - Create `pub fn flate_decompress(data: &[u8]) -> Result<Vec<u8>>` using `flate2::read::ZlibDecoder`.
  - Create `pub fn escape_xml(s: &str) -> String` and `pub fn unescape_xml(s: &str) -> String`.
  - Create `pub fn csv_escape(s: &str) -> String`.
- **Refactor**:
  - In `crates/youpdf-core/src/watermark.rs`, delete private `zlib_store` function and call `util::flate_compress`.
  - In `crates/youpdf-core/src/create.rs`, replace private `flate_compress` with `util::flate_compress`.
  - In `crates/youpdf-core/src/pdfa.rs`, replace private `flate_compress` with `util::flate_compress`.
  - In `crates/youpdf-core/src/office.rs`, replace private `escape_xml` with `util::escape_xml`.
  - In `crates/youpdf-core/src/forms.rs`, replace private `xml_escape` and `csv_field` with `util::escape_xml` and `util::csv_escape`.
  - In `crates/youpdf-core/src/convert.rs`, replace private `escape_xml` and `csv_field` with `util::escape_xml` and `util::csv_escape`.
- **Verification**: `cargo test -p youpdf-core` passes with no regressions.

### Task 0.3: Modal dialog boilerplate deduplication
- **File**: `crates/youpdf-app/src/app.rs`
  - Implement helper method on `YouPdfApp`:
    ```rust
    fn show_modal<R>(
        &mut self,
        ctx: &egui::Context,
        id_source: impl std::hash::Hash,
        title: &str,
        max_width: f32,
        open: &mut bool,
        add_contents: impl FnOnce(&mut Self, &mut egui::Ui) -> R,
    ) -> Option<R>
    ```
  - Configuration: `.collapsible(false)`, `.resizable(false)`, `.anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))`, frame styled with `pal().chrome_bg` and `Rounding::same(10.0)`.
  - Refactor all 21 modal dialogs (`compress_window`, `bates_window`, `watermark_window`, `crop_window`, `header_footer_window`, `protect_window`, `compare_window`, `ocr_window`, `create_window`, `measure_window`, etc.) to use `show_modal`.
- **Verification**: Verify each dialog opens centered with consistent background and padding, and pressing `Escape` or the close icon dismisses them properly.

---

# Phase 1: Window Chrome, Tab Management & Home Screen

### Task 1.1: Home / Start Screen (Recents & Templates Grid)
- **Files**: `crates/youpdf-app/src/home.rs` (New file), `crates/youpdf-app/src/app.rs`
- **State Structure**:
  ```rust
  pub struct HomeView {
      pub search_filter: String,
      pub selected_tab: HomeCategory, // Recents, Starred, Tools
      pub view_mode: GridOrList,      // Grid (cards) or List (table)
  }
  ```
- **UI Layout**:
  - Trigger: Home house button (`ph::HOUSE`) at top-left of the titlebar (`x: 12, y: 8, w: 28, h: 28`).
  - Top banner: Search bar ("Search file..."), "Open File" button (calls `rfd::FileDialog`), "Batch" button.
  - Recents Grid:
    - Card size: 160×200 pt with 8 pt rounding.
    - Top 130 pt: Rendered thumbnail of page 1.
    - Bottom 70 pt: File name (truncated), size (e.g. `2.4 MB`), modification relative time (e.g. `2 hours ago`), pin/star toggle icon.
  - Context menu on card: Open, Open in New Tab, Star / Favorite, Reveal in Finder, Remove from Recents, Delete File.
  - Drag and drop: Dropping a PDF onto the Home View opens it in a new tab.
- **Verification**: Opening app with no files displays Home screen. Clicking a recent document loads it and hides Home screen.

### Task 1.2: Complete Tab Bar & Contextual Actions
- **File**: `crates/youpdf-app/src/app.rs`
- **Requirements**:
  - `+` button at the right of the last tab: opens system file picker (or blank document if cancelled).
  - Tab item width: auto-sized between 120 pt and 220 pt, with favicon / PDF icon, filename label, and `✕` close button on hover or active.
  - Right-click context menu on tab header:
    - "Close Tab" (`Cmd+W`)
    - "Close Other Tabs"
    - "Close Tabs to the Right"
    - "Close All Tabs"
    - "Pin Tab"
    - "Duplicate Tab"
    - "Reveal in Finder" (`Shift+Cmd+J`)
    - "Copy File Path"
  - Middle-click on tab header closes the tab immediately (prompting for save if dirty).
  - Drag-and-drop tab reordering: visually slide tabs horizontally with egui pointer drag.
- **Verification**: Open 5 tabs. Reorder tab 1 to position 4. Right click tab 3 -> Close Tabs to the Right. Confirm tabs 4 and 5 close.

### Task 1.3: Document Status Bar & Viewport Controls
- **File**: `crates/youpdf-app/src/app.rs`
- **Bottom Status Bar**: Height 26 pt, docked to bottom of window, background `pal().chrome_bg`.
- **Components (Left to Right)**:
  - Navigation: First page (`|◀`), Previous page (`◀`), editable page input box `[ Page N ]`, total label `of M`, Next page (`▶`), Last page (`▶|`).
  - Divider (`|`).
  - View Layout Picker:
    - Single Page icon.
    - Continuous Scroll icon.
    - Two Pages icon.
    - Two Pages with Cover icon.
  - Divider (`|`).
  - Zoom controls: Zoom Out (`-`), Zoom Percentage text (e.g. `125%`), Zoom In (`+`), Zoom preset dropdown (Fit Width, Fit Page, Actual Size 100%, 50%, 75%, 150%, 200%, 400%).
  - Slideshow trigger icon (`ph::PROJECTOR_SCREEN` or `F5`).
  - Right cluster: Document physical dimensions (e.g. `210 × 297 mm` or `8.5 × 11 in`), Lock icon (if password protected / read-only).
- **Verification**: Toggling layout changes canvas arrangement instantly. Typing page number `5` and pressing Enter jumps smoothly to page 5.

---

# Phase 2: Complete Left Rail & Navigation Sidebars

### Task 2.1: Left Icon Rail Layout & Tooltips
- **File**: `crates/youpdf-app/src/app.rs`
- **Buttons (Top to Bottom, 52 pt width, icon 22 pt, tooltip on hover)**:
  1. `Organize Pages` (`ph::SQUARES_FOUR`) -> Switches main view to full Organize Pages canvas.
  2. `Search` (`ph::MAGNIFYING_GLASS`) -> Opens Search panel.
  3. `Thumbnails` (`ph::FILE_TEXT`) -> Opens Thumbnails panel.
  4. `Bookmarks` (`ph::BOOKMARK_SIMPLE`) -> Opens Bookmarks panel.
  5. `Comments` (`ph::CHAT_CIRCLE_DOTS`) -> Opens Comments list panel.
  6. `Form Fields` (`ph::LIST_CHECKS`) -> Opens Form fields list panel.
  7. `Attachments` (`ph::PAPERCLIP`) -> Opens Attachments panel.
  8. `Aspect & Display` (`ph::PALETTE` or `ph::CUBE`) -> Opens reading background & display settings.
- **Toggle behavior**: Clicking an active panel's rail icon collapses the panel. Width is resizable via a drag handle on the right border (min 180 pt, max 450 pt).

### Task 2.2: Search Sidebar (Search, Replace, Redact, History)
- **File**: `crates/youpdf-app/src/app.rs`
- **Header Tabs / Action Row**:
  - `Search Text`: Plain keyword search.
  - `Replace Text`: Find & Replace inputs.
  - `Delete Text`: Find & Delete (replaces match with empty space).
  - `Redact Matches`: Find & Mark for Redaction.
- **Filter Popover (Funnel icon)**:
  - Checkboxes: Match Case, Match Whole Words Only, Include Bookmarks, Include Comments.
- **History Section**:
  - List of recent search queries with count of occurrences.
  - `✕` button to remove individual history items; "Clear History" button.
- **Result List**:
  - Page header with badge count (e.g. "Page 3 (4 matches)").
  - Result row: highlighted snippet with bold matching text.
  - Clicking snippet centers viewport on the match with an animated highlight pulse.

### Task 2.3: Thumbnails Sidebar Context Menu & Page Reordering
- **File**: `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Direct drag-and-drop thumbnail reordering within the sidebar.
  - Right-click menu on single or multi-selected thumbnails:
    - Rotate Clockwise (`90°`)
    - Rotate Counter-Clockwise (`-90°`)
    - Insert Blank Page After...
    - Insert from File...
    - Extract Selected Pages...
    - Duplicate Pages
    - Delete Pages (`Backspace` / `Delete`)
    - Page Properties (dimensions, rotation, boxes).
  - Thumbnail bottom slider: zoom thumbnail size from 80 pt wide to 240 pt wide.

### Task 2.4: Bookmarks Sidebar (Tree View & Auto-Generation)
- **Files**: `crates/youpdf-core/src/structure.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Full tree visualization of PDF outline hierarchy (`/Outlines` and `/Kids`).
  - Expand / Collapse all chevron toggles.
  - Inline rename: double-click label to edit bookmark title in-place.
  - Right-click actions: Add Bookmark (pointing to current view), Add Sub-Bookmark, Set Destination, Delete Bookmark.
  - Drag and drop to re-parent or re-order bookmarks in the tree.
  - "Auto-Generate Bookmarks" button: calls `structure::generate_outline(&mut doc)` which detects headings via font size analysis and builds a structured outline.

### Task 2.5: Comments Sidebar (Filtering, Sorting & Threading)
- **Files**: `crates/youpdf-core/src/annotate.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - List every annotation on the document (Highlight, Underline, Strikeout, Squiggly, Note, FreeText, Ink, Shape, Stamp, Attachment).
  - Filter Bar: Filter by Type, Filter by Author, Filter by Color, Filter by Checked Status.
  - Sort Bar: Sort by Page, Sort by Date, Sort by Author.
  - Search input: Live filters comment text content.
  - Comment Card:
    - Author avatar/initial, author name, timestamp (relative/absolute).
    - Annotation icon + page number badge.
    - Quoted document text (for markup annotations).
    - User comment / note content.
    - Reply thread: nested reply list with "Reply..." text input box.
    - Status marker: None / Accepted / Rejected / Cancelled / Completed.
  - Top Action: "Export Comments" -> Exports comments list to FDF, XFDF, CSV, or formatted Markdown.

### Task 2.6: Attachments Sidebar
- **Files**: `crates/youpdf-core/src/document.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Extract and display all embedded files from the PDF `/EmbeddedFiles` name tree.
  - List item: File icon, File Name, File Size, Description.
  - Actions:
    - "Add Attachment" button (`rfd::FileDialog` to embed any file).
    - Double click or right-click "Open" -> saves file to system temp and opens in OS default app.
    - "Save As..." -> extracts attachment to user-selected folder.
    - "Delete" -> removes file from `/EmbeddedFiles` and cleans indirect objects.

### Task 2.7: Aspect & Display Sidebar
- **File**: `crates/youpdf-app/src/app.rs`
- **Settings**:
  - Canvas Background Theme: Dark (`#303030`), Light (`#ECECEE`), Mint (`#DFEEE7`), Starry Blue (`#080A18`).
  - Page Display Tints (eye protection): None (original white), Light Green, Parchment, Sepia, Dark Night, Rose, Sky Blue.
  - Page Shadow toggle: render subtle drop shadow around page rects.
  - Page Border toggle: 1 pt hairline border around pages.
  - Thumbnail scale slider.

---

# Phase 3: Mode Toolbars & Floating Attribute Inspectors

### Task 3.1: Mode Switching Pill & Clean Dismissal
- **File**: `crates/youpdf-app/src/app.rs`
- **Modes**:
  - Default: Reader Mode (no mode pill, clean canvas with text selection enabled).
  - Mode 0: `Comment` (Annotation tools) -> Keyboard shortcut `Ctrl+1`.
  - Mode 1: `Edit` (Text, Image, Link, Path editing) -> Keyboard shortcut `Ctrl+2`.
  - Mode 2: `Form` (Form preparation) -> Keyboard shortcut `Ctrl+3`.
  - Mode 3: `Redact` (Redaction and sanitization) -> Keyboard shortcut `Ctrl+4`.
- **Top Toolbar**:
  - Floating pill centered above canvas, rounded frame, shadow.
  - Rightmost button of every mode toolbar is always `Close` (`✕` icon + "Close"). Clicking exits to Reader Mode immediately.

### Task 3.2: Floating Attributes Inspector Component
- **Files**: `crates/youpdf-app/src/inspector.rs` (New file), `crates/youpdf-app/src/app.rs`
- **Data Model**:
  ```rust
  pub enum InspectorTarget {
      TextSelection { page: usize, range: TextRange, bounds: egui::Rect },
      TextElement { page: usize, id: TextBlockId, bounds: egui::Rect },
      ImageElement { page: usize, id: ImageId, bounds: egui::Rect },
      AnnotationElement { page: usize, id: AnnotationId, bounds: egui::Rect },
      FormFieldElement { page: usize, name: String, bounds: egui::Rect },
  }
  ```
- **Inspector Positioning**:
  - Compute rect directly above `target.bounds` in screen coordinates (`bounds.top() - 48.0 pt`).
  - Clamp to window viewport so inspector never clips outside top, left, or right edges.
  - Draw triangle arrow pointing down towards selected element.

### Task 3.3: In-Place Text Inspector (Edit Mode)
- **File**: `crates/youpdf-app/src/inspector.rs`
- **Controls (Single horizontal compact row)**:
  - Font Family Dropdown: System font list with preview.
  - Font Size: DragValue or dropdown with sizes `8, 9, 10, 11, 12, 14, 16, 18, 24, 36, 48, 72`.
  - Font Color: Circle swatch button opening 24-color quick palette + RGB/HEX color picker.
  - Style toggles: Bold (`B`), Italic (`I`), Underline (`U`), Strikethrough (`S`).
  - Alignment: Left, Center, Right, Justify segment control.
  - Line Spacing dropdown (`1.0`, `1.15`, `1.5`, `2.0`, `Custom`).
  - Character Spacing input.

### Task 3.4: In-Place Image Inspector (Edit Mode)
- **File**: `crates/youpdf-app/src/inspector.rs`
- **Controls**:
  - "Replace Image..." (`ph::ARROWS_CLOCKWISE`): pick new file, updates stream in-place preserving transform matrix.
  - "Extract Image..." (`ph::EXPORT`): exports original PNG/JPEG to disk.
  - "Rotate": Rotate 90° CW, Rotate 90° CCW.
  - "Flip": Flip Horizontal, Flip Vertical.
  - "Crop Image": opens modal crop handle directly over image on canvas.
  - "Opacity": 0% to 100% slider.
  - "Delete": deletes image XObject reference from page `/Resources`.

---

# Phase 4: In-Place Text & Content Editing Engine

### Task 4.1: Precise Text Caret Navigation & Character Editing
- **Files**: `crates/youpdf-core/src/edit.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Clicking inside a text block enters in-place text editing:
    - Flashing text cursor (caret) placed between glyphs based on mouse X coordinate.
    - Left / Right arrows move caret by character; Option+Left / Option+Right move by word; Cmd+Left / Cmd+Right jump to line start/end.
    - Shift+Arrows creates a character-level selection range.
    - Typing inserts characters at caret position.
    - Backspace and Delete remove characters.
  - Live preview:
    - Paint opaque mask over the original text bounding box sampled from surrounding page background color.
    - Render updated text in real-time matching original font metrics, color, and size.
  - Commit lifecycle:
    - Pressing Escape cancels changes and restores original text.
    - Clicking outside or pressing Enter (if single line) commits the modified text to the PDF content stream via `edit::replace_text_fragment`.
- **Verification**: Open a PDF with text "Invoice #12345". Click between "4" and "5", press Backspace, type "9", click outside. Verify text is now "Invoice #12395" and saves correctly.

### Task 4.2: Content Element Selection, Movement & Resizing
- **Files**: `crates/youpdf-core/src/edit.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - In Edit Mode, hovering over any text block or image highlights its bounding box with a 1 pt blue dashed border.
  - Clicking selects the element and displays 8 resize handles (4 corners, 4 midpoints) and a rotation handle at top center.
  - Dragging the element moves its position on the page (updating coordinates in PDF content stream or XObject matrix).
  - Dragging a corner handle scales the element proportionally (Shift-lock) or freely.
  - Undo/Redo stack records all moves, resizes, and content edits.

---

# Phase 5: Annotation Tools (Comment Mode)

### Task 5.1: Text Markup Tools (Highlight, Underline, Strikeout, Squiggly)
- **Files**: `crates/youpdf-core/src/annotate.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Select text via primary mouse drag:
    - If tool is Highlight, creates `/Highlight` annotation with `/QuadPoints` aligned to text line rects.
    - If tool is Underline, creates `/Underline` annotation.
    - If tool is Strikeout, creates `/StrikeOut` annotation.
    - If tool is Squiggly, creates `/Squiggly` annotation with sinusoidal wave appearance.
  - Floating palette on tool click: color swatches (Yellow, Green, Pink, Orange, Blue, Purple) and opacity slider.

### Task 5.2: Freehand Drawing (Ink / Pencil) & Eraser
- **Files**: `crates/youpdf-core/src/annotate.rs`, `crates/youpdf-app/src/app.rs`
- **Pencil Tool**:
  - Continuous drag captures path points `[(x0, y0), (x1, y1), ...]`.
  - Smooth points using Catmull-Rom or Bézier spline interpolation to eliminate jitter.
  - Color, stroke thickness (0.5 pt to 12 pt), opacity controls.
  - Writes `/Ink` annotation with `/InkList` array of paths.
- **Eraser Tool**:
  - Two modes selectable via dropdown:
    1. Stroke Eraser: clicking any part of an ink stroke deletes the entire stroke.
    2. Segment Eraser: dragging cuts intersecting sections out of the ink path.

### Task 5.3: Shapes (Rectangle, Circle, Line, Arrow, Polygon, Polyline)
- **Files**: `crates/youpdf-core/src/annotate.rs`, `crates/youpdf-app/src/app.rs`
- **Shape Tools**:
  - Rectangle (`/Square`), Circle/Oval (`/Circle`), Line (`/Line`), Arrow (`/Line` with `/LE [ /None /OpenArrow ]`).
  - Polygon: click points sequentially, double click to close loop.
  - Polyline: click points sequentially, double click to end open path.
- **Style Controls**:
  - Stroke Color, Fill Color (supports transparent / no-fill).
  - Stroke Thickness, Stroke Style: Solid, Dashed, Dotted.
  - Cloud effect option: applies scalloped border appearance (cloudy style) to polygons and rectangles.

### Task 5.4: Sticky Note & FreeText Callout
- **Files**: `crates/youpdf-core/src/annotate.rs`, `crates/youpdf-app/src/app.rs`
- **Sticky Note (`/Text`)**:
  - Click on page places note icon (`Comment`, `Key`, `Note`, `Help`, `Paragraph`).
  - Double clicking opens popup editor window with author, date, and multiline text editor.
- **Typewriter / FreeText (`/FreeText`)**:
  - Click to place text box directly on page content.
  - Type arbitrary text; font size, font color, background fill color customizable.
- **Callout Tool (`/FreeText` with `/IT /FreeTextCallout`)**:
  - 3-point click placement: Point 1 (arrow tip), Point 2 (knee point), Point 3 (text box corner).
  - Renders arrow line connected to a bordered text box.

### Task 5.5: Stamps & Visual Signatures
- **Files**: `crates/youpdf-core/src/annotate.rs`, `crates/youpdf-core/src/signature.rs`, `crates/youpdf-app/src/app.rs`
- **Built-in Dynamic Stamps**:
  - Stamps: `Approved`, `Not Approved`, `Draft`, `Final`, `Confidential`, `Sign Here`, `Initial Here`, `Witness`.
  - Dynamic stamps automatically stamp current date, time, and user name.
- **Custom Stamp Dialog**:
  - Import Image (PNG/JPEG) or create Text Stamp (custom text, border color, date toggle).
- **Visual Signature Creator**:
  - Modal with 3 creation tabs:
    1. `Draw`: interactive trackpad / mouse drawing canvas with stroke smoothing and thickness slider.
    2. `Type`: type your name, select from 5 script / handwriting cursive fonts.
    3. `Image`: upload scan/photo of handwritten signature; auto-remove background option (turns white pixels transparent).
  - Stored signatures list: save up to 10 signatures for 1-click reuse across documents.

---

# Phase 6: Form Builder & Form Filling

### Task 6.1: Interactive Form Filling (Reader Mode)
- **Files**: `crates/youpdf-core/src/forms.rs`, `crates/youpdf-app/src/app.rs`
- **Field Interaction**:
  - Text Field: Click to focus, caret navigation, type text. Multiline wraps properly. Password field masks with bullets (`●`).
  - Checkbox: Click toggles state between `/Yes` and `/Off`.
  - Radio Button: Click sets selected value in group, deselecting all other radios with the same field name.
  - Dropdown (`/Choice`): Click opens dropdown list popup, select item updates value.
  - List Box: Click to select, Cmd/Ctrl+Click for multi-selection.
  - Push Button: Visual press feedback; triggers configured action (e.g. Reset Form, Submit, JavaScript).
  - Date Picker: Date field shows calendar icon; clicking opens date picker grid.
  - Field Highlighting toggle: blue tint on all fillable form fields so user can locate them instantly.

### Task 6.2: Form Preparation Mode (Form Field Builder)
- **Files**: `crates/youpdf-core/src/forms.rs`, `crates/youpdf-app/src/app.rs`
- **Tool Palette**:
  - Text Field, Check Box, Radio Button, Dropdown, List Box, Button, Date Field, Barcode Field, Signature Field.
- **Canvas Drag-to-Place**:
  - Drag mouse on canvas to draw bounding box.
  - Automatic alignment guides: red alignment guidelines appear when field edges or centers align with nearby fields.
  - Multi-selection of fields: Shift-drag selection rectangle to select multiple fields.
  - Alignment tools in top bar: Align Left, Align Right, Align Top, Align Bottom, Distribute Horizontally, Distribute Vertically, Match Width, Match Height.

### Task 6.3: Comprehensive Form Field Properties Inspector
- **File**: `crates/youpdf-app/src/app.rs`
- **Multi-Tab Properties Modal**:
  - **Tab 1: General**:
    - Field Name (text edit, validates no duplicate names for different field types).
    - Tooltip (text edit).
    - Common Properties: Visible / Hidden / Visible but doesn't print / Hidden but printable.
    - Read Only checkbox.
    - Required field checkbox.
  - **Tab 2: Appearance**:
    - Border Color, Border Thickness (Thin, Medium, Thick), Border Style (Solid, Dashed, Beveled, Inset, Underline).
    - Fill Color.
    - Text Font (Helvetica, Courier, Times, etc.), Font Size (Auto or 6–72 pt), Text Color.
  - **Tab 3: Options (Contextual by Field Kind)**:
    - Text Field: Alignment (Left, Center, Right), Default Value, Multiline, Password, Scroll long text, Comb of N characters (draws character comb cells).
    - Checkbox / Radio: Button style (Check `✓`, Circle `●`, Cross `✕`, Diamond `◆`, Square `■`, Star `★`), Export Value, Checked by default.
    - Dropdown / List: Item name + Export value list builder; Add, Delete, Up, Down buttons; Sort items alphabetically checkbox; Allow user to enter custom text.
  - **Tab 4: Actions**:
    - Select Trigger: Mouse Up, Mouse Down, Mouse Enter, Mouse Exit, On Focus, On Blur.
    - Select Action: Reset a form, Run JavaScript, Go to page view, Open a web link.
  - **Tab 5: Format**:
    - Category: None, Number (decimals, separator, currency symbol), Percentage, Date (format picker e.g. `YYYY-MM-DD`, `DD/MM/YYYY`), Time, Special (Zip code, Phone number, SSN).
  - **Tab 6: Calculate**:
    - Predefined calculations: Value is the Sum (`+`), Product (`×`), Average, Minimum, Maximum of specified fields.

### Task 6.4: Form Data Import & Export
- **File**: `crates/youpdf-core/src/forms.rs`
- **File Formats**:
  - FDF (Forms Data Format) binary/text stream parsing and generation.
  - XFDF (XML Forms Data Format) parsing and generation.
  - CSV (comma-separated field name / value pairs).
  - JSON (structured dictionary of `{ "FieldName": "Value" }`).
- **Actions in Forms Panel**:
  - "Import Form Data..." -> loads matching fields into document.
  - "Export Form Data..." -> exports filled values.
  - "Reset Form" -> clears all fields to default values.

---

# Phase 7: Full-Window Page Organization Surface

### Task 7.1: Organize Pages Full-Screen Grid View
- **File**: `crates/youpdf-app/src/app.rs`
- **Layout & Interactions**:
  - Dedicated full-window view replacing document canvas when `Organize Pages` is clicked.
  - Top Toolbar:
    - Selection indicator: `X of Y pages selected`.
    - "Select All" (`Cmd+A`), "Deselect All".
    - "Select Odd Pages", "Select Even Pages".
    - "Rotate Left" (`-90°`), "Rotate Right" (`+90°`).
    - "Delete" (`Delete` / `Backspace`) with confirmation dialog.
    - "Extract...", "Insert...", "Replace...", "Split...", "Duplicate".
    - Close button (`✕ Close`) returns to normal view.
  - Grid Viewport:
    - Responsive thumbnail cards with page number beneath each.
    - Selection badge on top-left of each card.
    - Hovering card displays quick-action overlay: Rotate icon, Delete icon.
    - Multi-select via Click, Shift+Click (range), Cmd/Ctrl+Click (disjoint).
    - Inter-cell insertion indicator: dragging a thumbnail displays vertical insertion bar between destination cards; dropping reorders pages.

### Task 7.2: Organize Pages Action Dialogs
- **File**: `crates/youpdf-app/src/app.rs`
- **Extract Dialog**:
  - Option 1: Extract pages as a single PDF.
  - Option 2: Extract each page into separate PDF files.
  - Checkbox: "Delete original pages after extraction".
  - Destination folder picker.
- **Insert Dialog**:
  - Source: Blank Page, From File (PDF, DOCX, Image), From Clipboard.
  - Insertion location: Before Page N, After Page N, First Page, Last Page.
  - For image insertion: Option to fit page to image size or fit image into standard A4/Letter page.
- **Replace Dialog**:
  - "Replace pages [Start] to [End] with [Select File...] pages [Start] to [End]".
- **Split Dialog**:
  - Split by page count (e.g., split every 2 pages).
  - Split by maximum file size (MB).
  - Split by top-level bookmarks.

---

# Phase 8: Page Operations & Transformations

### Task 8.1: Crop Pages Tool
- **Files**: `crates/youpdf-core/src/crop.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Visual crop rectangle over page with draggable corner and edge handles.
  - Margin control inputs in inspector: Top, Bottom, Left, Right (supports mm, cm, in, pt).
  - Page Box target selector: CropBox, ArtBox, TrimBox, BleedBox.
  - Page Range selector: Current Page, All Pages, Odd Only, Even Only, Custom Range (e.g. `1-5, 8, 11-14`).
  - "Auto-Crop / Remove White Margins": scans page content bounds and automatically snugs CropBox to visible text and graphics.
  - "Reset Crop": restores CropBox to MediaBox dimensions.

### Task 8.2: Split Pages (Tile-Based Visual Splitting)
- **Files**: `crates/youpdf-core/src/crop.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Interactive split lines over the page:
    - Add Horizontal Split Line.
    - Add Vertical Split Line.
    - Grid Split (e.g. 2×2 grid to turn 1 poster page into 4 standard pages).
  - Preview split pieces in miniature tiles.
  - On Apply: creates N new pages in the document corresponding to each split tile rectangle, adjusting MediaBox/CropBox and content stream translations.

### Task 8.3: Watermark & Background Engine
- **Files**: `crates/youpdf-core/src/watermark.rs`, `crates/youpdf-app/src/app.rs`
- **Watermark Capabilities**:
  - Source: Text Watermark or File Watermark (PDF or Image).
  - Text controls: text input, font picker, font size, text color.
  - Position: 3×3 placement matrix (Top-Left, Top-Center, Top-Right, Center-Left, Center, Center-Right, Bottom-Left, Bottom-Center, Bottom-Right) + X and Y fine offset steppers.
  - Rotation: Preset (Diagonal 45°, Horizontal 0°, Vertical 90°) or arbitrary angle slider (-180° to +180°).
  - Tiling mode: "Tile across entire page" with customizable tile scale and tile spacing.
  - Layering: "Behind page content" or "Above page content".
  - Opacity slider: 0% to 100%.
  - Save as reusable Watermark Preset.
  - "Remove Watermark": detects and removes existing watermarks added by UPDF, Acrobat, or YouPDF.
- **Background Capabilities**:
  - Background Color (color picker) or Background Image/PDF.
  - Scaling: Fit to Page, Actual Size, Custom Scale %.
  - Page range target.

### Task 8.4: Header & Footer and Bates Numbering
- **Files**: `crates/youpdf-core/src/watermark.rs`, `crates/youpdf-app/src/app.rs`
- **Header & Footer**:
  - 6 independent slots: Left Header, Center Header, Right Header, Left Footer, Center Footer, Right Footer.
  - Macro inserters: `<<Page Number>>`, `<<Page Count>>`, `<<Current Date>>`, `<<File Name>>`.
  - Date formats: `YYYY-MM-DD`, `DD/MM/YYYY`, `MM/DD/YYYY`, `Month DD, YYYY`.
  - Page number formats: `1`, `1 / n`, `1 of n`, `Page 1`, `Page 1 of n`, `i, ii, iii`.
  - Margin adjustments: Top, Bottom, Left, Right margins (pt/mm).
  - Font family, size, color styling.
  - Option: "Different First Page" (skip header/footer on cover page).
- **Bates Numbering (Legal Discovery)**:
  - Prefix input (e.g. `CASE-`), Suffix input (e.g. `-CONF`).
  - Start Number (e.g. `1`), Number of Digits (e.g. `6` -> `CASE-000001-CONF`).
  - Position selection on page, font size, color.

---

# Phase 9: Security, Permissions & Digital Signatures

### Task 9.1: Password Encryption & Permissions Control
- **Files**: `crates/youpdf-core/src/protect.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - **Document Open Password**:
    - Require password to open and view the document.
    - Encryption Level: AES-256 (recommended) or AES-128.
  - **Permissions Password (Master / Owner Password)**:
    - Restrict editing and printing.
    - Printing Permission: None, Low Resolution (150 dpi), High Resolution.
    - Changes Allowed: None, Page insertion/deletion/rotation, Filling form fields and signing, Commenting and form filling, Any except extracting pages.
    - Enable text/graphic copying checkbox.
    - Enable screen readers / accessibility checkbox.
  - **Decryption & Security Removal**:
    - Prompt for password upon opening protected document.
    - "Remove Password / Security" command (requires entering permissions password if set).

### Task 9.2: Redaction (Two-Phase Destructive Engine)
- **Files**: `crates/youpdf-core/src/redact.rs`, `crates/youpdf-app/src/app.rs`
- **Workflow**:
  - Phase 1: Mark for Redaction:
    - User drags rectangles over sensitive areas or uses Search & Redact.
    - Rendered with translucent red fill and dark red border.
    - Hovering marked area shows black fill preview ("What it will look like").
    - Redaction properties: Fill color (Black, White, Custom), Overlay text (e.g. `[REDACTED]`, `CONFIDENTIAL`), text font and color.
  - Phase 2: Apply Redaction:
    - Warning modal: *"Applying redactions permanently destroys underlying text and images. This cannot be undone. Continue?"*
    - Core engine:
      - Intersects redaction rects with text characters and path vectors, permanently deleting them from content streams.
      - Intersects with images: burns black/white pixels into image bitmaps, recompressing them so no pixel underneath can be recovered.
      - Rasterizes complex vector pages if content cannot be cleanly sliced.
      - Removes overlapping annotations.
- **Document Sanitization**:
  - Action "Sanitize Document": Strips document metadata, hidden text, embedded files, bookmarks, comments, form field actions, and deleted object revisions.

### Task 9.3: Digital Signatures & Certificates (PAdES / PKCS#7)
- **Files**: `crates/youpdf-core/src/signature.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Digital Certificate Manager:
    - Import PKCS#12 certificate (`.p12` / `.pfx`) with password unlock.
    - Create Self-Signed Certificate: Name, Organization, Email, Country, Validity Period (years).
  - Sign Document:
    - Place digital signature field on canvas.
    - Select certificate from manager.
    - Signature Appearance: Name, Date/Time, Reason, Location, DN info + optional visual signature graphic.
    - Hash document with SHA-256, encrypt with private key, embed PKCS#7 signature dictionary into `/ByteRange` stream.
  - Validation:
    - Open signed document displays notification bar: *"All digital signatures are valid"* (Green) or *"Signature was modified after signing"* (Red).
    - Certificate details dialog showing validity, issuer, and timestamp.

---

# Phase 10: High-Fidelity Conversion & Office Export

### Task 10.1: Word (.docx) High-Fidelity Export
- **Files**: `crates/youpdf-core/src/office.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Reconstruct page layout into Microsoft Word OpenXML (`word/document.xml` in `.docx` ZIP).
  - Two conversion modes:
    1. Flow Layout: groups text fragments into paragraphs, lines, and heading styles for editable flow.
    2. Exact Layout: places text runs in absolute-positioned Word text frames (`w:txbxContent`) matching original PDF X/Y coordinates precisely.
  - Embed extracted images into `word/media/` and link via `word/_rels/document.xml.rels`.
  - Write styles into `word/styles.xml` and document relations.

### Task 10.2: Excel (.xlsx) & PowerPoint (.pptx) Export
- **Files**: `crates/youpdf-core/src/office.rs`, `crates/youpdf-app/src/app.rs`
- **Excel (.xlsx)**:
  - Table detection: identify column boundaries by vertical alignment of text runs and row boundaries by horizontal alignment.
  - Write cells into `xl/worksheets/sheet1.xml` with proper data types (numbers, dates, text strings).
  - Write shared strings into `xl/sharedStrings.xml`.
- **PowerPoint (.pptx)**:
  - Each PDF page converts to a PowerPoint slide (`ppt/slides/slide{N}.xml`).
  - Text blocks placed in slide text shapes (`p:sp`) with coordinates translated to EMU units (English Metric Units: 1 pt = 12,700 EMUs).
  - Background graphics and photos placed in slide picture shapes (`p:pic`).

### Task 10.3: Image & Text Export Formats
- **Files**: `crates/youpdf-core/src/convert.rs`, `crates/youpdf-app/src/app.rs`
- **Formats**:
  - Image: PNG, JPEG (quality 1–100), TIFF. Resolution options: 72, 150, 300, 600 DPI.
  - "Combine into Single Long Image": renders all pages and vertically stitches them into a continuous long graphic (useful for mobile scrolling).
  - Text & Web: Plain Text (`.txt`), Markdown (`.md`), HTML (`.html`), Rich Text Format (`.rtf`), XML (`.xml`), CSV (`.csv`).
  - Standard Archival PDF: Export to PDF/A-1b, PDF/A-2b, PDF/X-1a, PDF/X-4.

### Task 10.4: Advanced Compression (Image Resampling & Pruning)
- **Files**: `crates/youpdf-core/src/compress.rs`, `crates/youpdf-app/src/app.rs`
- **Compression Dialog**:
  - Presets:
    - High Quality (Print): Resample images to 300 DPI, JPEG quality 88.
    - Good Quality (Ebook): Resample images to 150 DPI, JPEG quality 75.
    - Small Size (Screen): Resample images to 72 DPI, JPEG quality 55.
  - Optimization toggles:
    - Compress uncompressed streams (`/FlateDecode`).
    - Remove unused bookmarks and document structure trees.
    - Remove embedded thumbnails (PDFium renders faster without them).
    - Strip private metadata and XML packets.
  - Preview calculation: Computes before/after size before saving.

---

# Phase 11: OCR Engine (Optical Character Recognition)

### Task 11.1: OCR Engine & Language Support
- **Files**: `crates/youpdf-core/src/ocr.rs`, `crates/youpdf-app/src/app.rs`
- **Engine Capabilities**:
  - Tesseract CLI / library integration.
  - Support 38 languages: English, French, German, Spanish, Italian, Portuguese, Dutch, Russian, Chinese (Simplified & Traditional), Japanese, Korean, Arabic, etc.
  - Support multi-language documents (e.g. `eng+fra`).
  - Page range selection (All, Current, Custom Range).

### Task 11.2: Three OCR Output Modes
- **File**: `crates/youpdf-core/src/ocr.rs`
- **Modes**:
  1. `Searchable PDF (Dual-layer OCR)`:
     - Keeps original scanned page image intact.
     - Injects an invisible text layer underneath the image matching OCR word bounding boxes.
     - Document looks identical to original scan, but text is selectable, searchable, and copyable.
  2. `Editable PDF (Dual-layer OCR)`:
     - Erases scanned text glyphs from background image.
     - Renders recognized text on top in visible fonts, preserving layout and illustrations.
     - Document is fully editable in Edit Mode.
  3. `Text and Pictures Only`:
     - Discards scanned paper texture and grain.
     - Reconstructs document purely with recognized text and isolated extracted photos/drawings.

### Task 11.3: Scanned Document Automatic Detection
- **File**: `crates/youpdf-app/src/app.rs`
- **Behavior**:
  - When opening a document where page 1 contains an image covering >80% of page area and 0 extractable text characters, display notification bar at top:
    *"Scanned document detected. Recognize text using OCR to enable editing, searching, and copying."*
  - Buttons: `[Perform OCR]` (opens OCR dialog) and `[Don't Show Again]`.

---

# Phase 12: Measurement Tools & Document Comparison

### Task 12.1: 2D Measuring Tools Suite
- **Files**: `crates/youpdf-core/src/measure.rs`, `crates/youpdf-app/src/app.rs`
- **Tools**:
  - `Distance Tool`: Click point A, drag to point B. Displays line with tick marks and length label (e.g. `14.2 mm`).
  - `Perimeter Tool`: Click multi-segment path, double click to finish. Displays total path perimeter.
  - `Area Tool`: Click polygon vertices, double click to close. Displays hatched filled polygon and total area (e.g. `124.5 cm²`).
- **Calibration & Scale Dialog**:
  - Scale Ratio: `X [Drawing Unit]` = `Y [Real World Unit]` (e.g. `1 in = 10 ft` or `1 cm = 1 m`).
  - Supported units: pt, mm, cm, in, ft, m, yd.
  - Interactive Calibration tool: click two known points on a blueprint dimension line, enter known real-world distance; computes ratio automatically.
  - Snapping: Snap to orthogonal (holding Shift for 0°, 45°, 90°), snap to vector endpoints.

### Task 12.2: PDF Comparison Engine
- **Files**: `crates/youpdf-core/src/compare.rs`, `crates/youpdf-app/src/app.rs`
- **Capabilities**:
  - Two-document picker dialog: Select Document A (Old) and Document B (New).
  - Comparison options: Ignore whitespace, Ignore text style (fonts/colors), Compare text & images.
  - View modes:
    - Side-by-side synchronized scrolling.
    - Overlay / Differential blend mode.
  - Difference Highlights:
    - Inserted content: Highlighted in Green (`#28A745`).
    - Deleted content: Highlighted in Red (`#DC3545`).
    - Modified content: Highlighted in Yellow / Orange (`#FFC107`).
  - Difference Navigator panel: list of all differences; clicking an item scrolls both views to the exact location.

---

# Phase 13: Batch Processing Engine

### Task 13.1: Batch Processing Shell & File Queue
- **File**: `crates/youpdf-app/src/app.rs`
- **Interface**:
  - Accessible via `Outils -> Traitement par lots` or Home screen `Batch` button.
  - File table: Index, File Name, Page Count, File Size, Status (Pending, Processing, Done, Error).
  - Add Files button, Add Folder button, Drag-and-drop files directly into table, Clear button.
  - Output options: Output to source directory or Specific folder; File naming rules (keep original, add prefix, add suffix).
  - Progress bar with estimated time remaining and cancel button.

### Task 13.2: 12 Batch Operations Implementation
- **Files**: `crates/youpdf-core/`, `crates/youpdf-app/src/app.rs`
- **Operations**:
  1. `Batch Convert`: Batch convert PDFs to Word, Excel, PPT, Image, Text, or HTML.
  2. `Batch OCR`: Run OCR over hundreds of documents in background threads.
  3. `Batch Combine`: Merge all queued PDF files into one master document.
  4. `Batch Insert`: Insert a specific cover page or appendix into all documents at specified index.
  5. `Batch Watermark`: Apply a chosen watermark preset across all documents.
  6. `Batch Background`: Apply uniform background color or letterhead to all documents.
  7. `Batch Header & Footer`: Apply uniform header/footer numbering across all files.
  8. `Batch Bates Numbering`: Continuous legal Bates numbering across entire document set.
  9. `Batch Compress`: Downsample images and deflate all documents to meet size caps.
  10. `Batch Encrypt`: Protect all documents with open or permissions passwords.
  11. `Batch Remove`: Batch delete specific page numbers (e.g. remove page 1 cover from all files).
  12. `Batch Print`: Send all queued documents sequentially to selected system printer.

---

# Phase 14: Settings, Themes & Full Localization (i18n)

### Task 14.1: Comprehensive Preferences Dialog (`Cmd+,`)
- **File**: `crates/youpdf-app/src/app.rs`
- **Tabs**:
  - **General**:
    - Application Startup: Show Home screen / Restore last opened documents / Open blank canvas.
    - Default Page Zoom: Fit Width / Fit Page / Actual Size (100%) / Custom %.
    - Default Page Layout: Single / Continuous / Two Pages / Two Pages with Cover.
    - Author Name: default author used for annotations and comments.
  - **Appearance**:
    - Theme selection: Light, Dark, Mint Green, Starry Blue, Follow System.
  - **Document**:
    - Auto-save interval slider: Disabled, 1 min, 5 min, 10 min.
    - Default measurement units.
  - **Comments**:
    - Keep annotation tool selected after use (toggle).
    - Automatically open comments pop-up note upon creation (toggle).
  - **AI Settings**:
    - Enable/disable AI layer.
    - Provider configuration (Ollama, OpenAI, Anthropic, Gemini).
    - Loopback network security toggle.

### Task 14.2: Full Localization (i18n) Architecture
- **Files**: `crates/youpdf-shared/src/i18n.rs` (New file), `crates/youpdf-app/`
- **Capabilities**:
  - Extract all user-facing strings into structured language dictionaries (`en.json`, `fr.json`, `de.json`, `es.json`, `it.json`, `zh-Hans.json`, `ja.json`).
  - Implement macro `t!("string_key")` or `t!("string_key", param = val)`.
  - Language picker in Settings with instant UI update without application restart.
  - Match UPDF's exact translations for all menus, tooltips, dialogs, and error messages.

---

# Execution Order & Verification Gates

```
[Phase 0: Cleanup & Refactor]
       │
       ▼
[Phase 1: Chrome & Tabs]  ──►  [Phase 2: Left Sidebars]
       │                                │
       ▼                                ▼
[Phase 3: Floating Inspectors] ◄─ [Phase 4: Content Editing]
       │
       ▼
[Phase 5: Annotations]  ──────►  [Phase 6: Form Builder]
       │                                │
       ▼                                ▼
[Phase 7: Organize Surface]  ──► [Phase 8: Page Operations]
       │                                │
       ▼                                ▼
[Phase 9: Security & Signatures]►[Phase 10: Office Export & Compress]
       │                                │
       ▼                                ▼
[Phase 11: OCR Engine]   ─────►  [Phase 12: Measure & Compare]
       │                                │
       ▼                                ▼
[Phase 13: Batch Processing] ──► [Phase 14: Settings & i18n]
```

Every phase ends with an end-to-end regression pass:
1. `cargo check --workspace --all-targets`
2. `cargo test --workspace`
3. Manual UI smoke test of the specific phase surface against UPDF.app reference behavior.
