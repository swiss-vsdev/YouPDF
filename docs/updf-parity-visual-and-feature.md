# YouPDF ↔ UPDF 2.5.6 — Combined Parity & Execution Plan

*Target: Close all parity gaps against UPDF 2.5.6 (Build 1362). This document merges the visual/feature plan with the execution plan. Builders: use checkboxes to track progress.*

---

## 0. Scope & Rules
- **Goal**: Full visual and feature parity. If a tool does not exist in UPDF, it must be removed from YouPDF.
- **Out of Scope**: Cloud/account/subscription infrastructure, telemetry, auto-updater.
- **Rules**: Follow the "Ponytail" principle (lazy/minimal). No unrequested abstractions.
- **State**: Waves 1-4 landed (commit `1e550459`).

---

## 1. Visual Fidelity Baseline (Apply everywhere)

*Apply these design tokens to all new or modified UI components.*

### 1.1 Palette (`youpdf-shared/src/theme.rs`)
- [x] Dark theme: Canvas `#303030`, Chrome `#282828`, Widget `#282828`.
- [x] Accent: `#7A7EF6`.
- [x] Selected tint: `#E8E8F8`.

### 1.2 Typography & Spacing
- [x] Font sizes: CAPTION 10.5, LABEL 11, BODY 12, TITLE 13, MONO 11.
- [x] Radii: TINY 3, XS 2, S 4, M 6, L 8, XL 10.
- [x] Spacing: TINY 1, XS 2, S 4, M 6, L 8.

---

## 2. Window-by-Window Checklist

### 2.1 Home Screen (`home.rs`)
- [x] Grid/List toggle button (matching UPDF asset: `ActionGrid`/`ActionList`).
- [x] Category tabs: Recents, Starred, Tools.
- [ ] Thumbnail generation: match UPDF's crop/display aspect.

### 2.2 Document Chrome (`chrome_ui.rs`)
- [x] Title bar: custom with traffic lights inset (78px width).
- [x] Tab bar: pinning, closing, reordering, middle-click close (visuals: palette colors).
- [x] Overflow menu (`…`): Consolidate "Tools" dropdown into a standard macOS-style overflow.

### 2.3 Floating Inspectors (`inspector_ui.rs`)
- [x] Appearance parity: Arrow positioning and palette.
- [x] Link appearance inspector: Visible toggle, width, dash/solid, colour picker.
- [x] Text editing: Add superscript/subscript, paragraph spacing after, horizontal scaling, increase/decrease indent.

---

## 3. Feature Decomposition

### 3.1 AI Subsystem (Gap: High)
- [x] **Conversation history:** Persistence of chat turns (`Turn` struct).
- [x] **Suggested questions:** Populate based on context/document.
- [ ] **UI Rendering:** Markdown + LaTeX rendering in chat responses.
- [ ] **Attachment picker:** `ChatPagePicker` wiring.
- [ ] **Advanced AI features:**
    - [ ] **ADD:** Mind-map generation (`MindMapCell`).
    - [ ] **ADD:** Image generation (~90 style templates: watermark, background, stamp, sticker).
    - [ ] **ADD:** AI bookmark generation (model-driven structure extraction).
    - [ ] **ADD:** AI search in document.
    - [ ] **ADD:** Web AI (Chat Project / Paper Search / Deep Research).
    - [ ] **ADD:** "Deep Thinking" (DeepSeek-R1) mode.

### 3.2 Forms (Gap: Medium)
- [x] **Action Dispatch:** Push-button actions (Reset/Submit/ShowField) landed in Wave 2 (`forms_editor.rs`).
- [ ] **Advanced properties:** Date picker field, XFA form support, JavaScript actions (interpreter integration), calculation-order list editor, multiple-copy form UI.
- [ ] **REMOVE:** Divider field (Not present in UPDF).

### 3.3 Decor & Organise (Gap: Medium)
- [ ] **Organize:**
    - [x] **ADD:** Merge pages (Batch).
    - [x] **ADD:** Duplicate pages.
    - [x] **ADD:** Page labels (/PageLabels reader/writer) — engine never landed; `ai.rs:251` NonConsecutive detector is a stub waiting on it.
    - [x] Page resize (landed: `organize/resize.rs`, dialog in `pages_ui.rs`).
- [ ] **Decor:** AI-generated watermarks, PDF-file watermarks, rotation randomness in watermarks, template/starred watermark presets.
- [x] **Bates:** Interactive (non-batch) Bates tool.

### 3.4 Security & Export (Gap: Low)
- [ ] **Security Properties Matrix:** Read-only view of permission matrix for encrypted files.
- [ ] **Export:** Validate PDF/A (ICC embedding), Office import (.docx/.xlsx/.pptx).
- [ ] **Flattening:** Separate flattening for watermarks and cropped pages.

### 3.5 Annotate Tools (Gap: Low)
- [x] **ADD:** Sticker tool.
- [x] **ADD:** Connected Lines tool.
- [x] **ADD:** Cloud tool.
- [x] **ADD:** Shape Fill tool.

---

## 4. Execution Plan (Wave-based build order)

*To manage development and prevent merge conflicts, the remaining gaps will be addressed in sequential waves.*

