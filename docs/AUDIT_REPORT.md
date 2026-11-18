# Phase 1 — Audit report: YouPDF

*Date: 2026-11-18 · Read-only audit · No source files were modified in Phase 1.*

## Scope & state

- **What this is:** a Rust 2021 workspace (`youpdf-shared`, `youpdf-core`, `youpdf-ai`, `youpdf-app`) building an egui/eframe **desktop PDF editor**. There is no server, database, routing layer, or web front end — so web-only categories (CORS, SQL injection, server-side authorization, bucket policies) have no direct analogue. I audited the closest equivalents: untrusted-PDF input handling, OS process/scheme invocation, network/AI endpoints, secret storage, and file writes.
- **Pre-existing state:** the working tree has substantial uncommitted changes (parity work + docs + `measure.rs` tests) and untracked files (`app/menu.rs`, `docs/screenshots/*`, `docs/visual-parity.md`). I treated these as current state and **did not touch them**. `AUDIT_STATUS.md` / `IMPROVEMENTS_SUMMARY.md` are parity trackers, not security audits.
- **Baseline checks run (read-only):** `cargo check --workspace` → passes with 1 warning (`THUMB_RATIO` never used). `cargo test --workspace` → **fails** (9 tests in `edit_image.rs` panic because pdfium cannot be loaded in this checkout). No `cargo-audit` installed, so dependency vulnerability scanning is unverified.

**Severity totals:** 7 high, 20 medium, 17 low across the six categories.

---

## Category A — Security

**A1 · HIGH · `crates/youpdf-app/src/app/files_ui.rs:276-287`, `app/canvas_ui.rs:1060-1065`, `youpdf-core/src/links.rs:142-150`**
A PDF-controlled `/URI` is passed verbatim to `open <url>` / `cmd /C start <url>` / `xdg-open <url>`. There is no scheme allowlist, so a malicious document can trigger `file://` URLs or registered custom-scheme handlers on a single click.
*Risk:* local file disclosure or launching external handlers from untrusted input. *Fix:* allowlist `http`, `https`, `mailto` (and optionally `tel`) in `open_url`/`follow_link`; toast a plain-language "blocked unsafe link" for anything else.

**A2 · MEDIUM · `crates/youpdf-app/src/ai.rs:393-395, 398-426, 496-545, 557-594, 87-92`**
The `ai_enabled` switch is enforced only as an early `return` in the UI (`ai.rs:689`). `send()`, `submit()`, `translate_pdf()` and the worker never check it, and `AiPanel::enabled()` is never called. *Risk:* any non-panel call path (or future code) can build a provider, read the keyring, and send document text while the user believes AI is off. *Fix:* check `enabled()` at the start of every request method and in the worker before `build_provider()`.

**A3 · MEDIUM · `youpdf-ai/src/providers/openai_compat.rs:183,228`, `anthropic.rs:145`, `ollama.rs:107,172,213`**
Provider error strings embed the full raw HTTP response body and URL, and are surfaced directly into UI status labels (`ai.rs:470`) and error toasts. *Risk:* provider/reflected content and internal detail leak to users and logs; inconsistent with the careful URL redaction in `endpoint.rs:113-120`. *Fix:* truncate and sanitize the body (or drop it), keep a short status + generic recovery message; log details at `debug` only.

**A4 · LOW · `crates/youpdf-app/src/app/files_ui.rs:452`**
Print spools via a predictable world-readable temp path `youpdf-print-{pid}.pdf` that is never removed. *Risk:* local symlink/race and temp clutter. *Fix:* use a unique `create_new`/random filename in a private dir and delete after `lp`.

**A5 · LOW · `.github/workflows/release.yml:33,105,110`**
Actions are pinned to mutable tags (`@v4`, `@v2`) and the release job relies on default `GITHUB_TOKEN` permissions. *Risk:* supply-chain drift and over-broad token scope. *Fix:* pin action SHAs and set `permissions: contents: write` on the release job only.

**Done well:** no secrets in tracked files or git history (`GITHUB_TOKEN` is the only reference). Keys live in the OS keyring (`keyring.rs`), never in `settings.json`. `endpoint.rs` rejects non-loopback HTTP, credentials-in-URL, cross-origin and HTTPS→HTTP redirects, and applies a 120 s timeout. OCR language is validated (`ocr/mod.rs:225-253`). All subprocess calls use argv arrays, not shells — no shell injection. No `unsafe` outside the macOS menu FFI.

