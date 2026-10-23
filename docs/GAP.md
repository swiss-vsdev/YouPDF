# YouPDF — UPDF 2.5.6 Parity Gap Analysis


---

## Legend
- 🟡 **Partial** — Core logic present; interface or edge cases missing
- ❌ **Missing** — Not implemented

---

## Parity Overview

|---|---|---|---|

---

## The 3 Open Parity Gaps

These represent all remaining work for total 1-to-1 feature parity:

1. **Task 4.2 — Content Element Arrange**
   - **Status:** Foundation only (`ContentElement`, `ContentDrag`, `ContentKind` types exist in `youpdf-app`).
   - **Missing:** Canvas hit-test dispatch in Edit mode, corner/edge resize handles, rotation handle, drag-to-move/scale, z-order reordering (Bring to Front, Send to Back).

2. **Task 12.2 — PDF Comparison Engine View Modes**
   - **Status:** Text-diff report implemented (`youpdf-core/src/compare.rs`).
   - **Missing:** Dual-document side-by-side synchronized scrolling, difference overlay blend mode, side-by-side difference highlights.

3. **Task 14.2 — Full Localization (i18n) Wiring**
   - **Status:** Dictionary implemented (`youpdf-shared/src/i18n.rs`, 110 entries).
   - **Missing:** Wiring `t("...")` across the remaining UI surfaces (status bar, sidebars, floating inspector, dialogs, toolbars).

---

## Verification Commands

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy -p youpdf-app -p youpdf-core --all-targets
```
