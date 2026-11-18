# YouPDF — UPDF 2.5.6 Parity Gap Analysis


**Last updated:** November 2026
**Code state:** Phase 5 refactoring complete — `youpdf-core` split into `organize/`, `signature/`, `ocr/`, `redact/`, `forms/`, `edit/`, `watermark/`, `protect/`, `render/`, `annotate/` modules.

---

## Legend

- 🟡 **Partial** — Core logic present; interface or edge cases missing
- ❌ **Missing** — Not implemented

---

## Parity Overview

|---|---|---|---|

---

## Open Parity Gaps (High Priority)

### Tier 1 — High value, low effort

|---|---|---|---|

### Tier 2 — High value, medium effort

|---|---|---|

### Tier 3 — High value, high effort (or product decisions)

|---|---|---|

---

## Closed Gaps (Since Last Audit)

| Gap | Commit | Notes |
|---|---|---|
| **Task 4.2 — Content element arrange** | `e21c6f6` | Drag-to-move, corner resize, free-angle rotation, z-order. |
| **Task 12.2 — Compare view modes** | `c37981a` | Side-by-side, overlay blend, per-page highlights. |
| **Task 14.2 — i18n wiring** | `f1a6f64` | 365-entry EN/FR dictionary, `t()` throughout UI. |
| **Digital signature window + cert manager** | — | `sign_ui.rs`: `sign_window`, `certificate_panel`, `validate_signatures`. |
| **Redaction extras** | — | `app/redact_ui.rs` + `app/annots_ui.rs`: code sets (8 `DEFAULT_CODE_SETS` incl. U.S. Privacy Act / FOIA), `mark_code_matches`, repeat-mark-across-pages, auto-size-to-region via `redact::fit_block`. |
| **Measure extras** | — | `core/measure.rs`: `MeasureKind::{Azimuth,Altitude}` (degrees) + `Measurement::label`; `app/measure_ui.rs` + `app/canvas_commit.rs` wired. Angle label `90.0°`. Snap-to-path/rulers deferred. |
| **Link appearance line style** | — | `app/inspector_ui.rs`: solid/dashed/dotted via `LinkAppearance.dash`; visible-rect, width, colour already wired. |
| **Security Properties matrix** | — | `app/pages_ui.rs` `security_properties_window` + `Protect ▸ Security Properties…` menu item; decodes `/Encrypt` `/P` flags via `core/protect/permissions.rs`. |
| **Open Quickly launcher** | — | Already present: `app/home_ui.rs` `open_quickly_panel` (⌘F). |
| **Word count / stats, Read Aloud** | — | Already present: `app/windows_ui.rs:329` `DocumentStats` (words/CJK/spaces/total); `app/readaloud_ui.rs` + `core/capture/tts.rs` (voice list, speed slider, play/stop). |


---

## Verification Commands

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy -p youpdf-app -p youpdf-core --all-targets
python3 scripts/i18n_extract.py --check   # i18n coverage gate
```

---
