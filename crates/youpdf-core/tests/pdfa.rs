//! Conformité déclarée (PDF/A, PDF/X, PDF/E) et PDF d'image.
//!
//! On vérifie que les marqueurs normatifs sont bien présents après
//! `apply_conformance` et que le paquet XMP se décompresse en texte contenant
//! l'identifiant du niveau. Pour `save_image_pdf`, on passe par un vrai
//! `PdfiumRenderer` ; s'il est indisponible, le test est ignoré (le chemin
//! d'erreur est couvert séparément par `NullRenderer`).

use lopdf::Object;
use youpdf_core::{blank_document, Conformance, Document};

const LEVELS: [Conformance; 5] = [
    Conformance::PdfA1b,
    Conformance::PdfA2b,
    Conformance::PdfX1a,
    Conformance::PdfX4,
    Conformance::PdfE1,
];

/// Charge un document lopdf depuis les octets sérialisés.
fn reload(doc: &Document) -> lopdf::Document {
    let bytes = doc.to_bytes().expect("sérialisation");
    lopdf::Document::load_mem(&bytes).expect("relecture lopdf")
}

/// Dictionnaire pointé par `key` dans le catalogue, en résolvant la référence.
fn catalog_entry<'a>(doc: &'a lopdf::Document, key: &[u8]) -> &'a lopdf::Dictionary {
    let catalog = doc.catalog().expect("catalogue");
    let value = catalog.get(key).expect("entrée catalogue absente");
    match value {
        Object::Reference(id) => doc.get_dictionary(*id).expect("dictionnaire référencé"),
        Object::Dictionary(dict) => dict,
        other => panic!("{key:?} n'est pas un dictionnaire : {other:?}"),
    }
}

#[test]
fn apply_conformance_pose_les_marqueurs_de_chaque_niveau() {
    for level in LEVELS {
        let mut doc = blank_document(2, 300.0, 400.0).expect("document vierge");
        doc.apply_conformance(level).expect("application de la conformité");

        let raw = reload(&doc);
        let catalog = raw.catalog().expect("catalogue");
        assert!(catalog.has(b"Metadata"), "{level:?} : /Metadata absent");
        assert!(
            catalog.has(b"OutputIntents"),
            "{level:?} : /OutputIntents absent"
        );
        assert!(catalog.has(b"MarkInfo"), "{level:?} : /MarkInfo absent");

        // /MarkInfo << /Marked true >>
        let mark_info = catalog_entry(&raw, b"MarkInfo");
        assert_eq!(
            mark_info.get(b"Marked").and_then(Object::as_bool).ok(),
            Some(true),
            "{level:?} : /Marked n'est pas true"
        );

        // /OutputIntents : un seul intent, du bon sous-type.
        let catalog = raw.catalog().expect("catalogue");
        let intents = catalog
            .get(b"OutputIntents")
            .and_then(Object::as_array)
            .expect("tableau /OutputIntents");
        assert_eq!(intents.len(), 1, "{level:?} : un seul intent attendu");

        let intent = match &intents[0] {
            Object::Reference(id) => raw.get_dictionary(*id).expect("intent référencé"),
            Object::Dictionary(dict) => dict,
            other => panic!("{level:?} : intent inattendu : {other:?}"),
        };
        let expected_subtype: &[u8] = match level {
            Conformance::PdfX1a | Conformance::PdfX4 => b"GTS_PDFX",
            _ => b"GTS_PDFA1",
        };
        assert_eq!(
            intent.get(b"S").and_then(Object::as_name).ok(),
            Some(expected_subtype),
            "{level:?} : sous-type /S incorrect"
        );
        let identifier = intent
            .get(b"OutputConditionIdentifier")
            .and_then(Object::as_str)
            .expect("/OutputConditionIdentifier");
        assert_eq!(
            identifier,
            level.identifier().as_bytes(),
            "{level:?} : identifiant"
        );
    }
}

