# YouPDF ↔ UPDF 2.5.6 — Combined Parity & Execution Plan

*Target: Close all parity gaps against UPDF 2.5.6 (Build 1362). This document merges the visual/feature plan with the execution plan. Builders: use checkboxes to track progress.*

---

## 0. Scope & Rules
- **Goal**: Full visual and feature parity. If a tool does not exist in UPDF, it must be removed from YouPDF.
- **Out of Scope**: Cloud/account/subscription infrastructure, telemetry, auto-updater.
- **Rules**: Follow the "Ponytail" principle (lazy/minimal). No unrequested abstractions.
- **State**: Waves 0–5 all landed (`e8b3a8ea`). Tier 1/2 round shipped Open Quickly, presenter view, read-aloud, document stats, security-properties matrix, link appearance, redaction code sets, measure azimuth/altitude, page labels, page resize, interleave/exchange, PDF-file watermarks, Office import, create-from-capture. See `docs/GAP.md` and `docs/visual-parity.md`.

---

## 1. Visual Fidelity Baseline (Apply everywhere)

*Apply these design tokens to all new or modified UI components.*

### 1.1 Palette (`youpdf-shared/src/theme.rs`)
- [x] Dark theme: Canvas `#303030`, Chrome `#282828`, Widget `#282828`.
- [x] Accent: `#7A7EF6`.
- [x] Selected tint: `#E8E8F8`.
- [x] Light / Mint / Starry themes shipped (4 themes, 15-field `Palette`).

### 1.2 Typography & Spacing
- [x] Font sizes: CAPTION 10.5, LABEL 11, BODY 12, TITLE 13, MONO 11.
- [x] Radii: TINY 3, XS 2, S 4, M 6, L 8, XL 10.
- [x] Spacing: TINY 1, XS 2, S 4, M 6, L 8.

---

## 2. Window-by-Window Checklist

### 2.1 Home Screen (`home.rs`)
- [x] Grid/List toggle button (`ActionGrid`/`ActionList`).
- [x] Category tabs: Recents, Starred, Tools.
- [ ] Thumbnail generation: match UPDF's crop/display aspect → per-page aspect, not the fixed 160×130 box (`docs/visual-parity.md` §7.2).

### 2.2 Document Chrome (`chrome_ui.rs`)
- [x] Title bar: custom with traffic lights inset (78px width).
- [x] Tab bar: pinning, closing, reordering, middle-click close (visuals: palette colors).
- [x] Overflow menu (`…`): consolidate "Tools" dropdown into a standard macOS-style overflow.
- [ ] Native 9-menu bar — not implemented; all commands still in the `…` overflow (`docs/visual-parity.md` §1.1).

### 2.3 Floating Inspectors (`inspector_ui.rs`)
- [x] Appearance parity: Arrow positioning and palette.
- [x] Link appearance inspector: Visible toggle, width, dash/solid, colour picker.
- [x] Text editing: Add superscript/subscript, paragraph spacing after, horizontal scaling, increase/decrease indent.

---

## 3. Feature Decomposition

### 3.1 AI Subsystem
- [x] **Conversation history:** Persistence of chat turns (`Turn` struct, `message.rs`).
- [x] **Suggested questions:** `suggested_questions()` in `tasks.rs`.
- [x] **AI writing tools:** Polish / Shorten / Expand / Proofread / Tone (5 prompt tasks).
- [x] **AI Check Pages:** `detect_page_defects` — blank, invalid, rotated, non-consecutive, comments, mismatched-dimensions, scanned.
- [x] **Whole-PDF bilingual translation:** `translate_page` + `translate_pdf`, shrink-to-fit, bilingual side-by-side.
- [ ] **UI Rendering:** Markdown + LaTeX rendering in chat responses.
- [ ] **Attachment picker:** `ChatPagePicker` wiring.
- [ ] **Mind-map generation** (`MindMapCell`).
- [ ] **Image generation** (~90 style templates: watermark, background, stamp, sticker).
- [ ] **AI bookmark generation** (model-driven structure extraction).
- [ ] **AI search in document.**
- [ ] **Web AI** (Chat Project / Paper Search / Deep Research).
- [ ] **"Deep Thinking"** (DeepSeek-R1) mode.

### 3.2 Forms
- [x] **Action Dispatch:** Push-button actions (Reset/Submit/ShowField) landed (`forms_editor.rs`).
- [x] **Advanced properties:** Date picker field. XFA, JavaScript actions, calculation-order list editor, multiple-copy form UI remain out of scope.
- [x] **REMOVE:** Divider field (Not present in UPDF).

### 3.3 Decor & Organise
- [x] **Organize:** Merge pages (Batch), Duplicate pages, Page labels (`labels.rs`), Page resize (`resize.rs`).
- [x] **Decor:** Text watermarks (single/tiled, rotation, position, opacity, page range); header & footer with page numbering; background (color + image); Bates numbering; PDF-file watermarks; rotation randomness in tiled watermarks; template/starred watermark presets.
- [ ] **AI-generated watermarks** (AI-driven text/style suggestions).
- [x] **Bates:** Interactive (non-batch) Bates tool.

### 3.4 Security & Export
- [x] **Digital Signatures:** PKCS#7 detached (RSA SHA-256), self-signed cert, PKCS#12 import, validation, `signature_status()` banner (green valid / red modified).
- [x] **Security Properties Matrix:** Read-only `/P` permission view (`Protect ▸ Security Properties…`).
- [x] **Office import** (.docx/.xlsx/.pptx) via `soffice`.
- [ ] **Export:** Validate PDF/A (ICC embedding).
- [ ] **Flattening:** Separate flattening for watermarks and cropped pages.

