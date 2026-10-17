//! # youpdf-shared
//!
//! Types fondamentaux partagés par tous les crates YouPDF.
//!
//! Ce crate **ne dépend d'aucune logique métier** : uniquement des
//! structures de données, des erreurs et des jetons de thème. Il est
//! volontairement léger pour pouvoir être utilisé partout sans créer
//! de dépendance circulaire.

pub mod error;
pub mod theme;

pub use error::{Error, Result};
pub use theme::{pal, set_palette, Palette, Theme};
