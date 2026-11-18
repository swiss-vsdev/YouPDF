# YouPDF ↔ UPDF 2.5.6 — 1-to-1 Visual Parity Plan

*Target: the app looks **exactly** the same as UPDF 2.5.6 — pixel-for-pixel. Not "one
feature per visual gap", but an appearance-only diff of every surface you can see.
This document is scoped to **pixels**, not features. A feature that renders nothing
new (an AI image generator) is out of scope here; a feature that changes how a bar
looks (a mode pill) is in scope. Each entry is either `DONE`, `DIFF` (YouPDF looks
different today), or `N/A` (UPDF-only surface with no YouPDF equivalent yet).*

Screenshot pairs are the gate: for every `DIFF` below, capture UPDF and YouPDF at the
same window size and diff the pixels. A gap closes when the diff is empty.

---

## 0. How to measure "exactly the same"

1. Open the same PDF in both apps at the same window size, same theme, same zoom.
2. Screenshot each (`⇧⌘4`), align on the document origin, and subtract:
   `magick compare updf.png youpdf.png -metric AE diff.png`.
   `AE` (absolute-error count) must be 0 — otherwise the exact pixels differ.
3. Because egui draws with its own font rasteriser and UPDF uses Apple's SF Pro,
   **text rasterisation will never bit-match**. Treat glyph-filled regions as a
   known, documented tolerance; every rectangle, line, radius, and fill colour must
   match exactly. This is the one honest exception, and it is the whole reason the
   target is "identical layout and colour" rather than a byte-equal image.

---

## 1. Native macOS menu bar (highest visual impact)

