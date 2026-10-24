//! Module `annotation` — helpers de transformation de coordonnées.
//!
//! Les annotations sont dessinées en repère affiché (celui du canevas, rotation
//! et recadrage visibles) mais écrites en repère page dans le PDF. Ce module
//! centralise les conversions entre ces deux repères, ainsi que la recherche de
//! lien sous un point.

pub mod comments;
pub mod transform;