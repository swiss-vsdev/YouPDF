# YouPDF — Parity Agent Specifications (Full Detail)

*Target: 18 subagents across 5 waves, closing 31 parity gaps against UPDF 2.5.6. Each spec includes the plan, the thought (why this approach), the lightweight trick, file ownership, API surface, test cases, and `ponytail:` ceilings.*

---

## Wave 0 — Collision Prep (Do This Yourself, One Commit)

**Goal:** Prevent subagent merge conflicts before they happen.

### Actions

1. **Create empty module stubs:**
   ```bash
   touch crates/youpdf-core/src/organize/labels.rs
   echo "// Page labels: /PageLabels dictionary reader/writer" > crates/youpdf-core/src/organize/labels.rs
   
   touch crates/youpdf-core/src/capture/mod.rs
   echo "// Capture subsystem: screenshots, clipboard, TTS" > crates/youpdf-core/src/capture/mod.rs
   ```

2. **Wire modules:**
   - Add `pub mod labels;` to `crates/youpdf-core/src/organize/mod.rs`
   - Add `pub mod capture;` to `crates/youpdf-core/src/lib.rs`

3. **i18n freeze:**
   - Document in `docs/CONTRIBUTING.md`: *"Agents write raw `t(\"English\")` strings. Run `python3 scripts/i18n_extract.py --check` once at the end of each wave — not per-agent — to regenerate the dictionary and avoid merge conflicts."*

4. **Toolbar freeze:**
   - Wave 2/3 agents expose `pub(super) fn xxx_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui)` and **do not edit** `toolbar_ui.rs`. You wire menu entries in one clean commit per wave.

---

## Wave 1 — Core Engine Gaps (6 Parallel Agents)

### Agent A1 — Page Labels

**File:** `crates/youpdf-core/src/organize/labels.rs` (new, ~180 LoC)

**Plan:**
```rust
pub enum NumStyle { Decimal, RomanUpper, RomanLower, LettersUpper, LettersLower, None }

pub struct PageLabel {
    pub prefix: String,
    pub style: NumStyle,
    pub start: u32,  // 1-indexed start within this label segment
}

pub fn get_label(labels: &[PageLabel], page: u32) -> String;
pub fn set_label(labels: &mut Vec<PageLabel>, page: u32, label: PageLabel);
pub fn write_to_pdf(doc: &mut Document, labels: &[PageLabel]) -> Result<()>;
pub fn read_from_pdf(doc: &Document) -> Result<Vec<PageLabel>>;

// Formatters
fn to_roman(n: u32) -> String;   // 1→I, 4→IV, 5→V, 9→IX, 10→X
fn to_letters(n: u32) -> String; // 1→A, 26→Z, 27→AA
```

**Thought:**
- `/PageLabels` is a sparse array: `/Nums [0 << /P (1-) /S /D >> 5 << /P (A-) /S /A >>]`
- The same shape `outline_gen.rs` already walks — just a different dictionary key.
- Roman/letter conversion is pure arithmetic — no dependencies, no PDF passes.

**Lightweight Trick:**
- Skip UPDF's nested prefix inheritance UI. Ship flat: each page range gets one label, period.
- `ponytail:` comment: *"No prefix inheritance across segments; add if complex legal docs need it."*

**Test Cases (`crates/youpdf-core/tests/page_labels.rs`):**
```rust
#[test]
fn roman_upper_correct() { assert_eq!(to_roman(1), "I"); assert_eq!(to_roman(1999), "MCMXCIX"); }
#[test]
fn letters_wrap_at_z() { assert_eq!(to_letters(26), "Z"); assert_eq!(to_letters(27), "AA"); }
#[test]
fn label_for_page() { /* sparse array lookup correctness */ }
#[test]
fn roundtrip_pdf() { /* write → read → assert equal */ }
```

**UI Hook (Wave 2 B1):**
- One `ComboBox` for style, one `TextEdit` for prefix, one `DragValue` for start page.
- Live preview label: `"Page 5 → " + get_label(labels, 5)`.

---

### Agent A2 — Page Resize, Interleaving, Page Exchange

**Files:** `crates/youpdf-core/src/organize/resize.rs`, `exchange.rs` (~220 LoC total)

**Plan:**
```rust
// resize.rs
pub fn resize_page(doc: &mut Document, page: usize, width: f32, height: f32) -> Result<()> {
    // 1. Rewrite /MediaBox [0 0 width height]
    // 2. Prepend scale matrix to content stream: "sx 0 0 sy tx ty cm"
    // 3. (Optional ponytail) Skip annotation /Rect rescaling
}

// exchange.rs
pub fn interleave_pages(doc: &mut Document, even_first: bool) -> Result<()>;
pub fn exchange_ranges(doc: &mut Document, range_a: Range<usize>, range_b: Range<usize>) -> Result<()>;
```