#[test]
fn xmp_decompresse_et_nomme_le_niveau() {
    for level in LEVELS {
        let mut doc = blank_document(1, 300.0, 400.0).expect("document vierge");
        doc.apply_conformance(level).expect("application");

        let raw = reload(&doc);
        let catalog = raw.catalog().expect("catalogue");
        let metadata_id = catalog
            .get(b"Metadata")
            .and_then(Object::as_reference)
            .expect("/Metadata doit être une référence");
        let stream = raw
            .get_object(metadata_id)
            .and_then(Object::as_stream)
            .expect("/Metadata doit être un flux");

        // Le flux est bien compressé en FlateDecode.
        assert_eq!(
            stream.dict.get(b"Filter").and_then(Object::as_name).ok(),
            Some(b"FlateDecode".as_slice()),
            "{level:?} : /Filter attendu FlateDecode"
        );

        let text = String::from_utf8(stream.decompressed_content().expect("décompression XMP"))
            .expect("XMP UTF-8");
        assert!(
            text.contains(level.identifier()),
            "{level:?} : l'identifiant n'apparaît pas dans le XMP"
        );

        // Marqueur normatif par niveau.
        match level {
            Conformance::PdfA1b => {
                assert!(text.contains("<pdfaid:part>1</pdfaid:part>"), "pdfaid:part");
            }
            Conformance::PdfA2b => {
                assert!(text.contains("<pdfaid:part>2</pdfaid:part>"), "pdfaid:part");
                assert!(
                    text.contains("<pdfaid:conformance>B</pdfaid:conformance>"),
                    "pdfaid:conformance"
                );
            }
            Conformance::PdfX1a => {
                assert!(
                    text.contains("<pdfx:GTS_PDFXVersion>PDF/X-1a:2001</pdfx:GTS_PDFXVersion>"),
                    "GTS_PDFXVersion"
                );
            }
            Conformance::PdfX4 => {
                assert!(
                    text.contains("<pdfx:GTS_PDFXVersion>PDF/X-4</pdfx:GTS_PDFXVersion>"),
                    "GTS_PDFXVersion"
                );
            }
            Conformance::PdfE1 => {
                assert!(
                    text.contains("<pdfe:ISO_PDFEVersion>PDF/E-1</pdfe:ISO_PDFEVersion>"),
                    "ISO_PDFEVersion"
                );
                assert!(
                    text.contains("pdfaExtension:schemas"),
                    "schéma d'extension PDF/E"
                );
            }
        }
    }
}

#[test]
fn id_du_trailer_est_deux_fois_seize_octets_identiques() {
    let mut doc = blank_document(1, 300.0, 400.0).expect("document vierge");
    doc.apply_conformance(Conformance::PdfA2b).expect("application");

    let raw = reload(&doc);
    let ids = raw
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .expect("/ID tableau");

    assert_eq!(ids.len(), 2, "/ID doit contenir deux valeurs");
    let bytes = |obj: &Object| match obj {
        Object::String(bytes, _) => bytes.clone(),
        other => panic!("/ID inattendu : {other:?}"),
    };
    let first = bytes(&ids[0]);
    let second = bytes(&ids[1]);
    assert_eq!(first.len(), 16, "/ID doit faire 16 octets");
    assert_eq!(first, second, "les deux /ID doivent être identiques");
}

#[test]
fn le_fichier_se_reouvre_avec_le_meme_nombre_de_pages() {
    for level in LEVELS {
        let mut doc = blank_document(3, 300.0, 400.0).expect("document vierge");
        doc.apply_conformance(level).expect("application");

        let bytes = doc.to_bytes().expect("sérialisation");
        let reopened = Document::from_bytes(&bytes).expect("réouverture");
        assert_eq!(reopened.page_count(), 3, "{level:?} : nombre de pages");
    }
}

#[test]
fn save_image_pdf_ecrit_une_page_par_page() {
    let Some(renderer) = youpdf_core::PdfiumRenderer::load().ok() else {
        eprintln!("pdfium indisponible : PDF d'image non testé (chemin d'erreur couvert)");
        return;
    };

    let doc = blank_document(2, 300.0, 400.0).expect("document vierge");
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let path = dir.path().join("images.pdf");

    doc.save_image_pdf(&path, &renderer, 72.0)
        .expect("écriture du PDF d'image");

    let reopened = Document::open(&path).expect("réouverture");
    assert_eq!(reopened.page_count(), 2, "une page par page d'origine");
    for page in 1..=2 {
        let (w, h) = reopened.page_size(page);
        assert!(
            (w - 300.0).abs() < 1.0 && (h - 400.0).abs() < 1.0,
            "page {page} : {w}×{h}, attendue 300×400"
        );
    }

    // Chaque page porte bien un XObject image nommé /Im0.
    let bytes = std::fs::read(&path).expect("lecture");
    let raw = lopdf::Document::load_mem(&bytes).expect("relecture lopdf");
    for (_, page_id) in raw.get_pages() {
        let page = raw.get_dictionary(page_id).expect("page");
        let resources = page
            .get(b"Resources")
            .and_then(Object::as_dict)
            .expect("ressources");
        let xobjects = resources
            .get(b"XObject")
            .and_then(Object::as_dict)
            .expect("XObject");
        assert!(xobjects.has(b"Im0"), "image /Im0 absente de la page");
    }
}

#[test]
fn save_image_pdf_refuse_un_rendu_sans_contenu() {
    let doc = blank_document(1, 300.0, 400.0).expect("document vierge");
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let path = dir.path().join("vide.pdf");

    let result = doc.save_image_pdf(&path, &youpdf_core::render::NullRenderer, 72.0);
    assert!(result.is_err(), "NullRenderer doit être refusé");
    assert!(!path.exists(), "aucun fichier ne doit être écrit");
}