### 3.5 Annotate Tools
- [x] **Sticker tool.**
- [x] **Connected Lines tool.**
- [x] **Cloud tool.**
- [x] **Shape Fill tool.**

---

## 4. Execution Plan (Wave-based build order)

### Wave 0 — Collision Prep
- [x] `pub mod labels;` in `organize/mod.rs`; `pub mod capture;` in `lib.rs`.
- [x] i18n extractor run once per wave.
- [x] `toolbar_ui.rs` frozen; agents expose `pub(super) fn`.

### Wave 1 — Core Engine Gaps
- [x] **A1 (Page Labels)** — `organize/labels.rs`.
- [x] **A2 (Page Resize & Reorder)** — `organize/resize.rs` & `exchange.rs`.
- [x] **A3 (Link Appearance)** — `links.rs` `/BS` + `/Border` mirror.
- [x] **A4 (Redaction Extras)** — `redact/` code sets + repeat-across-pages + auto-size.
- [x] **A5 (Word Count & Stats)** — `search.rs`.
- [x] **A6 (Security Permissions Matrix)** — `protect/` `/P` decode.

### Wave 2 — UI Integration
- [x] **B1** (`inspector_ui.rs`) page-label editor.
- [x] **B2** (`pages_ui.rs`) resize dialog, interleave, exchange.
- [x] **B3** (`annots_ui.rs`) link appearance inspector.
- [x] **B4** (`redact_ui.rs`) code-sets checklist.
- [x] **B5** (`windows_ui.rs`) stats + permissions matrix in Properties.
- [x] **B6** (`forms_editor.rs`) push-button action dispatch.

### Wave 3 — New Subsystems
- [x] **C1 (Read-aloud / TTS)** — `capture/tts.rs`, macOS `say`.
- [x] **C2 (Presenter View)** — `slideshow_ui.rs` `presenter_window`.
- [x] **C3 (Open Quickly)** — `home_ui.rs` ⌘F launcher.
- [x] **C4 (Create from Capture)** — `capture/screenshot.rs`.

### Wave 4 — AI Gaps
- [x] **D1 (AI Writing Tools & Check Pages)** — `tasks.rs` + `ai.rs`.
- [x] **D2 (Whole-PDF Bilingual Translation)** — `ai.rs` + `redact.rs`.

### Wave 5 — Strategic Product Decisions
- [x] **Office Import:** `soffice --convert-to pdf`.
- [ ] **Validating PDF/A:** PDF/A-2b with embedded sRGB ICC + `/OutputIntent`.
- [ ] **Element Align/Arrange Panel:** blocked on multi-select; single-object arrange done.
- [ ] **PDF Portfolio:** `/Collections` wrapper over `/EmbeddedFiles`.

### Wave 6 — 1-to-1 visual parity
Appearance-only work, tracked in `docs/visual-parity.md` §10. Pixel diffs, not feature gaps:
1. Native 9-menu bar (`NSMenu`) — the top visual gap.
2. Toolbar auto-hide presets (Keep Display / Scroll Pages / Keep Hidden).
3. Paper style background.
4. Page shadow matching UPDF.
5. Liquid Glass translucency.
6. Traffic-light inset from a measured constant.
7. Mode-pill capsule chrome.
8. Status-bar layout mirror.
9. Home thumbnail per-page aspect.
10. Back/forward page history (`⌘[`/`⌘]`).
11. Cover-page-alone in two-page view.
12. Hardcoded-colour → palette-token sweep.

---

## 5. Evidence Map (Current State: Waves 1-5 + Tier 1/2 landed)

| Tool/Evidence | Status |
|---|---|
| `reference notes` | Fully analyzed |
| `en.localizable.strings` | Fully analyzed |
| `updf_swift_source_paths` | Fully analyzed |
| `Assets.car` (3903 assets) | Partially analyzed |

*Builders: verify a UI action with `grep` against `crates/youpdf-app/src/app/`.*

---

## 6. Remaining Gaps (Wave 6+)

| Area | Gap | Effort |
|---|---|---|
| Visual | Native macOS 9-menu bar (`NSMenu`) | Medium |
| Visual | Toolbar auto-hide presets | Low |
| Visual | Paper style background | Medium |
| Visual | Liquid Glass translucency | High |
| Visual | Home thumbnail per-page aspect | Low |
| Visual | Back/forward page history (`⌘[`/`⌘]`) | Low |
| Visual | Hardcoded-colour → palette sweep | Low |
| AI | Markdown + LaTeX rendering in chat responses | Medium |
| AI | Attachment picker (`ChatPagePicker`) wiring | Low |
| AI | Mind-map generation | High |
| AI | Image generation (~90 style templates) | High |
| AI | AI bookmark generation | Medium |
| AI | Web AI / Deep Thinking | High / Low |
| Forms | XFA form support | High |
| Forms | JavaScript actions | High |
| Decor | AI-generated watermarks | Medium |
| Security/Export | PDF/A validation engine (ICC) | High |
| Security/Export | Separate flattening for watermarks/cropped pages | Medium |
| Export | PDF/UA, PDF/E, PDF/X writers | High |
| Annotate | — | ✅ All landed |
| Organize | — | ✅ All landed |