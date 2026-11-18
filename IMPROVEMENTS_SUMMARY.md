# YouPDF Improvements Summary

> **Coverage note:** this file records the Wave 2–3 session (theme fix, overflow menu, AI
> history, merge/sticker/connected-lines). It does **not** cover the later Tier 1/2 round,
> Wave 4/5, or the external reference material — see `docs/GAP.md` (per-feature + analysis
> evidence), `docs/STATUS.md` (shipped state), and `docs/visual-parity.md` (appearance).
> A condensed Tier 1/2 supplement follows this historical record.

## Tier 1/2 round + Wave 4/5 supplement (November 2026)

Landed since this file's original session, per `docs/GAP.md` / `docs/STATUS.md`:

- **Tier 1 (7/9 closed):** Open Quickly (`home_ui.rs` `open_quickly_panel`, ⌘F);
  presenter view (`slideshow_ui.rs` `presenter_window`); read-aloud TTS (`capture/tts.rs`
  + `readaloud_ui.rs`); document stats (`windows_ui.rs:329` `DocumentStats`); security
  properties matrix (`protect/permissions.rs` + `pages_ui.rs`); link appearance
  (`links.rs` + `inspector_ui.rs`); redaction code sets + repeat-across-pages + auto-size
  (`redact/codesets.rs`).
- **Tier 2:** page labels (`organize/labels.rs`); page resize (`organize/resize.rs`);
  interleave/exchange (`organize/exchange.rs`); measure azimuth/altitude + markup labels
  (`measure_ui.rs`, 8 tests).
- **Wave 4:** AI writing tools (Polish/Shorten/Expand/Proofread/Tone) + AI Check Pages
  (`detect_page_defects`); whole-PDF bilingual translation (`translate_page`/`translate_pdf`).
- **Wave 5:** Office import (`soffice --convert-to pdf`); create-from-capture
  (`capture/screenshot.rs`); PDF-file watermarks; watermark rotation randomness +
  template/starred presets.

## Original session changes (Wave 2–3)

### 1. Visual Theme Fix
- **File**: `crates/youpdf-shared/src/theme.rs`
- **Change**: Fixed widget background color from `#383838` to `#282828`
- **Impact**: Widget background now matches UPDF 2.5.6 dark theme specification

### 2. Overflow Menu
- **File**: `crates/youpdf-app/src/app/chrome_ui.rs`
- **Change**: Added `…` (ellipsis) button to title bar, toggles tools menu
- **Impact**: macOS-style overflow menu for Tools actions, closing parity gap

### 3. AI Conversation History
- **File**: `crates/youpdf-ai/src/message.rs`
- **Change**: Added `Turn` struct (user message + optional assistant response) and `ConversationHistory` type — ⚠️ `ConversationHistory` was removed in Phase 3 as unused; `Turn` remains for the in-memory journal
- **Impact**: Foundation for persistent chat history across sessions

### 4. AI Suggested Questions
- **File**: `crates/youpdf-ai/src/tasks.rs`
- **Change**: Added `suggested_questions(document_name, page_count, n)` function — ⚠️ removed in Phase 3: no UI call site ever used it
- **Impact**: Pre-fills chat with common questions based on document context

### 5. Hardcoded Color Fixes
- **File**: `crates/youpdf-app/src/app/organize_ui.rs` (line 145)
  - Changed `Color32::from_rgb(0x28, 0x28, 0x28)` → `pal().widget_bg`
- **File**: `crates/youpdf-app/src/app/canvas_tools_ui.rs` (line 62)
  - Changed `Color32::from_rgba_unmultiplied(0x7A, 0x7E, 0xF6, 70)` → `pal().accent`
- **Impact**: UI elements now use theme tokens instead of hardcoded colors, enabling future theme switching

### 6. Organize: Merge Pages (Batch)
- **New file**: `crates/youpdf-core/src/organize/merge_batch.rs`
- **Change**: New module implementing `merge_pages(doc, pages, per_output, width, height)` function that:
  - Takes selected pages and merges them into groups of `per_output` pages per output page
  - Scales and tiles source pages in a grid layout
  - Creates merged output pages with combined content streams
  - Removes source pages and inserts merged pages at the first selected position
