//! Annotations PDF natives.
//!
//! ## Approche retenue
//!
//! YouPDF crée de **véritables annotations PDF** (entrées dans le tableau
//! `/Annots` de chaque page), plutôt que de dessiner sur le contenu de la
//! page. C'est l'approche standard des éditeurs PDF professionnels :
//!
//! - les annotations restent éditables dans n'importe quel lecteur ;
//! - elles peuvent être listées, filtrées, exportées séparément ;
//! - le contenu original de la page n'est pas modifié.
//!
//! ## Types supportés
//!
//! | Type          | Subtype PDF  |
//! |---------------|--------------|
//! | Surlignage    | `/Highlight` |
//! | Soulignage    | `/Underline` |
//! | Barré         | `/StrikeOut` |
//! | Note          | `/Text`      |
//! | Texte libre   | `/FreeText`  |
//! | Rectangle     | `/Square`    |
//! | Cercle        | `/Circle`    |
//! | Crayon        | `/Ink`       |
//!
//! ## Marquage
//!
//! Toutes les annotations créées par YouPDF portent un `/NM` (nom) qui
//! commence par `youpdf-`. Cela permet de les identifier et de les
//! remplacer sans toucher aux annotations créées par d'autres outils.


mod render;
mod types;

pub use render::{
    dynamic_stamp_text, quad_from_rect, smooth_polyline, stamp_id_for_pdf_name, stamp_name,
    stamp_template,
};
pub use types::*;


pub(crate) use render::push_text_literal;

// ────────────────────────────────────────────────────────────────────────
// Intégration dans un Document
// ────────────────────────────────────────────────────────────────────────

/// Préfixe d'identification des annotations créées par YouPDF.
const NM_PREFIX: &str = "youpdf-";
mod ops;
mod write;


mod export;

pub use export::export_comments;
