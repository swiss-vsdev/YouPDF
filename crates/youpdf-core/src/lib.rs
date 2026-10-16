//! # youpdf-core
//!
//! Moteur PDF de YouPDF. **Rust pur**, aucun service tiers.
//!
//! ## Modules
//!
//! - [`document`] : ouverture, lecture, sauvegarde
//! - [`metadata`] : métadonnées PDF
//! - [`render`]   : rendu de pages en bitmap via pdfium
//! - [`search`]   : recherche plein texte
//! - [`ocr`]      : OCR + réinjection PDF consultable
//! - [`convert`]  : conversion PDF → autres formats
//! - [`annotate`] : annotations PDF natives
//! - [`forms`]    : formulaires AcroForm
//! - [`redact`]   : caviardage destructif
//! - [`protect`]  : chiffrement RC4-128 (rév. 3) et AES-256 (rév. 6)
//! - [`edit`]     : extraction de texte positionné et d'images
//! - [`signature`]: signature visuelle et demande de signature
//! - [`compare`]  : comparaison de deux documents
//! - [`watermark`]: filigranes, en-têtes et numéros de page

pub mod annotate;
pub mod compare;
pub mod convert;
pub mod document;
pub mod edit;
pub mod forms;
pub mod metadata;
pub mod organize;
pub mod ocr;
pub mod protect;
pub mod redact;
pub mod render;
pub mod search;
pub mod signature;
pub mod structure;
pub mod watermark;

pub use annotate::{Annotation, AnnotationKind, AnnotationStyle, STAMP_NAMES, stamp_name};
pub use compare::{ComparisonReport, Difference, DifferenceKind};
pub use document::Document;
pub use edit::{ImageInfo, TextFragment, TextSpan, encode_pdf_literal};
pub use forms::{FormField, FormFieldKind};
pub use metadata::DocumentMetadata;
pub use ocr::{OcrEngine, OcrMode, OcrResult, TesseractEngine, validate_language};
pub use protect::{EncryptionOptions, EncryptionStrength, Permissions};
pub use redact::RedactionRegion;
pub use render::{PageBitmap, PdfiumRenderer, RenderOptions, Renderer};
pub use search::{SearchIndex, SearchOptions, SearchResult, fold_case};
pub use signature::{SignaturePlacement, SignatureRequest, SignatureStatus};
pub use structure::{AttachmentInfo, OutlineItem, decode_pdf_text};
pub use watermark::{HeaderFooter, NumberStyle, PageRange, Watermark};