---

## Category B — Compute and network waste

**B1 · HIGH · `crates/youpdf-app/src/app/windows_ui.rs:329-331`**
`properties_window` calls `SearchIndex::build(&tab.doc)` — extracting and case-folding **every page** — on every repaint, with no memoization. *Risk:* hundreds of ms of work per frame on a large PDF just to show word counts. *Fix:* reuse the cached `self.search_index` for the active tab, or compute once when the window opens.

**B2 · MEDIUM · `app/readaloud_ui.rs:26` + `youpdf-core/src/capture/tts.rs:37-51`**
`TtsEngine::list_voices()` spawns `say -v ?` and parses output on every repaint. *Fix:* `OnceLock<Vec<String>>` or compute once on window open.

**B3 · MEDIUM · `app/readaloud_ui.rs:108-110`**
`ctx.request_repaint_after(250ms)` runs unconditionally, waking the whole UI 4×/s even when playback is idle (and amplifying B1). *Fix:* request the repaint only while playing.

**B4 · MEDIUM · `app/thumbnails_ui.rs:61-107`**
One layout + cache lookup for every page, each frame; egui `ScrollArea` does not cull. *Fix:* `if !ui.is_rect_visible(rect) { ui.add_space(rect.height()); continue; }`.

**B5 · MEDIUM · `crates/youpdf-app/src/ai.rs:119-150, 581-590`**
Whole-PDF translation makes one paid LLM call per page with no cap and no batching, repeating a large system prompt each time; unlike chat, which is capped at 40 pages / 60k chars. *Fix:* batch pages per request with numbered markers, or apply the same cap policy.

**B6 · MEDIUM · `app/decor_ui.rs:104-105`**
`Document::open(src)` reopens and parses the entire watermark source PDF on every frame while the window is open. *Fix:* load once when the path is chosen and cache it.

**B7 · LOW · `Cargo.toml:37`**
`tokio` enables `rt-multi-thread`, but only `new_current_thread()` is used (`ai.rs:76`). *Fix:* drop the feature.

**Done well:** search input is debounced (`search_ui.rs:52-78`); the search index is built once and folded at build time; render pyramid/texture reuse, thumbnail cache, AI text extraction cache, and fragment cache are all present; AI replies use non-blocking `try_recv` and the worker exits cleanly; autosave is time-gated in `update` (no timer thread).

---

## Category C — Code health

**C1 · HIGH · `app.rs:1198-1716`** — `YouPdfApp` is a ~518-line, ~283-field god struct touched by ~60 UI modules. *Risk:* state invariants are impossible to reason about. *Fix:* delete dead fields (C4) now; split into sub-structs only on a concrete collision, not speculatively.

**C2 · HIGH · `app/decor_ui.rs:88`** — `ui.checkbox(&mut app.watermark_pdf_source.is_some(), …)` mutates a temporary `bool`; egui discards the write each frame, so the "Use a PDF file as watermark" control can never be toggled. *Fix:* use a real `bool` field or set `watermark_pdf_source` on `changed()`. (Same temporary trick at `inspector_ui.rs:94` is saved only by an adjacent read.)

**C3 · HIGH · `pages_ui.rs:1006,1013`, `forms_ui.rs:472,526,570`, `signature_ui.rs:331`, `canvas_tools_ui.rs:423`, `files_ui.rs:545,547`** — ignored `Result`s followed by unconditional success toasts. *Risk:* users are told an edit succeeded when the PDF write failed. *Fix:* toast success only in the `Ok` arm; route errors through `Error::user_message()`.

**C4 · MEDIUM · `app.rs:1138-1179,781`, `marks_ui.rs:285`, `inspector.rs:56`, `ai.rs:186`, `redact/geometry/mod.rs` (6 sites), `protect/crypto.rs:307`, `app.rs:183` (`THUMB_RATIO`), `youpdf-ai/.../tasks.rs:105` (`suggested_questions`), `message.rs:53` (`ConversationHistory`), `Cargo.toml:37` (`anyhow`)** — dead code and unused deps hidden behind `#[allow(dead_code)]` or exports; the two AI helpers are advertised in the README but never called. *Fix:* delete after confirming no callers (git history is the backup).

