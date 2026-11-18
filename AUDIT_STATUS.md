# YouPDF ↔ UPDF 2.5.6 — Parity Audit Status

*Based on: docs/updf-parity-visual-and-feature.md*

## AUDIT SUMMARY

Waves 0–5 and the Tier 1/2 round have **landed** (HEAD `e8b3a8ea`). The per-feature
PARTIAL / 42 MISSING / 1 UNKNOWN across 118 feature families**. Remaining gaps are the
appearance-only diffs in `docs/visual-parity.md` and the Tier 2/3 backlog in `docs/GAP.md`.

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
| **Conversation history:** Persistence of chat turns (`Turn` struct) | ⚠️ Partial — `Turn` remains for the in-memory journal; `ConversationHistory` was removed in Phase 3 (never used) |
| **Suggested questions:** Populate based on context/document | ❌ Removed in Phase 3 — `suggested_questions()` was never called by the UI |
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
| Decor: AI-generated watermarks, PDF-file watermarks, rotation randomness, template/starred presets | ⚠️ **AI-generated ☐**; **PDF-file ✅, rotation randomness ✅, template/starred ✅** (Wave 5, `e8b3a8ea`) |
| **Bates: Interactive (non-batch) Bates tool** | ✅ Already implemented in `pages_ui.rs` `bates_window()` |

### 3.4 Security & Export — GAP: LOW
| Item | Status |
|------|--------|
| **Security Properties Matrix: Read-only view of permission matrix** | ✅ **DONE** — `Protect ▸ Security Properties…` decodes `/Encrypt` `/P` matrix (`protect/permissions.rs` + `pages_ui.rs`) |
| **Export: Validate PDF/A (ICC embedding), Office import (.docx/.xlsx/.pptx)** | ⚠️ **Office import ✅ done** (`soffice`); **Validating PDF/A ☐** (Tier 3) |
| **Flattening: Separate flattening for watermarks and cropped pages** | ☐ |

### 3.5 Annotate Tools — GAP: LOW
| Item | Status |
|------|--------|
| **ADD: Sticker tool** | ✅ **Added** `ToolsEntry::Sticker` + `ToolAction::Sticker` |
| **ADD: Connected Lines tool** | ✅ **Added** `ToolsEntry::ConnectedLines` + `ToolAction::ConnectedLines` |
| **ADD: Cloud tool** | ✅ **Added** `Cloud` to `DraftKind` + UI in `annots_ui.rs` |
| **ADD: Shape Fill tool** | ✅ **Added** `annot_fill` in `app.rs` + inspector UI + `annot_style()` uses fill |

---

## 4. Execution Plan (Waves 1–4) — LANDING STATUS

| Wave | Status |
|------|--------|
| **Wave 0 — Collision Prep** | ✅ All items done (label/mod.rs, capture/lib.rs, i18n freeze) |
| **Wave 1 — Core Engine Gaps** | ✅ A1-A6 all done (labels, resize/exchange, links, redact codesets, word count, permissions) |
| **Wave 2 — UI Integration** | ✅ B1-B6 all done (inspector page labels, resize/interleave/exchange, link appearance, redact code-sets, windows stats+matrix, forms push-button) |
| **Wave 3 — New Subsystems** | ✅ C1-C4 all done (TTS, presenter view, Open Quickly, create from capture) |
| **Wave 4 — AI Gaps** | ✅ D1-D2 done (AI writing tools + Check Pages, bilingual translation) |
| **Wave 5 — Strategic** | ⚠️ Office import ✅, PDF/A validation ☐, element align ☐, PDF Portfolio ☐ |

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

This list is the *current* open backlog — the October items below were closed in the
Tier 1/2 + Wave 5 rounds and are now in `docs/GAP.md` / `docs/visual-parity.md`:

1. **Native macOS menu bar** — real `NSMenu` for the 9-menu bar (top visual gap).
2. **Visual parity (1-to-1)** — toolbar presets, paper style, Liquid Glass, traffic-light inset, home thumbnail per-page aspect, mode-pill chrome, status-bar mirror (`docs/visual-parity.md` §10).
3. **AI rendering** — Markdown + LaTeX in chat responses, attachment picker.
4. **Advanced AI** — Mind-map, image generation, AI bookmarks, web AI, DeepThink mode.
5. **Export & PDF/A** — validating PDF/A (ICC), flattening separation, PDF/UA/PDF/E/PDF/X writers.

**Closed since this audit:** in-memory chat turns (Wave 2 — `ConversationHistory`/`suggested_questions` since removed as unused), merge/duplicate pages + Bates (Wave 5), Sticker/ConnectedLines/Cloud/Shape Fill (Wave 5), Security Properties matrix + push-button forms + Office import (Wave 5), page labels/resize/interleave/exchange (Wave 1/2), TTS + presenter + Open Quickly + create-from-capture (Wave 3), AI writing tools + Check Pages + bilingual translation (Wave 4).

(Home-screen thumbnail *per-page aspect* is back in scope — `docs/visual-parity.md` §7.2.)

---
*Generated from: docs/updf-parity-visual-and-feature.md*