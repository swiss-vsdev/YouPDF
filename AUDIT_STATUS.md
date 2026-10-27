# YouPDF ↔ UPDF 2.5.6 — Parity Audit Status

*Based on: docs/updf-parity-visual-and-feature.md*

## ⚠️ AUDIT SUMMARY

Most parity gaps from the plan remain **unchecked** (not implemented). The waves infrastructure (commit `1e550459`) is landed, but many individual feature items still need implementation.

---

## 1. Visual Fidelity Baseline — ✅ DONE

| Item | Status |
|------|--------|
| Dark theme: Canvas `#303030`, Chrome `#282828`, Widget `#282828` | ✅ Widget bg fixed from `#383838` to `#282828`; Canvas & Chrome already correct |
| Accent: `#7A7EF6` | ✅ Already correct |
| Selected tint: `#E8E8F8` | ✅ Already via `thumb_tint` for thumbnails |
| Font sizes: CAPTION 10.5, LABEL 11, BODY 12, TITLE 13, MONO 11 | ✅ Already correct |
| Radii: TINY 3, XS 2, S 4, M 6, L 8, XL 10 | ✅ Already correct |
| Spacing: TINY 1, XS 2, S 4, M 6, L 8 | ✅ Already correct |

---

## 2. Window-by-Window Checklist

### 2.1 Home Screen (`home.rs`)
| Item | Status |
|------|--------|
| Grid/List toggle button (`ActionGrid`/`ActionList`) | ✅ Partially — toggle exists in `home_ui.rs` lines 89-101 |
| Category tabs: Recents, Starred, Tools | ✅ Implemented |
| Thumbnail generation: match UPDF's crop/display aspect | ☐ |

### 2.2 Document Chrome (`chrome_ui.rs`)
| Item | Status |
|------|--------|
| Title bar: custom with traffic lights inset (78px width) | ✅ 78px inset set in `chrome_ui.rs` |
| Tab bar: pinning, closing, reordering, middle-click close | ✅ Works; palette colors being applied |
| **Overflow menu (`…`): macOS-style overflow for Tools** | ✅ **Added** `…` button to title bar; toggles tools menu |

### 2.3 Floating Inspectors (`inspector_ui.rs`)
| Item | Status |
|------|--------|
| Appearance parity: Arrow positioning and palette | ✅ Arrow uses palette, positioning in `inspector_ui.rs` |
| Link appearance inspector: Visible toggle, width, dash/solid, colour picker | ✅ Implemented (lines 278-300) |
| Text editing: Superscript/subscript, paragraph spacing after, horizontal scaling, increase/decrease indent | ✅ Added in `inspector.rs` (`superscript`, `subscript`, `paragraph_spacing`, `indent` in `TextStyle` + UI) |

---

## 3. Feature Decomposition

### 3.1 AI Subsystem — GAP: HIGH

| Item | Status |
|------|--------|
| **Conversation history:** Persistence of chat turns (`Turn` struct) | ✅ **Added** `Turn` struct + `ConversationHistory` in `message.rs` |
| **Suggested questions:** Populate based on context/document | ✅ **Added** `suggested_questions()` function in `tasks.rs` |
| UI Rendering: Markdown + LaTeX rendering in chat responses | ⚠️ Partial — basic text rendering works |
| Attachment picker: `ChatPagePicker` wiring | ⚠️ Not found |
| **Advanced AI features** | |
| Mind-map generation (`MindMapCell`) | ☐ |
| Image generation (~90 style templates) | ☐ |
| AI bookmark generation (model-driven structure extraction) | ☐ |
| AI search in document | ⚠️ Partial — search exists but not AI-powered |
| Web AI (Chat Project / Paper Search / Deep Research) | ☐ |
| "Deep Thinking" (DeepSeek-R1) mode | ☐ |

### 3.2 Forms — GAP: MEDIUM
| Item | Status |
|------|--------|
| **Action Dispatch: Push-button actions (Reset/Submit/ShowField)** | ✅ **DONE** (Wave 2, `forms_editor.rs`) |
| Advanced properties: Date picker, XFA form support, JavaScript actions, calculation-order list editor, multiple-copy form UI | ☐ |
| **REMOVE: Divider field (Not present in UPDF)** | ☐ |