**C5 · MEDIUM · `app.rs:2140-2213`** — ~70 lines of empty `// --- Section ---` scaffolding comments inside `impl YouPdfApp`. *Fix:* delete.

**C6 · MEDIUM · `providers/openai_compat.rs:180-189`, `anthropic.rs:142-151`, `ollama.rs:104-113`** — byte-identical HTTP status/body/JSON handling in three providers. *Fix:* one `parse_response()` helper.

**C7 · MEDIUM · `ai.rs:499-519` vs `558-574`** — `submit()` and `translate_pdf()` duplicate the busy guard, config/model selection, and status strings. *Fix:* extract one `active_model() -> Result<(ProviderConfig,String),&'static str>`.

**C8 · MEDIUM · `ai.rs:686,740,845,853`, `prefs_ui.rs:213`, `toolbar_ui.rs:705`** — six `let _ = settings.save()` with no logging, including the toggle that disables all network access. *Risk:* settings silently revert on restart. *Fix:* log failures with `tracing::warn!`.

**C9 · MEDIUM · `pages_ui.rs:1001`** — `tabs.get(active).unwrap()` in the page-label handler while the next line uses `if let Some`. *Fix:* use the guarded form.

**C10 · MEDIUM · `windows_ui.rs:330`** — `.expect("build index")` on an arbitrary user PDF in a properties dialog. *Fix:* `.ok().map(DocumentStats::from_index)` and show `—`.

**C11 · MEDIUM · `ai.rs:672-1045` (`body()`, 373 lines), `app.rs:1719-~2100` (constructor ~380 lines)** — screen-sized functions. *Fix:* extract clearly separated UI blocks; no new abstractions needed.

**C12 · LOW · `home_ui.rs:286-296,456-466,657-661`** — file metadata→relative-time chain triplicated. *Fix:* one `file_stamp()` helper.

**C13 · LOW · `app/render.rs:265-278`, `app.rs:78-79`** — commented-out `PolyLine` rendering branch, and `#[allow(unused_imports)] pub use icons::*` wildcard re-export. *Fix:* delete / import explicitly.

**C14 · LOW · multiple** — error toasts mix raw English strings, French provider errors, and an existing `Error::user_message()` helper. *Fix:* route errors through `user_message()`; keep success strings in the i18n table.

**Done well:** no `serde_json::Value`/`any` escape hatches; 0 `dbg!`/`TODO`/`unimplemented!` in production; the 17 `unsafe` uses are confined to the macOS `objc2` menu; password-dialog unwraps are provably guarded; PDF parsing's `.ok()` chains are idiomatic handling of untrusted input.

---

## Category D — Reliability and data

**D1 · HIGH · `youpdf-core/src/document.rs:296-305`, `annotate/write.rs:20-34`, `app/files_ui.rs:135,173`**
`Document::save` → `lopdf::Document::save` truncates the target in place (no temp+rename, no `fsync`, no backup). Autosave and ⌘S pass the document's own path. *Risk:* a mid-write failure corrupts the user's original PDF irreversibly. *Fix:* one shared `atomic_save(bytes, path)` helper (write `<path>.youpdf.tmp`, `sync_all`, `rename`).

**D2 · HIGH · `home_ui.rs:352-353, 190-200`**
"Delete File…" calls `std::fs::remove_file` immediately, with no confirmation, trash, or undo; the recents/stars entries are dropped even when deletion fails. *Risk:* one misclick permanently destroys a source PDF. *Fix:* confirmation modal naming the file; drop entries only on success.

**D3 · MEDIUM · `prefs_ui.rs:233-261`**
Autosave uses `.is_ok()` with no toast and never updates `saved_hash`/`history_base` as manual save does. *Risk:* repeated silent failures and stale dirty state after edit→autosave→undo. *Fix:* surface errors, update bookkeeping on success, use the atomic helper.

**D4 · MEDIUM · `forms/import_export.rs:82-88`, `forms_editor.rs:27-35`**
Form-data import applies values in a loop with `?`; a failure mid-loop leaves partial edits in memory with no dirty flag or undo snapshot, which then persist on the next save. *Fix:* apply to a clone and swap only on full success.