**Thought:**
- The `cm`-prepend trick scales text, vector, and images uniformly in one operator — no per-element rewriting, no XObject wrapping.
- Interleave/exchange are pure index permutations — build a new order `Vec` and call existing `reorder_pages`.

**Lightweight Trick:**
- `ponytail:` comment on annotation bounds: *"Annotations keep original /Rect in user-space; rescale if visual drift appears on extreme zoom."*
- Interleave = `pages.iter().step_by(2).zip(pages.iter().skip(1).step_by(2)).flatten()`.

**Test Cases (`crates/youpdf-core/tests/page_resize.rs`, `exchange.rs`):**
```rust
#[test]
fn resize_a4_to_letter() { /* MediaBox changed, content scaled */ }
#[test]
fn interleave_even_first() { /* [0,2,4,1,3,5] order */ }
#[test]
fn exchange_disjoint_ranges() { /* swap 0-2 with 10-12 */ }
```

**UI Hook (Wave 2 B2):**
- Reuse the existing crop-margin 4-box UI shape — same layout, different fields (W × H instead of margins).
- Interleave/Exchange = two buttons in the Organize toolbar.

---

### Agent A3 — Link Appearance

**File:** `crates/youpdf-core/src/links.rs` (extend existing, ~70 LoC)

**Plan:**
```rust
pub struct LinkAppearance {
    pub visible: bool,
    pub width: f32,          // points
    pub dash: Option<Vec<f32>>, // e.g. [3.0, 2.0] for dashed
    pub color: [u8; 3],      // RGB
}

pub fn apply_link_appearance(doc: &mut Document, annot_id: ObjectId, appearance: &LinkAppearance) -> Result<()> {
    // Write /BS << /W width /S /S or /D /D [dash] >>
    // Write /C [r g b] (0-1 range)
    // Legacy mirror: /Border [h v w [dash]]
}
```

**Thought:**
- `forms/write.rs` already writes border dictionaries for form fields — this is the same three keys (`/W`, `/S`, `/D`).
- Copy the pattern, don't build an abstraction over both.

**Lightweight Trick:**
- `/BS` alone is enough for modern readers; write legacy `/Border` as a one-line mirror for compat.
- Default: `visible=true, width=1.0, dash=None, color=[0,0,255]` (blue underline).

**Test Cases (`crates/youpdf-core/tests/link_appearance.rs`):**
```rust
#[test]
fn solid_blue_border() { /* /BS << /W 1 /S /S >>, /C [0 0 1] */ }
#[test]
fn dashed_invisible() { /* /BS << /W 0 /S /D /D [3 2] >> */ }
#[test]
fn roundtrip_appearance() { /* apply → read → assert */ }
```

**UI Hook (Wave 2 B3):**
- 4 widgets: visible toggle, width `DragValue`, dash `ComboBox` (Solid/Dashed), colour picker.

---

### Agent A4 — Redaction Extras (Image/Page Redaction, Code Sets)

**Files:** `crates/youpdf-core/src/redact/burn.rs` (extend), `crates/youpdf-core/src/redact/codesets.rs` (new, ~150 LoC)

**Plan:**
```rust
// burn.rs extension
pub fn redact_marks(doc: &mut Document, marks: &[RedactionMark], renderer: &dyn Renderer, fill: RedactionFill) -> Result<()> {
    // Relax: marks no longer require hit_text; a mark with only bbox is valid (image/page redaction)
}

pub fn redact_page(doc: &mut Document, page: usize, fill: RedactionFill) -> Result<()> {
    // Full-page redaction = one mark covering entire MediaBox
}

// codesets.rs
pub struct CodeSetEntry { pub name: &'static str, pub regex: &'static str, pub example: &'static str }

pub const DEFAULT_CODE_SETS: &[CodeSetEntry] = &[
    CodeSetEntry { name: "U.S. Social Security Number", regex: r"\b\d{3}-\d{2}-\d{4}\b", example: "123-45-6789" },
    CodeSetEntry { name: "Credit Card (Visa/MC/AmEx)", regex: r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13})\b", example: "4111-1111-1111-1111" },
    CodeSetEntry { name: "Email Address", regex: r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b", example: "test@example.com" },
    CodeSetEntry { name: "Phone Number (US)", regex: r"\b(?:\+1[-.]?)?\(?\d{3}\)?[-.]?\d{3}[-.]?\d{4}\b", example: "(555) 123-4567" },
    CodeSetEntry { name: "Date (MM/DD/YYYY)", regex: r"\b\d{1,2}/\d{1,2}/\d{4}\b", example: "12/31/2023" },
    CodeSetEntry { name: "U.S. Privacy Act Exemption", regex: r"\b5 U\.S\.C\. § 552a\b", example: "5 U.S.C. § 552a" },
    CodeSetEntry { name: "FOIA Exemption", regex: r"\b5 U\.S\.C\. § 552\(b\)\(\d\)\b", example: "5 U.S.C. § 552(b)(6)" },
];

pub fn find_code_matches(text: &str, code_sets: &[CodeSetEntry]) -> Vec<(usize, usize, &'static str)>;
pub fn mark_code_matches(doc: &Document, page: usize, code_sets: &[CodeSetEntry]) -> Result<Vec<RedactionMark>>;
```

