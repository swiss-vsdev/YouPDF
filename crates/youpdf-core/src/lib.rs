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
//! - [`compress`] : recompression des flux et objets inutilisés
//! - [`watermark`]: filigranes, en-têtes et numéros de page
//! - [`util`]     : aides partagées (zlib, échappement XML/CSV)

pub mod annotate;
pub mod annotation;
pub mod compare;
pub mod compress;
pub mod convert;
pub mod create;
pub mod crop;
pub mod document;
pub mod geom;
pub mod edit;
pub mod extract;
pub mod forms;
pub mod links;
pub mod measure;
pub mod metadata;
pub mod organize;
pub mod ocr;
pub mod office;
pub mod pdfa;
pub mod protect;
pub mod redact;
pub mod render;
pub mod search;
pub mod signature;
pub mod structure;
pub mod text;
pub mod units;
pub mod util;
pub mod watermark;

pub use annotate::{
    export_comments, smooth_polyline, stamp_name, Annotation, AnnotationKind, AnnotationState,
    AnnotationStyle, CommentFormat, STAMP_NAMES,
};
pub use compare::{ComparisonReport, Difference, DifferenceKind};
pub use convert::{ImageFormat, TextFormat};
pub use create::{blank_document, document_from_image_bytes, document_from_images};
pub use crop::CropMargins;
pub use compress::{CompressionExtras, CompressionReport};
pub use document::{Document, PageBoxes, PageView};
pub use edit::{
    encode_pdf_literal, estimate_text_width, patch_font_name, ImageCrop, ImageInfo, ImageTransform,
    PatchFont, TextAlign, TextChar, TextFragment, TextSpan, TextStyle,
};
pub use extract::{ExtractedTable, KeyValue};
pub use forms::{FieldProperties, FormDataFormat, FormField, FormFieldKind, NewFormField};
pub use links::{LinkAction, PageLink};
pub use measure::{MeasureKind, Measurement};
pub use metadata::DocumentMetadata;
pub use ocr::{
    LANGUAGES, OcrEngine, OcrLanguage, OcrMode, OcrResult, TesseractEngine, combine_languages,
    language_name, validate_language,
};
pub use office::OfficeFormat;
pub use pdfa::Conformance;
pub use protect::{EncryptionOptions, EncryptionStrength, Permissions};
pub use redact::{
    RedactionFill, RedactionMark, RedactionOverlay, RedactionRegion, RedactionReport,
};
pub use render::{PageBitmap, PdfiumRenderer, RenderOptions, Renderer};
pub use search::{
    SearchIndex, SearchOptions, SearchResult, find_query, fold_case, is_whole_word, match_rects,
    query_matches, replace_occurrences,
};
pub use signature::{SignaturePlacement, SignatureRequest, SignatureStatus};
pub use structure::{
    decode_pdf_text, AttachmentInfo, OutlineEntry, OutlineItem, OutlineStyle, OutlineTarget,
};
pub use units::Unit;
pub use watermark::{
    Background, BandSlots, Bates, BatesPosition, HeaderFooter, NumberStyle, PageRange, Watermark,
};