**D5 · MEDIUM · `signature/sign.rs:48-56`, `sign_ui.rs:401-409`, `document.rs:296`**
Signing uses a correct incremental update, but any later full save re-serializes via lopdf and recomputes xref/ByteRange offsets. *Risk:* a signature that validated in-session silently fails on disk after any subsequent edit. *Fix:* warn that editing invalidates signatures, or preserve the signed revision on later saves.

**D6 · LOW · AI providers (`openai_compat.rs:218-234` et al.)** — no retry/backoff on transient 5xx/network blips (timeout is handled). *Fix:* retry idempotent GETs and optionally chat once with short backoff.

**D7 · LOW · `capture/screenshot.rs:20,38`, `files_ui.rs:452`** — temp files never deleted. *Fix:* remove after use / RAII.

**D8 · LOW · `config.rs:183-194`, `home.rs:134-146,188-200`** — settings/recents/stars use direct `fs::write` with no atomicity, backup, or schema version. *Fix:* atomic helper; consider a version field.

**D9 · LOW · `batch_types.rs:339-388`** — batch writes are independent; a late failure leaves partial outputs and advances `bates_next`, creating numbering gaps. *Fix:* stage outputs and commit per file only on full success.

**D10 · LOW · `canvas_commit.rs:234-238`, `annots_ui.rs:113,165` vs `comments_ui.rs:33`** — generated dates mix `Local::now()` and `Utc::now()`. *Fix:* pick one zone and document it.

**Done well:** busy-guard prevents duplicate AI submissions; generation counter cancels queued requests; undo history is byte/count bounded and self-healing; annotation writes are idempotent and restore in-memory state; clone-based save helpers don't contaminate working state; signed output uses an increment; batch outputs never overwrite originals; cached serialization strips `/Prev` and `/XRefStm` correctly.

---

## Category E — UI and usability

*Evaluated against the Phase 2 standard, adapted for a desktop app: "responsive" means usable at resizable windows (min 960×620 in `main.rs:19`), not phone layouts.*

**E1 · HIGH · `search_ui.rs:112-123`, `text_edit_ui.rs:312-364`**
"Replace all" rewrites every match immediately: no count preview, no confirmation, and no `mark_document_changed()` — so no undo snapshot is pushed, and the next undo restores the *pre-replace* state. *Fix:* preview count + confirm, then call `mark_document_changed()` after a successful replace.

**E2 · MEDIUM · `ocr_ui.rs:112-127,147-151`**
OCR runs synchronously on the UI thread with no spinner, progress, or disabled state — the window appears hung. Batch mode already does this correctly (`batch_ui.rs:15-20`). *Fix:* set a busy flag + spinner before the blocking call, or move to the worker pattern.

**E3 · MEDIUM · `thumbnails_ui.rs:209`, `page_action_ui.rs:73-80` vs `organize_ui.rs:77,344`**
Two page-delete paths with different safety: the thumbnails context menu deletes instantly while Organize asks for confirmation. *Fix:* route both through `ask_delete_pages`.

**E4 · MEDIUM · `page_action_ui.rs:31,40,66,79`, `attachments_ui.rs:141,234`, `measure_ui.rs:131`, `bookmarks_ui.rs:425`, `password_ui.rs:94,103`, `ocr_ui.rs:142,167`**
Raw `{err}` strings are shown in user-facing toasts — internal jargon with no plain-language recovery step. *Fix:* fixed short messages ("Couldn't delete the pages — try again") with optional details.

**E5 · MEDIUM · `home_ui.rs:237-252,309-326`, `inspector.rs:296-307,412-429`, `widgets.rs:180-212`, `search_ui.rs:32-33,210`**
Custom-drawn cards, colour swatches, rail icons, and icon-only buttons have no accessible names; formatting buttons lack tooltips. *Fix:* give every response an accessible name / `on_hover_text`, or use `Button::new` with text.

**E6 · MEDIUM · `theme.rs:225`, `redact_ui.rs:157,191`**
White 12 px text on accent `#7A7EF6` is ≈3.4:1, below WCAG AA 4.5:1 — affects primary/Confirm buttons. *Fix:* darken the accent token or use dark foreground text on accent.

