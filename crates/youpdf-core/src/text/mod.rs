//! Texte positionné : helpers de géométrie de sélection.
//!
//! Ces fonctions opèrent sur des [`TextFragment`](crate::edit::TextFragment)
//! pour calculer la sélection de texte au caractère près (clic mot, clic
//! ligne, glissé) et les rectangles/quadrilatères correspondants. Elles sont
//! indépendantes de l'interface et donc testables unitairement.

pub mod geometry;