**Thought:**
- The engine never cared whether the region covered text — so "image redaction" is a UI affordance (drag a free box), not an engine feature. Half this gap is one `if`.
- Code sets = a data table + one regex pass reusing the existing search → mark → apply pipeline.

**Lightweight Trick:**
- Use `regex` crate (already in workspace via `tracing-subscriber`/`reqwest` deps) or hand-roll a simple matcher for the 8 patterns.
- `ponytail:` comment: *"No nested/compound rules (e.g. SSN adjacent to a name); add a rule DSL if legal workflows need it."*

**Test Cases (`crates/youpdf-core/tests/redact_codesets.rs`):**
```rust
#[test]
fn ssn_detection() { assert_eq!(find_code_matches("SSN: 123-45-6789", DEFAULT_CODE_SETS).len(), 1); }
#[test]
fn credit_card_detection() { /* Visa/MC/AmEx patterns */ }
#[test]
fn redact_full_page() { /* redact_page burns entire MediaBox */ }
```

**UI Hook (Wave 2 B4):**
- Fill colour is **already wired** (Black/White/Custom) — just add the checkbox list.
- Collapsible `CollapsingHeader("Code Sets")` with 8 checkboxes + "Mark All Matches" button.

---

### Agent A5 — Word Count & Document Stats

**File:** `crates/youpdf-core/src/search.rs` (extend existing `SearchIndex`, ~30 LoC)

**Plan:**
```rust
impl SearchIndex {
    pub fn word_count(&self) -> usize {
        self.pages.iter().map(|p| p.text.split_whitespace().count()).sum()
    }
    
    pub fn cjk_count(&self) -> usize {
        self.pages.iter()
            .flat_map(|p| p.text.chars())
            .filter(|c| matches!(c as u32, 0x4E00..=0x9FFF | 0x3040..=0x309F | 0x30A0..=0x30FF | 0xAC00..=0xD7AF))
            .count()
    }
    
    pub fn space_count(&self) -> usize {
        self.pages.iter().map(|p| p.text.chars().filter(|c| c.is_whitespace()).count()).sum()
    }
    
    // char_count() already exists
}

pub struct DocumentStats {
    pub words: usize,
    pub cjk: usize,
    pub spaces: usize,
    pub total_chars: usize,
}
```

**Thought:**
- `SearchIndex::build` already extracts every page's text and `char_count()` already exists.
- The whole feature is one whitespace split over text you already have — **zero new PDF passes**.

**Lightweight Trick:**
- No new struct needed if you expose individual methods; `DocumentStats` is for UI convenience only.
- CJK ranges: CJK Unified Ideographs (4E00–9FFF), Hiragana (3040–309F), Katakana (30A0–30FF), Hangul Syllables (AC00–D7AF).

**Test Cases (`crates/youpdf-core/tests/word_count.rs`):**
```rust
#[test]
fn english_word_split() { assert_eq!(index.word_count(), "hello world".split_whitespace().count()); }
#[test]
fn cjk_character_count() { assert_eq!(index.cjk_count(), 5); /* "你好世界！" */ }
#[test]
fn stats_struct() { /* words + cjk + spaces + total */ }
```

**UI Hook (Wave 2 B5):**
- Two `Grid`s in the existing Properties window — no new "Document Stats" window needed.

---

### Agent A6 — Security Permissions Matrix

**File:** `crates/youpdf-core/src/protect/permissions.rs` (new, ~50 LoC)

**Plan:**
```rust
pub enum Permission {
    Print, ChangeDocument, Assemble, CopyContent,
    ExtractPages, Comment, FillForms, Sign,
    CreateTemplatePages, HighQualityPrint,
}

pub fn permissions_from_flags(p: i32) -> Vec<(Permission, bool)>;
pub fn flags_from_permissions(perms: &[(Permission, bool)]) -> i32;

// Rendering helper for UI
pub fn permission_label(p: Permission) -> &'static str;
```

**Thought:**
- `protect.rs` already writes these flags. This is a *reader* for data the crate already owns — pure decode, no policy.
- `/P` is a 32-bit integer with known bit positions — no `bitflags` dependency needed.