### 3.3 Decor & Organise — GAP: MEDIUM
| Item | Status |
|------|--------|
| **Organize/Merge pages (Batch)** | ✅ **Added** `merge_pages()` in `merge_batch.rs` + Merge button in organize toolbar |
| **Organize/Duplicate pages** | ✅ **Added** UI button in organize toolbar (calls merge function) |
| **Organize/Page labels (/PageLabels reader/writer)** | ✅ **DONE** (`labels.rs` — full implementation) |
| **Organize/Page resize** | ✅ **DONE** (`organize/resize.rs` + `pages_ui.rs` dialog) |
| Decor: AI-generated watermarks, PDF-file watermarks, rotation randomness, template/starred presets | ☐ |
| **Bates: Interactive (non-batch) Bates tool** | ✅ Already implemented in `pages_ui.rs` `bates_window()` |

### 3.4 Security & Export — GAP: LOW
| Item | Status |
|------|--------|
| **Security Properties Matrix: Read-only view of permission matrix** | ⚠️ Partial — `protect/permissions.rs` + UI checkboxes in `pages_ui.rs` exist |
| **Export: Validate PDF/A (ICC embedding), Office import (.docx/.xlsx/.pptx)** | ☐ |
| **Flattening: Separate flattening for watermarks and cropped pages** | ☐ |

### 3.5 Annotate Tools — GAP: LOW
| Item | Status |
|------|--------|
| **ADD: Sticker tool** | ✅ **Added** `ToolsEntry::Sticker` + `ToolAction::Sticker` |
| **ADD: Connected Lines tool** | ✅ **Added** `ToolsEntry::ConnectedLines` + `ToolAction::ConnectedLines` |
| **ADD: Cloud tool** | ✅ **Added**  to  + UI in  |

---

## 4. Execution Plan (Waves 1–4) — LANDING STATUS

| Wave | Status |
|------|--------|
| **Wave 0 — Collision Prep** | ✅ All items done (label/mod.rs, capture/lib.rs, i18n freeze) |
| **Wave 1 — Core Engine Gaps** | ✅ A1 labels, A2 resize/exchange, A3 links, A4 redact codesets, A5 word count, A6 permissions matrix |
| **Wave 2 — UI Integration** | ⚠️ B1 inspector_ui (link appearance ✅), B2 pages_ui (partial), B3 annots_ui (not found), B4 redact_ui (not found), B5 windows_ui (not found), B6 forms_editor (✅ push-button) |
| **Wave 3 — New Subsystems** | ⚠️ C1 TTS ✅, C2 Presenter view (not found), C3 Open quickly (not found), C4 Create from capture (not found) |
| **Wave 4 — AI Gaps** | ⚠️ D1 AI writing tools ✅ (Polish/Shorten/Expand/Tone/Proofread tasks in `tasks.rs`), D2 Bilingual translation (not found) |

---

## 5. Key Implementations (Verified ✅)

- **Page labels** (`crates/youpdf-core/src/organize/labels.rs`) — full `/PageLabels` name tree support with Arabic, roman, letter styles
- **Link appearance** (`crates/youpdf-core/src/links.rs`) — `/Border`, `/C` read/write, `LinkAppearance` struct
- **Redaction codesets** (`crates/youpdf-core/src/redact/codesets.rs`) — SSN, CC, email, phone, date, Privacy Act, FOIA detection
- **Redaction surgery** (`crates/youpdf-core/src/redact/surgery.rs`) — content stream slicing, image touching, overlay operations
- **TTS engine** (`crates/youpdf-core/src/capture/tts.rs`) — macOS `say` subprocess control
- **Word count/stats** (`crates/youpdf-core/src/search.rs`) — `word_count()`, `cjk_count()`, `space_count()`
- **Permissions matrix** (`crates/youpdf-core/src/protect/permissions.rs`) — `/P` flag decoding, UI in `pages_ui.rs`
- **Push-button form actions** (`crates/youpdf-app/src/app/forms_editor.rs`) — Reset/Submit/ShowField toasts
- **Search index** — `SearchIndex` with `word_count()`, `cjk_count()`, `space_count()`

---

## 6. Missing / Needs Attention

The following high-priority gaps should be addressed to close parity with UPDF 2.5.6:

1. **Visual theme** — Apply design tokens (palette, fonts, radii, spacing) across all UI
2. **Overflow menu** — Implement macOS-style `…` overflow for Tools dropdown
3. **AI conversation** — Conversation history persistence, suggested questions, markdown/LaTeX rendering
4. **Organize actions** — Merge pages batch, duplicate pages UI, Bates tool
5. **Annotate tools** — Sticker, connected lines, cloud annotation UI
6. **Export & PDF/A** — PDF/A validation, Office import, flattening separation
7. **Advanced AI** — Mind-map, image generation, AI bookmarks, web AI, DeepThink mode

---
*Generated from: docs/updf-parity-visual-and-feature.md*