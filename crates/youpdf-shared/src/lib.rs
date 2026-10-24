//! # youpdf-shared
//!
//! Types fondamentaux partagés par tous les crates YouPDF.
//!
//! Ce crate **ne dépend d'aucune logique métier** : uniquement des
//! structures de données, des erreurs et des jetons de thème. Il est
//! volontairement léger pour pouvoir être utilisé partout sans créer
//! de dépendance circulaire.

pub mod error;
pub mod i18n;
pub mod theme;

pub use error::{Error, Result};
pub use i18n::{t, Language};
pub use theme::{
    caption, modal_frame, muted, pal, section, set_palette, Palette, Theme, BODY, CAPTION,
    LABEL, MONO, TITLE, RADIUS_L, RADIUS_M, RADIUS_S, RADIUS_TINY, RADIUS_XS, RADIUS_XL,
    SPACE_L, SPACE_M, SPACE_S, SPACE_TINY, SPACE_XS,
};