**Lightweight Trick:**
- One match over known bit positions (bits 3-12 for permissions, 2 for revision).
- `ponytail:` comment: *"No UI for CreateTemplatePages/HighQualityPrint (rare); add if enterprise workflows need it."*

**Test Cases (`crates/youpdf-core/tests/permissions.rs`):**
```rust
#[test]
fn decode_print_flag() { assert!(permissions_from_flags(0b1100).contains(&(Permission::Print, true))); }
#[test]
fn encode_all_allowed() { assert_eq!(flags_from_permissions(&[(Permission::Print, true), ...]), 0b1111111100); }
#[test]
fn roundtrip() { /* encode → decode → assert equal */ }
```

**UI Hook (Wave 2 B5):**
- Read-only `Grid` in Properties window: 10 rows × 2 columns (Permission Name | Allowed/Denied).

---

## Wave 2 — UI Integration (6 Parallel Agents, Single File Ownership)

### Agent B1 — Page Label Editor

**File:** `crates/youpdf-app/src/app/inspector_ui.rs` (extend, ~40 LoC)

**Plan:**
```rust
pub(super) fn page_label_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
    // ComboBox: NumStyle (Decimal, Roman Upper, Roman Lower, Letters Upper, Letters Lower, None)
    // TextEdit: prefix
    // DragValue: start page (1-indexed)
    // Live preview: "Page 5 → " + get_label(labels, 5)
    // Button: "Apply to Selected Pages"
}
```

**Lightweight Trick:**
- Reuse the existing inspector layout (label + widget pairs).
- No dialog — inline panel, instant apply.

---

### Agent B2 — Page Resize & Exchange Dialog

**File:** `crates/youpdf-app/src/app/pages_ui.rs` (extend, ~60 LoC)

**Plan:**
```rust
pub(super) fn resize_page_dialog(&mut self, ctx: &egui::Context) {
    // 4-box UI shape (like crop margins): Width × Height, or preset (A4, Letter, Legal)
    // Button: "Resize Selected Pages"
}

pub(super) fn interleave_exchange_buttons(&mut self, ui: &mut egui::Ui) {
    // Button: "Interleave (Even First)" | "Interleave (Odd First)"
    // Button: "Exchange Ranges…" → opens range picker dialog
}
```

**Lightweight Trick:**
- Reuse the crop-margin 4-box UI shape — same layout, different fields (W × H instead of margins).
- Presets = `vec![("A4", 595.0, 842.0), ("Letter", 612.0, 792.0), ("Legal", 612.0, 1008.0)]`.

---

### Agent B3 — Link Appearance Inspector

**File:** `crates/youpdf-app/src/app/annots_ui.rs` (extend, ~35 LoC)

**Plan:**
```rust
// In the link inspector panel:
ui.checkbox(&mut appearance.visible, "Visible Border");
ui.add(DragValue::new(&mut appearance.width).range(0.0..=10.0).suffix(" pt"));
ComboBox::from_label("Style").selected_text(style_label).show_ui(ui, |ui| {
    ui.selectable_value(&mut appearance.dash, None, "Solid");
    ui.selectable_value(&mut appearance.dash, Some(vec![3.0, 2.0]), "Dashed");
});
color_picker(&mut appearance.color);
```

**Lightweight Trick:**
- 4 widgets, no new panel. Default values pre-filled from existing link.

---

### Agent B4 — Redaction Code Sets Checklist

**File:** `crates/youpdf-app/src/app/redact_ui.rs` (extend, ~45 LoC)

**Plan:**
```rust
CollapsingHeader::new("Code Sets").show(ui, |ui| {
    for (i, entry) in DEFAULT_CODE_SETS.iter().enumerate() {
        ui.checkbox(&mut self.code_set_enabled[i], entry.name);
        ui.label(RichText::new(entry.example).weak().small());
    }
    if ui.button("Mark All Matches on Current Page").clicked() {
        self.pending_marks.extend(mark_code_matches(doc, page, enabled_sets));
    }
});
```