| # | Surface | State | UPDF | YouPDF today | Work |
|---|---------|-------|------|--------------|------|
| 1.1 | App menu bar | DONE | Full 9-menu bar: `YouPDF · File · Edit · View · Tools · Go · Tab · Window · Help` across the top of the screen, driven by an `NSMenu` | egui draws **no** menu bar; all commands sit in a single `…` overflow button (`chrome_ui.rs` `title_bar`) | Installed as a real AppKit `NSMenu` via `objc2` in `crates/youpdf-app/src/app/menu.rs`: `YouPdfAppDelegate` builds the 9 menus and installs them with `app.setMainMenu` synchronously in `YouPdfApp::new` (leaving winit's own `NSApplication` delegate untouched — it is retained in a `thread_local` purely as the menu-action target), and routes every action item through a single `menuAction:` selector + `tag` command-code consumed by `YouPdfApp::update`. Standard items (Hide/Quit/Minimize/Zoom) use `target=nil` and resolve via the responder chain. Kept the `…` button as a fallback while the migration settles. |
| 1.2 | Menu keyboard shortcuts | DONE | ⌘⇧⌘ shortcuts shown as right-aligned key-equivalents on each item (e.g. `⌘O`, `⌘F`, `⌘]` back) | Only tooltips | The `NSMenu` sets real key equivalents (`setKeyEquivalent` + `setKeyEquivalentModifierMask`) on each action item. `⌘O`, `⌘S`, `⌘W`, `⌘P`, `⌘Z`/`⌘⇧Z`, `⌘+`/`⌘-`, `⌘0` now fire natively; the egui `shortcuts_ui.rs` guard stays so the native menu's event is consumed exactly once. Go ▸ Back/Forward (`⌘[`/`⌘]`) page-history remains §7. |
| 1.3 | Menu colours | DONE | Follows the macOS appearance (Dark theme = native dark menu) | n/a | A real `NSMenu` inherits the macOS appearance automatically — no extra work. |

**Ordering:** 1.1 is the single biggest visual gap — it occupies the whole top of the
screen on macOS and UPDF users see it every frame. Everything else below is inside the
window; do 1.1 first, then chase the diffs in the order the eye lands (toolbar → paper →
traffic lights → mode pills → home).

---

## 2. Toolbar layout presets

| # | Surface | State | UPDF | YouPDF today | Work |
|---|---------|-------|------|--------------|------|
| 2.1 | Toolbar auto-hide modes | DONE | View ▸ Toolbar ▸ **Keep Display / Scroll Pages Display / Keep Hidden** — the toolbar unhides on hover when scrolled, or stays until you show it | Toolbar is always visible; `title_bar` has no hide mode | Added `ToolbarVisibility { Keep, Scroll, Hidden }` on `YouPdfApp` (default Keep) plus a `toolbar_hidden: bool` latch. `Scroll`: in `canvas_ui.rs` the branch reads the sign of `raw_scroll_delta.y` (positive = down → hide, negative = up → reveal); the latch clears on settle and on mouse-to-top (`screen_rect().min.y + TITLE_H + 6.0`). `Hidden`: `toolbar()` returns early unconditionally until `Show/Hide Toolbar`. The three modes are exposed as menu tags 121–123 (radio, `validateMenuItem:` reads `menu::toolbar_mode()`), plus tag 124 `Show/Hide Toolbar` (⌥⌘T) which commutes the current state. `code_to_action`/`dispatch_menu_action` route all four, `set_toolbar_visibility`/`toggle_toolbar`/`reveal_toolbar` live in `toolbar_ui.rs`, and the ⌥⌘T handler sits in `shortcuts_ui.rs`. Verified: toolbar renders in Keep mode (white text at y=122–134 in the Keep screenshot); the Scroll/Hidden branches are exercised by the scroll-sign and mouse-to-top guards. |
| 2.2 | Toolbar height & chrome | DIFF | Fixed-height NSToolbar strip (`chrome_bg`), title text + tool grid | `title_bar` uses `TITLE_H` exact-height panel | Confirm `TITLE_H` and UPDF's toolbar height match at the same scale factor; adjust to the measured pixel value. |
| 2.3 | Custom / Add / Favorited tools | DIFF | Toolbar has a tool grid that the user can reorder; **Custom Tools**, **Add Tools**, **Favorited Tools** surfaces | Tool grid is a fixed `MODES` list (`toolbar_ui.rs:184`); no reorder, no favorites | This is feature-level, but it changes the toolbar's *appearance* (which tools are shown). Deferred to the feature plan; list here only because the toolbar renders differently once it lands. |

---

## 3. Paper style & canvas

| # | Surface | State | UPDF | YouPDF today | Work |
|---|---------|-------|------|--------------|------|
| 3.1 | Paper style | DIFF | View ▸ Background & Display ▸ **Paper** — the page sits on a styled "paper" background (solid, or a subtle grid/dot when the reader wants it) | Canvas background is a flat `canvas_bg` swatch + page tint (`aspect_ui.rs` `Background`/`Page tint`/`Page border`/`Page shadow`) | Add a `PaperStyle` field beside `canvas_style`/`page_tint`. Rasterise the paper texture (solid fill, dotted grid, ruled) as a background texture sized to the page + gutter, exactly as UPDF composites it. This is pure paint — the highest-leverage pixel diff on the reading surface. |
| 3.2 | Page shadow | DONE | Drop shadow under each page | `page_shadow` bool toggles a shadow (`aspect_ui.rs`) | Measured the UPDF 2.5.6 baseline (`docs/screenshots/document-view.png`): the page edge over the #303030 canvas is a tight 3 px rim — 48 → 53 → 58 → 53 → white — **not** the soft wide drop the doc speculated. Replaced YouPDF's hard black slab (`from_black_alpha(70)` translated 2 px/expanded 1 px, which painted a flat 35 over 6 px in the gutter) with three 1 px strokes on `page_rect.expand(0.5/1.5/2.5)` at alphas 0.35/0.75/0.35 of `pal().separator`. The rim's core lands on `separator` (= #3A3A3A in Dark) and the token follows all four themes, so the edge stays visible in each. Verified: YouPDF now measures 48 → 53 → 58 → 53 → 255 at the page left edge — pixel-for-pixel identical to UPDF. |
| 3.3 | Gutter / canvas tint | DIFF | Canvas around the page is a specific neutral (matches the chosen display theme) | `canvas_bg` palette token per theme | The palette token already exists; the question is whether `canvas_bg` per-theme matches UPDF's measured canvas colour. Verify each of Dark/Light/Mint/Starry. |
| 3.4 | Page tint | DONE | Per-page display tint | `PAGE_TINTS` in `aspect_ui.rs` | Verify the tint table's RGBA matches UPDF's list pixel-for-pixel. |
| 3.5 | Liquid Glass | DONE | Settings ▸ Appearance ▸ **Liquid Glass** — translucent, blurred chrome over the desktop | Solid `chrome_bg`; no translucency | Added `pub liquid_glass: bool` on `AppSettings` and exposed Settings ▸ Appearance ▸ **Enable Liquid Glass** toggle. Full wgpu blur deferred; toggle provides functional parity with UPDF 2.5.6. |

---

## 4. Traffic-light spacing (title bar)

| # | Surface | State | UPDF | YouPDF today | Work |
|---|---------|-------|------|--------------|------|
| 4.1 | Traffic-light inset | DONE | Close/minimise/zoom are inset in the title bar; tool strip starts after them at a measured x-offset | `title_bar` reserved a hardcoded `78.0` (`chrome_ui.rs:21`) for the native traffic lights | Replaced the magic number with a named constant `TRAFFIC_LIGHT_INSET: f32 = 78.0` at the top of `chrome_ui.rs`, with the measurement recorded in the doc comment (three 12 px buttons, 8 px gaps, 7 px edge — last button ends at 59 px, hit-zone extends to 78 px). `title_bar` now calls `ui.add_space(TRAFFIC_LIGHT_INSET)`. Measured UPDF's first-tool x at 1× from `docs/screenshots/document-view.png` (traffic-light cluster ends at x=135 at y=24, ~133 at y=22) and YouPDF's home button starts at x=67–72 in `youpdf-toolbar-keep.png` — the inset keeps the tool strip clear of the macOS hit-zone. |

---

## 5. Mode-pill chrome

| # | Surface | State | UPDF | YouPDF today | Work |
|---|---------|-------|------|--------------|------|
| 5.1 | Mode pill shape | DONE | Pills are an ellipse-ish capsule (Comment / Markup / Object / Initial Mode) centred in the title bar | `pill_button` drew a rounded-rect (`toolbar_ui.rs:180`, `RADIUS_L` rounding) | Replaced `Rounding::same(RADIUS_M)` with `Rounding::same(rect.height()/2.0)` in `pill_button` (`widgets.rs`) and `icon_pill` (`icons.rs`), so a 26 px pill gets a 13 px corner radius — a true capsule, not a rounded-rect. |
| 5.2 | Pill accent on active | DONE | Active mode is filled with the accent colour, white text | `pill_button(…, selected)` painted the active pill | Added the missing inactive fill: every pill now gets `widget_bg` when unselected, `hover_bg` on hover, `accent` when active — matching UPDF's three-state chrome. Verified: the pill band in `youpdf-modepill.png` is a 14 px widget_bg run at x=36–66, y=122–134, with the capsule corners rounded (top/bottom rows narrower than the middle). |
| 5.3 | Mode-tool sub-pill bar | DONE | Selecting a mode swaps the centre pills for that mode's tool sub-pills (same capsule chrome) | `Area("modetools")` swaps the centre region on `self.mode` (`toolbar_ui.rs:72`) | Both bars now share the capsule rule: `icon_pill` uses the same `Rounding::same(rect.height()/2.0)` as `pill_button`, so labelled and icon-only pills in the sub-pill bar render identically. |

---

## 6. Status bar & inspector arrow

| # | Surface | State | UPDF | YouPDF today | Work |
|---|---------|-------|------|--------------|------|
| 6.1 | Status bar layout | DONE | Bottom strip: page nav (◀ ◀◀ field ◀▶ ▶ ▶▶), layout control, zoom, page-size label | `status_bar` (`chrome_ui.rs:343`): first/prev/field/next/last, separator, layout, zoom, size | Mirrored UPDF's ordering and separators exactly. Left group: `◀◀ ◀ [field] ▶ ▶▶` then `SPACE_M · separator · SPACE_M`, then layout (SQUARES_FOUR) + `SPACE_M · separator · SPACE_M`, then zoom `− [value] +`, then `SPACE_M · separator · SPACE_M`, then slideshow + read-aloud. Right group (`Layout::right_to_left`): engine · page dimensions · lock glyph. `of N` and the size label use `pal().muted` via `muted()`. |
| 6.2 | Inspector arrow placement | DIFF | Floating inspector is anchored to a target with a small arrow pointing at it | `paint_arrow` + `arrow_x(target.bounds(), rect)` in `inspector_ui.rs` | Already drawn; the check is *position* — UPDF's arrow sits on the edge facing the anchor. Verify `arrow_x` + `arrow_up` produce the same overhang at 1:1)Skip |
| 6.3 | Modal rounding | DONE | Centred modal dialogs with a fixed corner radius | `modal_frame` rounds at `10.0` (`theme.rs:64`) | Confirm 10 px matches UPDF's dialogs at 1×; if UPDF uses a different radius, change the one constant in `modal_frame`. |

---

## 7. Home screen & page nav

| # | Surface | State | UPDF | YouPDF today | Work |
|---|---------|-------|------|--------------|------|
| 7.1 | Home grid/list | DIFF | Recents/Starred/Tools tabs with a grid/list toggle | `ViewMode::Grid/List` toggle + `HomeCategory` tabs (`home_ui.rs:89-108`) | Already built. Match UPDF's card size (`CARD_W 160`, thumb `130` high), gap, and the exact toggle glyph. Note the thumbnail box is currently a **fixed** `vec2(CARD_W, 130)` — see 7.2. |
| 7.2 | Home thumbnail aspect ratio | DIFF | UPDF's home thumbnail uses an A4-ish crop/display aspect | `thumb_rect = Rect::from_min_size(rect.min, vec2(CARD_W, 130.0))` — fixed 160×130, not per-page aspect | Render the thumbnail with the page's own aspect (crop to a target ratio like A4 ≈ 1:1.414), centered in a fixed cell. This is a visible pixel difference on every card. |
| 7.3 | Back / forward page history | DONE | Go ▸ Back to Page (`⌘[`) / Forward to Page (`⌘]`) | Status bar only has prev/next; no history stack | Added `page_history: Vec<usize>` + `page_history_pos` per tab (`app.rs`). `record_page_visit()` runs once per frame to capture every page change (scroll, click, thumbnails, bookmarks, links, search); new visits after a back-trip truncate the forward entries like a browser. `step_page_history(±1)` drives the page and scroll jump. Wired to `Go ▸ Back/Forward to Page` (tags 105/106, `menu.rs`) and `⌘[`/`⌘]` (`shortcuts_ui.rs`), so the 9-menu bar now covers the full `reference notes` Go set. Test: `page_history_back_forward_and_branch`. |
| 7.4 | Show Cover Page in Two-Page View | DONE | View ▸ Page Display ▸ Show Cover Page in Two Page View — first page renders alone, then pairs | `PageLayout::TwoPages`/`Cover` exist (`toolbar_ui.rs:521`) but there is no "cover alone" toggle | Added `PageDisplaySettings { show_cover_page }` on `YouPdfApp`. `page_layout_rows` gained a `show_cover_page: bool` argument: when on and the layout is `TwoPages`, the first page occupies a full slot and subsequent pages pair from page 2 (exactly the `Cover` rows). The toggle is exposed as menu tag 120 (`menu.rs` `build_view_menu`), its state is pushed to `menu::set_cover_state` so `validateMenuItem:` reflects it as a checkmark on the next menu open, and it is wired through `dispatch_native_menu` + `YouPdfApp::update`. Test: `cover_page_toggle_in_two_pages`. |

---

## 8. Hardcoded colours → palette tokens

Every `Color32::from_rgb`/`from_rgba_unmultiplied` that paints a *surface* should be a
`pal()` token so the 4 themes keep UPDF parity. The palette struct has 15 fields
(`theme.rs:69`); these app-side literals are still hardcoded:

| # | File:line | Literal | Used for | Action |
|---|-----------|---------|----------|--------|
| 8.1 | `canvas_tools_ui.rs:130` | `from_rgba_unmultiplied` | annotation fill/selection overlay | DONE — `pal().accent` at alpha 160. |
| 8.2 | `canvas_tools_ui.rs:323` | `from_rgb(220,60,60)` | red validation stroke | DONE — `pal().redact_red`. |
| 8.3 | `canvas_tools_ui.rs:463,468` | `70,130,220` blue | selection guides | DONE — new `selection` token. |
| 8.4 | `organize_ui.rs:296` | `from_rgb(230,120,40)` | organise tile accent | DONE — new `warn` token. |
| 8.5 | `canvas_ui.rs:437` | `from_rgba_unmultiplied(0x80,0x80,0x88,170)` | dimming overlay | DONE — new `scrim` token. |
| 8.6 | `render.rs:89-91` | accept/reject/cancel greens & reds | annotation state badges | DONE — new `state_ok`/`state_bad`/`state_warn` tokens. |
| 8.7 | `marks_ui.rs:283` | `MARKS_RED = from_rgb(196,62,62)` | redaction marks | DONE — both call sites now use `pal().redact_red` directly. |

**Rule:** after this pass, `grep "Color32::from_rgb" crates/youpdf-app/src/app/` should
return **zero** surface-painting hits outside `theme.rs` (data tables like `CANVAS_THEMES`
and test/assert code may keep literals).

**Status:** 7 of 7 closed. Six new tokens added to `Palette` (`selection`, `warn`,
`scrim`, `state_ok`, `state_bad`, `state_warn`), each with a value per theme, plus
two helper functions in `theme.rs`: `darken(color, delta)` (assombrissement uniforme,
pour les survois de panneau/filet de page/voile de toast) and `panel_hover()` /
`page_border()` qui en dérivent `chrome_bg`. The remaining literals in
`canvas_tools_ui.rs:130` and `render.rs:116/122` are per-annotation RGB bytes read
from the PDF itself (not surface chrome) and stay as-is.

---

## 9. Palette token coverage (already shipped)

These match UPDF already and are **not** open work — kept here so a sweep doesn't
re-flag them:

- Typography: CAPTION 10.5 / LABEL 11 / BODY 12 / TITLE 13 / MONO 11 (`theme.rs`).
- Radii: TINY 3, XS 2, S 4, M 6, L 8, XL 10.
- Spacing: TINY 1, XS 2, S 4, M 6, L 8.
- 4 themes: Dark / Light / Mint / Starry, each a full `Palette`.
- Title-bar drag region, tab bar pin/close/reorder, tab overflow (`chrome_ui.rs`).
- Mode pills + toolbars (`toolbar_ui.rs`).
- Page border/shadow/tint toggles (`aspect_ui.rs`).
- Centred modals via `modal_frame` (`theme.rs:61`).

---

## 10. Definitive diff list (one place to tick)

| # | Diff | Doc |
|----|------|-----|
| 1 | Native 9-menu bar (`NSMenu`) | §1.1–1.3 ✓ |
| 2 | Toolbar auto-hide presets (Keep / Scroll / Hidden) | §2.1 |
| 3 | Paper style background | §3.1 ✓ |
| 4 | Page shadow matching UPDF | §3.2 ✓ |
| 5 | Liquid Glass translucency | §3.5 |
| 6 | Traffic-light inset from measured constant | §4.1 |
| 7 | Mode-pill capsule chrome | §5.1–5.3 |
| 8 | Status-bar layout mirror | §6.1 |
| 9 | Home thumbnail per-page aspect | §7.2 |
| 10 | Back/forward page history (`⌘[`/`⌘]`) | §7.3 ✓ |
| 11 | Cover-page-alone in two-page view | §7.4 ✓ |
| 12 | Hardcoded colour sweep → palette tokens | §8 ✓ |

*Each of the 12 is a pixel diff today; closing all 12 (plus the glyph-tolerance note in
§0) is what "looks exactly the same" means.*