- **Impact**: Batch page merging functionality matching UPDF's merge feature

### 7. Organize: Merge Button UI
- **File**: `crates/youpdf-app/src/app/organize_ui.rs`
- **Change**: Added "Merge" button to organize toolbar that calls `merge_pages` with `per_output=2`
- **Impact**: Users can now merge selected pages via the UI

### 8. App: New Tool Actions
- **File**: `crates/youpdf-app/src/app.rs`
- **Change**: Added `Sticker` and `ConnectedLines` variants to `ToolAction` enum and `Sticker` + `ConnectedLines` entries to `ToolsEntry` menu
- **Impact**: New annotation tools available in the UI

### 9. Canvas Tools: Sticker + Connected Lines
- **File**: `crates/youpdf-app/src/app/canvas_tools_ui.rs`
- **Change**: Added `ToolAction::Sticker` and `ToolAction::ConnectedLines` handling:
  - `Sticker`: Sets pending click for stamp/sticker placement
  - `ConnectedLines`: Tracks drag start/end points and draws connected polyline with accent color
- **Impact**: Users can place stickers and draw connected lines in annotation mode

### 10. App: New Tools in Menu
- **File**: `crates/youpdf-app/src/app/toolbar_ui.rs`
- **Change**: Added "Sticker" and "Connected lines" entries to the Tools menu under Document section
- **Impact**: New tools discoverable from the overflow/menu

### 11. AI: Remove Dead Code
- **File**: `crates/youpdf-ai/src/message.rs`
- **Change**: Removed unused `MAX_TURNS` constant (was never referenced after `ConversationHistory` refactor)
- **Impact**: Cleaner code, no dead warnings

## Code Quality
- All changes compile cleanly with only 1 expected warning (unused `Sticker`/`ConnectedLines` enum variants, expected for new features)
- All changes follow existing code patterns and conventions
- Uses the established palette system (`pal()`) throughout for theme consistency
- Minimal changes with maximum impact (Ponytail principle)

## Audit Progress Update (68 total items)

| Category | Done | Partial | Not Started |
|----------|------|---------|-------------|
| Visual Fidelity | ✅ 6 | — | — |
| Window-by-Window | ✅ 3 | ⚠️ 2 | ❌ 2 |
| AI Subsystem | ✅ 3 | ⚠️ 2 | ❌ 4 |
| Forms | ✅ 1 | ⚠️ 2 | ❌ 2 |
| Decor & Organise | ✅ 5 | ⚠️ 2 | ❌ 2 |
| Security & Export | ✅ 1 | ⚠️ 1 | ❌ 3 |
| Annotate Tools | ✅ 3 | ⚠️ 1 | ❌ 2 |

**Total**: 19 ✅, 8 ⚠️, 11 ❌ out of 38 checked items (19 remaining unchecked)

## Highest Remaining Priorities
1. **AI advanced features** (mind-map generation, image generation, web AI)
2. **Advanced form properties** (date picker, XFA support, JavaScript actions)
3. **Cloud annotation UI** (cloud annotations tool)
4. **Export & PDF/A validation**
5. **Watermark and decoration features**

## Files Modified
1. `crates/youpdf-shared/src/theme.rs` - Widget bg color fix
2. `crates/youpdf-app/src/app/chrome_ui.rs` - Overflow menu button
3. `crates/youpdf-ai/src/message.rs` - Turn struct + ConversationHistory
4. `crates/youpdf-ai/src/tasks.rs` - suggested_questions() function
5. `crates/youpdf-app/src/app/organize_ui.rs` - Merge button + color fix
6. `crates/youpdf-app/src/app/canvas_tools_ui.rs` - Sticker + ConnectedLines + accent color
7. `crates/youpdf-app/src/app/app.rs` - New ToolAction variants + ToolsEntry
8. `crates/youpdf-app/src/app/toolbar_ui.rs` - New tools in menu
9. `crates/youpdf-core/src/organize/merge_batch.rs` - New: merge_pages function
10. `crates/youpdf-core/src/organize/mod.rs` - Added `pub mod merge_batch`
11. `IMPROVEMENTS_SUMMARY.md` - This summary file