**Lightweight Trick:**
- Fill colour is **already wired** — just add the collapsible header.
- No "repeat across pages" yet (that's Wave 3 scope if needed).

---

### Agent B5 — Properties Window Stats & Permissions

**File:** `crates/youpdf-app/src/app/windows_ui.rs` (extend, ~50 LoC)

**Plan:**
```rust
// In properties_window():
CollapsingHeader::new("Statistics").show(ui, |ui| {
    ui.grid("stats").num_columns(2).show(ui, |ui| {
        ui.label("Words:"); ui.label(stats.words.to_string());
        ui.label("CJK Characters:"); ui.label(stats.cjk.to_string());
        ui.label("Spaces:"); ui.label(stats.spaces.to_string());
        ui.label("Total Characters:"); ui.label(stats.total_chars.to_string());
    });
});

CollapsingHeader::new("Security Permissions").show(ui, |ui| {
    ui.grid("permissions").num_columns(2).show(ui, |ui| {
        for (perm, allowed) in permissions {
            ui.label(permission_label(perm));
            ui.label(if *allowed { "✓ Allowed" } else { "✗ Denied" });
        }
    });
});
```

**Lightweight Trick:**
- Two `CollapsingHeader`s in a window that already exists — no new windows.

---

### Agent B6 — Push-Button Action Dispatch

**File:** `crates/youpdf-app/src/app/forms_editor.rs` (extend, ~40 LoC)

**Plan:**
```rust
// In the Actions tab of the form field inspector:
ui.group(|ui| {
    ui.label("Button Actions:");
    if ui.button("Reset Form").clicked() {
        // Call existing form-clear logic
        tab.doc.reset_form_fields();
    }
    if ui.button("Submit Form").clicked() {
        // Write /AA << /U << /S /SubmitForm /F (url) >> >>
        // Show toast: "Submit requires a server URL — not implemented in local reader"
    }
    if ui.button("Show/Hide Field").clicked() {
        // Toggle /F 4 (hidden flag)
        field.flags ^= 4;
    }
});
```

**Lightweight Trick:**
- Reset = existing form-clear logic (already in `forms/mod.rs`).
- Submit = write `/AA` dictionary + toast (skip JS engine, skip network).
- Show/Hide = toggle one bit.

---

## Wave 3 — New Subsystems (4 Parallel Agents)

### Agent C1 — Read-Aloud / TTS

**File:** `crates/youpdf-core/src/capture/tts.rs` (new, ~40 LoC)

**Plan:**
```rust
use std::process::{Child, Command};

pub struct TtsEngine {
    child: Option<Child>,
    current_voice: String,
    rate: f32,  // words per minute
}

impl TtsEngine {
    pub fn list_voices() -> Vec<String> {
        // Parse `say -v ?` output: "Alice  # English"
    }
    
    pub fn speak(&mut self, text: &str) -> Result<()> {
        self.stop();  // Kill previous
        self.child = Some(Command::new("say")
            .arg("-v").arg(&self.current_voice)
            .arg("-r").arg(self.rate.to_string())
            .arg(text)
            .spawn()?);
        Ok(())
    }
    
    pub fn stop(&mut self) {
        if let Some(child) = self.child.take() {
            let _ = child.kill();
        }
    }
    
    pub fn is_playing(&self) -> bool {
        self.child.as_ref().map(|c| c.try_wait().map_or(true, |r| r.is_none())).unwrap_or(false)
    }
}
```

**Thought:**
- **Shell out to macOS `say`.** No `objc2`, no AVFoundation bindings, no new dependency in `Cargo.toml`.
- It's already on every Mac, it already has the voice catalogue, and it already handles queueing.

**Lightweight Trick:**
- The entire engine is ~30 LoC and one `Option<Child>`.
- Voice picker = parse `say -v ?` lines (format: `Name  # Language`).
- Speed = a slider → `-r` (default 175 wpm, range 100-300).
- `ponytail:` comment: *"Windows/Linux stubs return `Err("Platform not supported")`; add `speech-dispatcher` / `SAPI` later if cross-platform TTS is needed."*

**UI Hook (you wire in Wave 3 commit):**
- Reader strip: Play/Pause/Stop buttons + voice dropdown + speed slider.
- Highlight current sentence (optional, future enhancement).

---

### Agent C2 — Presenter View

**File:** `crates/youpdf-app/src/app/slideshow_ui.rs` (extend, ~120 LoC)

**Plan:**
```rust
pub struct PresenterView {
    start_time: Instant,
    show_next_page: bool,
    show_timer: bool,
}

pub(super) fn presenter_window(&mut self, ctx: &egui::Context) {
    egui::Window::new("Presenter View")
        .collapsible(false)
        .resizable()
        .show(ctx, |ui| {
            // Top bar: elapsed time, current page / total
            let elapsed = Instant::now() - self.presenter.start_time;
            ui.label(format!("{}:{:02} · Page {} of {}", 
                elapsed.as_secs() / 60, 
                elapsed.as_secs() % 60,
                self.current_page + 1,
                self.doc.page_count()));
            
            ui.horizontal(|ui| {
                // Left: current page (large)
                if ui.button("◀ Prev").clicked() { self.prev_page(); }
                if ui.button("Next ▶").clicked() { self.next_page(); }
                
                // Right: next page thumbnail (small)
                if self.presenter.show_next_page {
                    let next = (self.current_page + 1).min(self.doc.page_count() - 1);
                    let thumbnail = self.render_thumbnail(next, 200.0);
                    ui.image(thumbnail);
                }
            });
        });
}
```

**Thought:**
- `slideshow_ui.rs` and the render cache already exist — presenter view is just *a second viewport on pages you're already rendering*.
- No new rendering path — reuse the thumbnail renderer.

**Lightweight Trick:**
- Reuse the thumbnail renderer for the next-page preview rather than a full-res second page.
- Timer = `Instant::now() - start`.
- Toggle presenter view with a checkbox in the slideshow toolbar.

---

### Agent C3 — Open Quickly (⌘⇧O Launcher)

**File:** `crates/youpdf-app/src/app/home_ui.rs` (extend, ~50 LoC)

**Plan:**
```rust
pub(super) fn open_quickly_panel(&mut self, ctx: &egui::Context) {
    egui::Window::new("Open Quickly")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 100.0])
        .show(ctx, |ui| {
            let response = ui.add(TextEdit::singleline(&mut self.open_query).desired_width(400.0));
            response.request_focus();
            
            // Fuzzy match over recents
            let mut matches: Vec<_> = self.recents.iter()
                .filter_map(|r| {
                    let score = subsequence_score(&r.path.display().to_string(), &self.open_query);
                    if score > 0 { Some((r, score)) } else { None }
                })
                .collect();
            matches.sort_by_key(|(_, s)| -*s);
            
            ui.vertical(|ui| {
                for (recent, _score) in matches.iter().take(12) {
                    if ui.selectable_label(false, &recent.path.display().to_string()).clicked() {
                        self.open_path(&recent.path);
                        self.open_quickly_open = false;
                    }
                }
            });
        });
}

fn subsequence_score(haystack: &str, needle: &str) -> usize {
    // Simple: count matched run length
    let mut h = haystack.chars().peekable();
    let mut score = 0;
    for n in needle.to_lowercase().chars() {
        while let Some(&ch) = h.peek() {
            h.next();
            if ch.to_lowercase().next() == Some(n) {
                score += 1;
                break;
            }
        }
    }
    if score == needle.chars().count() { score } else { 0 }
}
```

**Thought:**
- No `fuzzy-matcher` dependency. A 25-line subsequence matcher scores well enough for a recents list of ~50 entries.
- You're scoring a list that already fits in L1 cache — no need for an index.

**Lightweight Trick:**
- Score = matched-run-length, sort, take 12.
- Note the ⌘F collision: `menu_tree.txt` shows UPDF uses ⌘F for *both* Find and Open Quickly; pick ⌘⇧O and move on rather than emulating a bug.
- Window anchor = center-top (like Spotlight).

---

### Agent C4 — Create from Capture

**File:** `crates/youpdf-core/src/capture/screenshot.rs` (new, ~40 LoC)

**Plan:**
```rust
use std::process::Command;
use std::path::PathBuf;

pub fn capture_region_interactive() -> Result<PathBuf> {
    let temp = std::env::temp_dir().join(format!("screencapture-{}.png", timestamp()));
    Command::new("screencapture")
        .arg("-i")  // interactive region
        .arg("-x")  // no sound
        .arg(&temp)
        .status()?;
    if temp.exists() { Ok(temp) } else { Err(Error::CaptureFailed) }
}

pub fn capture_window(window_id: u32) -> Result<PathBuf> {
    let temp = std::env::temp_dir().join(format!("screencapture-{}.png", timestamp()));
    Command::new("screencapture")
        .arg("-l").arg(window_id.to_string())
        .arg("-x")
        .arg(&temp)
        .status()?;
    if temp.exists() { Ok(temp) } else { Err(Error::CaptureFailed) }
}

pub fn clipboard_as_png() -> Result<Vec<u8>> {
    // osascript -e 'the clipboard as «class PNGf»'
    let out = Command::new("osascript")
        .arg("-e").arg("the clipboard as «class PNGf»")
        .output()?;
    if out.status.success() { Ok(out.stdout) } else { Err(Error::ClipboardNotPng) }
}
```

**Thought:**
- `screencapture` **is** the macOS capture API — it already gives you the interactive crosshair, window-targeting (`-l`), and screen selection.
- Reimplementing that with `CGDisplayCreateImage` bindings is weeks of work for a feature the OS ships as a binary.

**Lightweight Trick:**
- The whole "PDF from screen/window/selection" row collapses to `Command::new("screencapture").arg("-i")` + the existing image→PDF path.
- ID-card capture = same call, two regions — skip it, it's a mobile feature.
- `ponytail:` comment: *"Windows: use `SnippingTool.exe /clip`; Linux: `flameshot gui -r` — add `#[cfg]` branches later."*

**UI Hook (you wire in Wave 3 commit):**
- "Create…" menu → "From Screen Capture" (⌘⇧S), "From Window", "From Clipboard".

---

## Wave 4 — AI Gaps (2 Parallel Agents)

### Agent D1 — AI Writing Tools & AI Check Pages

**Files:** `crates/youpdf-ai/src/tasks.rs` (extend), `crates/youpdf-app/src/ai.rs` (extend, ~120 LoC total)

**Plan:**
```rust
// tasks.rs
pub enum AiTask {
    // Existing: Chat, Summarize, Translate, KeyPoints, Explain, LegalReview, FinancialReview
    Polish,
    Shorten,
    Expand,
    Proofread,
    Tone { style: ToneStyle },  // Professional, Casual, Confident, Friendly
}

pub enum ToneStyle { Professional, Casual, Confident, Friendly }

impl AiTask {
    pub fn prompt_template(&self) -> &'static str {
        match self {
            AiTask::Polish => "Improve the grammar, clarity, and flow of the following text:\n\n{text}",
            AiTask::Shorten => "Condense the following text while preserving key information:\n\n{text}",
            AiTask::Expand => "Elaborate on the following text with additional detail and context:\n\n{text}",
            AiTask::Proofread => "Identify and correct any errors in the following text:\n\n{text}",
            AiTask::Tone { style } => format!("Rewrite the following text in a {} tone:\n\n{{text}}", style.as_str()),
            // ... existing tasks
        }
    }
}

// ai.rs: Check Pages detectors
pub enum PageDefect {
    Blank,
    Invalid,
    NeedsRotation { degrees: i32 },
    NonConsecutive { expected: u32, found: u32 },
    ContainsComments,
    MismatchedDimensions { expected: (f32, f32), found: (f32, f32) },
    Scanned,
}

pub fn detect_page_defects(doc: &Document, page: usize) -> Vec<PageDefect> {
    let mut defects = Vec::new();
    
    // Blank = empty SearchIndex text
    if index.page_text(page).trim().is_empty() {
        defects.push(PageDefect::Blank);
    }
    
    // Scanned = existing looks_scanned heuristic
    if doc.looks_scanned(page) {
        defects.push(PageDefect::Scanned);
    }
    
    // Dimensions = compare /MediaBox to document majority
    let media = doc.page_media_box(page);
    let majority = doc.majority_media_box();
    if (media.0 - majority.0).abs() > 10.0 || (media.1 - majority.1).abs() > 10.0 {
        defects.push(PageDefect::MismatchedDimensions { expected: majority, found: media });
    }
    
    // Comments = annot count > 0
    if doc.page_annot_count(page) > 0 {
        defects.push(PageDefect::ContainsComments);
    }
    
    // Rotation = majority-aspect vote
    let majority_rotation = doc.majority_rotation();
    let page_rotation = doc.page_rotation(page);
    if page_rotation != majority_rotation {
        defects.push(PageDefect::NeedsRotation { degrees: (majority_rotation - page_rotation) * 90 });
    }
    
    defects
}
```

**Thought:**
- The writing tools are **prompt strings, not code** — the enum + dropdown plumbing is already built, so each tool is ~4 lines.
- Check Pages is almost free too: 5 of 7 detectors are one-liners over APIs that already exist.

**Lightweight Trick:**
- 5 of 7 detectors are one-liners. Only "non-consecutive" needs thinking (and only once page labels land — sequence it after A1).
- Skip the one-click rotation repair; show the page list and let the existing rotate button do it.
- `ponytail:` comment: *"NonConsecutive detector requires PageLabels (A1); stub returns empty until A1 lands."*

**UI Hook (you wire in Wave 4 commit):**
- AI dropdown: 5 new tasks + existing 7.
- "Check Pages" button in the toolbar → shows a list of defects with "Go to Page" buttons.

---

### Agent D2 — Whole-PDF Bilingual Translation

**Files:** `crates/youpdf-app/src/ai.rs` (extend), `crates/youpdf-core/src/office.rs` (reuse text placement, ~150 LoC)

**Plan:**
```rust
// ai.rs
pub struct TranslationOptions {
    pub target_language: String,
    pub bilingual: bool,  // side-by-side original + translation
    pub preserve_layout: bool,  // shrink-to-fit, no reflow
}

pub async fn translate_pdf(doc: &mut Document, options: TranslationOptions, provider: &AiProvider) -> Result<()> {
    for page in 0..doc.page_count() {
        // 1. Extract text runs per block (existing search index gives bounding boxes)
        let runs = extract_text_runs(doc, page);
        
        // 2. Batch translate (one request per page, not per run)
        let prompt = format!("Translate the following text to {}. Preserve line breaks:\n\n{}", 
            options.target_language, 
            runs.iter().map(|r| r.text).collect::<Vec<_>>().join("\n"));
        let translated = provider.chat(&prompt).await?;
        let translated_lines: Vec<_> = translated.lines().collect();
        
        // 3. For each run, white-out original and place translation
        for (i, run) in runs.iter().enumerate() {
            if options.preserve_layout {
                // Shrink font size until translation fits in original box
                let mut font_size = run.font_size;
                while !fits_in_box(&translated_lines[i], font_size, run.bbox) && font_size > 4.0 {
                    font_size -= 0.5;
                }
                
                // White-out original
                fill_rect(doc, page, run.bbox, Color::WHITE);
                
                // Place translation
                place_text(doc, page, &translated_lines[i], run.bbox, font_size, run.font.clone());
                
                if options.bilingual {
                    // Place original below translation, offset by half box height
                    let bilingual_bbox = [run.bbox[0], run.bbox[1] - run.bbox[3] / 2.0, run.bbox[2], run.bbox[3] / 2.0];
                    place_text(doc, page, &run.text, bilingual_bbox, font_size * 0.8, run.font.clone());
                }
            }
        }
    }
    Ok(())
}
```

**Thought:**
- Reuse `office.rs`'s `w:framePr` lesson rather than writing a layout engine: **don't reflow.**
- Fit the translation into the original box by shrinking font size until it fits, exactly as a translator would.
- Reflow is the trap that makes this a quarter-long project.

**Lightweight Trick:**
- Bilingual mode is trivial once the overlay exists — draw original then translated, offset by half the box height.
- Send one batched request per page, not per run, to keep the quota sane.
- Accept that the result is approximate and label it as such.
- `ponytail:` comment: *"No reflow engine; complex layouts (multi-column, tables) may overlap. Add reflow if professional translation workflows need it."*

**UI Hook (you wire in Wave 4 commit):**
- Translate menu → "Whole PDF (Preserve Layout)" → dialog with target language + bilingual checkbox.

---

## Wave 5 — Strategic Product Decisions (No Code Agents)

| Gap | Recommendation | Rationale |
|---|---|---|
| **Office Import** | Shell to `soffice --convert-to pdf` when LibreOffice is present; else show install prompt. | OOXML readers are 10k+ LoC each (docx, xlsx, pptx). LibreOffice is free, cross-platform, and already perfect at this. |
| **Validating PDF/A** | Scope as PDF/A-2b with an embedded sRGB ICC profile and `/OutputIntent` (skip full validation engine). | Full validation = color-space analysis, font embedding checks, metadata validation. Buy the sRGB ICC from a public source (e.g. LittleCMS) and write `/OutputIntent << /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB) >>`. |
| **Element Align/Arrange Panel** | Blocked on multi-select; single-object arrange is already done. | Align/arrange requires 2+ selected objects. Multi-select is a canvas interaction overhaul — do that first or don't build the panel. |
| **PDF Portfolio** | Treat as `/Collections` wrapper over existing `/EmbeddedFiles` attachments. | You already have attachments. `/Collections` is just a navigation layer — worth ~1 day once anything else touches collections; not before. |

---

## Rules to Hand Every Subagent

1. **Own one file.** Report the menu/toolbar entries you need; never edit `toolbar_ui.rs` or `i18n.rs` yourself.
2. **Engine + test, not engine + UI.** Wave 1 agents ship a core function and one `tests/<name>.rs` assert. That's the check ponytail requires for branch/loop logic.
3. **Mark deliberate ceilings** with `ponytail:` comments naming the upgrade path (annotation rescale in A2, reflow in D2).
4. **No new dependencies without asking.** `say` and `screencapture` are the pattern: platform binary before crate.
5. **Each wave lands green:** `cargo check --workspace --all-targets`, `cargo test --workspace`, then you run the i18n extractor and wire the menus.

---

## Sequencing Constraints

- **D1's "non-consecutive pages" detector needs A1** (Page Labels).
- **A2 before B2**, **A4 before B4**, **A3 before B3**, etc. — Wave 2 strictly follows Wave 1.
- **Everything in Wave 3 is independent** of Waves 1–2 and can start immediately.
- **Wave 4 is independent** of Wave 3 (AI stack is isolated).

---

## Effort Reality

| Wave | Agents | Total LoC (est.) | Gaps Closed |
|---|---|---|---|
| 0 | — | 20 | Prep |
| 1 | 6 | ~800 | 14 |
| 2 | 6 | ~350 | 6 |
| 3 | 4 | ~250 | 4 |
| 4 | 2 | ~270 | 7 |
| **Total** | **18** | **~1,690** | **31** |

Waves 1+3 are where the value density is — roughly 1,000 lines total for ~18 gaps, most of it reusing code that already exists. Wave 5 is where the honest answer is "not yet."