### Wave 0 — Collision Prep (Single Developer Commit)
- [x] Add `pub mod labels;` to `crates/youpdf-core/src/organize/mod.rs` (empty file).
- [x] Add `pub mod capture;` to `crates/youpdf-core/src/lib.rs`.
- [x] Run `python3 scripts/i18n_extract.py --check` once at the end of each wave (not per-agent) to prevent merge conflicts on the i18n dictionary.
- [x] Freeze `toolbar_ui.rs` — Wave 2/3 agents expose `pub(super) fn` and you wire the menu entries in one clean commit per wave.

### Wave 1 — Core Engine Gaps (6 Parallel Agents, Zero File Overlap)
*Each agent writes a core module in `crates/youpdf-core/src/` and one test in `crates/youpdf-core/tests/`.*
1. [ ] **Agent A1 (Page Labels):** `organize/labels.rs`. `PageLabel { prefix, style, start }` over `/PageLabels << /Nums [0 <<…>>] >>`. Roman/Arabic formatters.
2. [ ] **Agent A2 (Page Resize & Reorder):** `organize/resize.rs` & `exchange.rs`. `resize_page` via `/MediaBox` + `cm` prepended to content stream; interleave & exchange over page index arrays (`ponytail:` note on annotation bounds).
3. [ ] **Agent A3 (Link Appearance):** `links.rs`. `/BS` dictionary + legacy `/Border` mirror + colour `[r g b]`.
4. [ ] **Agent A4 (Redaction Extras):** `redact/`. Relax text requirement for `redact_marks` (enables image/page redaction), add `redact_page`, and build a regex code-sets matcher (SSN, CC, IBAN, email, phone, date, US Privacy Act, FOIA).
5. [ ] **Agent A5 (Word Count & Stats):** `search.rs`. `word_count()`, `cjk_count()`, `space_count()` over `SearchIndex` text (zero new PDF passes).
6. [ ] **Agent A6 (Security Permissions Matrix):** `protect/`. Decode `/P` permission bits into a typed boolean matrix.

### Wave 2 — UI Integration (6 Parallel Agents, Single File Ownership)
*Each agent owns one UI file in `crates/youpdf-app/src/app/` and exposes a rendering function.*
1. [ ] **Agent B1 (`inspector_ui.rs`):** Page-label editor widget.
2. [ ] **Agent B2 (`pages_ui.rs`):** Resize dialog, interleave, and exchange buttons.
3. [ ] **Agent B3 (`annots_ui.rs`):** Link appearance inspector fields (visible toggle, width, dash, colour).
4. [ ] **Agent B4 (`redact_ui.rs`):** Collapsible code-sets checklist (`CollapsingHeader`) + page/image redaction triggers.
5. [ ] **Agent B5 (`windows_ui.rs`):** Word count + permissions matrix in the existing Properties window.
6. [ ] **Agent B6 (`forms_editor.rs`):** Push-button action dispatch (Reset / Submit / ShowField toast).

### Wave 3 — New Subsystems (4 Parallel Agents)
1. [ ] **Agent C1 (Read-aloud / TTS):** `capture/tts.rs`. Shell out to macOS `say` (`say -v ?`, `say -r <wpm> -v <voice> <text>`, child process handle). Zero new dependencies.
2. [ ] **Agent C2 (Presenter View):** `slideshow_ui.rs`. Second `egui::Window` showing next-page thumbnail (reusing render cache), elapsed timer, and counter.
3. [ ] **Agent C3 (Open Quickly):** `home.rs` / `toolbar_ui.rs`. ⌘⇧O fuzzy launcher over the recents list (subsequence match, top 12).
4. [ ] **Agent C4 (Create from Capture):** `capture/screenshot.rs`. Shell out to `screencapture -i -x` + existing image-to-PDF pipeline.

### Wave 4 — AI Gaps (2 Parallel Agents)
1. [ ] **Agent D1 (AI Writing Tools & Check Pages):** `crates/youpdf-ai/` & `ai.rs`. 5 prompt tasks (Polish, Shorten, Expand, Proofread, Tone) + 7 defect detectors over `Document` (blank, scanned, dimensions, comments, rotation).
2. [ ] **Agent D2 (Whole-PDF Bilingual Translation):** `ai.rs` + `office.rs`. Text-run extraction, batched translation, absolute-box placement with automatic font-size shrinking (no reflow), and optional bilingual side-by-side stack.

### Wave 5 — Strategic Product Decisions (No Code Agents)
1. [ ] **Office Import:** Shell to `soffice --convert-to pdf` when LibreOffice is present; else show install prompt.
2. [ ] **Validating PDF/A:** Scope as PDF/A-2b with an embedded sRGB ICC profile and `/OutputIntent` (skip full validation engine).
3. [ ] **Element Align/Arrange Panel:** Blocked on multi-select; single-object arrange is already done.
4. [ ] **PDF Portfolio:** Treat as `/Collections` wrapper over existing `/EmbeddedFiles` attachments.

---

## 5. Evidence Map (Current State: Waves 1-4 landed)

| Tool/Evidence | Status |
|---|---|
| `menu_tree.txt` | Fully analyzed |
| `en.localizable.strings` | Fully analyzed |
| `updf_swift_source_paths` | Fully analyzed |
| `Assets.car` (3903 assets) | Partially analyzed |

*Builders: If you need to verify a specific UI action, run `grep` against `crates/youpdf-app/src/app/` for the tool/button action ID.*