**E7 · MEDIUM · `ocr_ui.rs:29-31,123,169`** — "Tesseract (not installed)" is internal jargon and "Recognise" is disabled with no install guidance. *Fix:* plain-language message with short enable steps.

**E8 · LOW · `widgets.rs:208`** — hardcoded hover `Color32::from_rgb(0x33,…)` bypasses theme tokens and looks wrong in light themes. *Fix:* use `pal().hover_bg`.

**E9 · LOW · `redact_ui.rs:174`, `batch_ui.rs:80,84,115`, `ocr_ui.rs:29-31`, `decor_ui.rs:156,169`, `measure_ui.rs:100-114`** — critical/help strings bypass `t()` and stay English. *Fix:* wrap with `t()` and add keys.

**E10 · LOW · `prefs_ui.rs:52-58`** — "Default zoom" shows raw values `fit-width`, `fit-page`, `actual`. *Fix:* human labels.

**E11 · LOW · `password_ui.rs:100-105`** — a wrong password clears the field, forcing a full retype. *Fix:* select-all instead.

**E12 · LOW · `home_ui.rs:133-136,502`** — "Clear" recents has no confirmation; the list-view forget ✕ is hover-only. *Fix:* keep ✕ visible (muted); optionally confirm Clear.

**Done well:** no placeholders/TODOs/dead routes; strong empty states throughout; redaction is a model two-phase destructive flow with explicit "cannot be recovered" warning; dirty-tab close confirmation; extensive shortcuts and Enter/Esc handling; centralized palette/type/spacing tokens with 4 themes and inspector clamping tests; recents capped at 8.

---

## Category F — Tests, tooling, and docs

**F1 · HIGH · `cargo test --workspace` (run now); `crates/youpdf-core/tests/edit_image.rs:290,338,388,423,444,468,603`**
The suite **fails in this checkout**: 9 `edit_image` tests panic with `.expect("rendu")` / pdfium config errors because `libpdfium.dylib` is not loadable, while other suites skip gracefully with `eprintln!("pdfium indisponible : test ignoré")`. This contradicts `docs/STATUS.md`'s claim of "47 suites, 0 failures". *Fix:* make these tests skip when pdfium is unavailable (same guard as neighbours), and document the `YOUPDF_PDFIUM_DIR` / `install-pdfium.sh` setup required for the full suite.

**F2 · MEDIUM · `docs/STATUS.md:3`, `AUDIT_STATUS.md`, `IMPROVEMENTS_SUMMARY.md`**
Docs are stale: `STATUS.md` cites HEAD `e8b3a8ea` while the actual HEAD is `1737b870`; the other two describe older waves with unchecked claims. *Fix:* regenerate from the current tree during Phase 5.

**F3 · MEDIUM · `README.md:46-56` vs `youpdf-ai/src/tasks.rs:105`, `message.rs:53`**
README advertises "Chat with history + suggested questions", but `suggested_questions()` and `ConversationHistory` are exported and never used by the app (the panel keeps its own `Vec<Turn>`). *Fix:* either wire them up or correct the README/remove the dead code.

**F4 · LOW · `cargo check --workspace` (run now)** — passes, but with one warning: `app.rs:183` `THUMB_RATIO` is unused (a reverted thumbnail-aspect change left the constant behind). *Fix:* delete it.

**F5 · LOW · repo root / `docs/STATUS.md:11-16`** — no `rustfmt.toml` or Clippy config; `STATUS.md` lists a `cargo clippy …` gate that CI (`.github/workflows/release.yml`) does not run. *Fix:* add the check to CI or drop the claim. `cargo-audit` is not available, so dependency advisories are unverified — run it in CI or locally.

---

## Notes for the next phases

- **Phase 2:** the UI standard was applied with the desktop adaptation above; no phone/tablet layout is expected, and deliberate UPDF-parity visual choices were ignored unless they harmed usability.
- **Phase 3 order** (security → correctness/data → compute waste → code health → UI) works with this report; the highest-value first commits are A1, A2, D1, D2, E1, B1.
- No source files were changed in Phase 1; pre-existing uncommitted work was left intact.
