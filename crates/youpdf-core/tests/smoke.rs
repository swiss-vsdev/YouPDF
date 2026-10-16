//! Test de fumée : plus petit vérifiable qui échoue si la logique casse.
//!
//! Couvre les chemins non triviaux du noyau : ouverture/texte,
//! aller-retour des annotations, index de recherche, comparaison.

use youpdf_core::{Annotation, AnnotationKind, Document, SearchIndex};

/// Construit un PDF d'une page contenant `text` (via lopdf, sans fichier).
fn tiny_pdf(text: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use lopdf::{content::Content, dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");

    // Police Helvetica standard.
    let font_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });

    // Contenu : une ligne de texte.
    let content = Content {
        operations: vec![
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
            lopdf::content::Operation::new("Td", vec![100.into(), 700.into()]),
            lopdf::content::Operation::new("Tj", vec![Object::string_literal(text)]),
            lopdf::content::Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().map_err(|e| e.to_string())?,
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });

    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page_id.into()]),
        "Count" => 1,
    });

    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });

    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

#[test]
fn document_opens_and_reads_text() {
    let bytes = tiny_pdf("Hello YouPDF").expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    assert_eq!(doc.page_count(), 1);
    let text = doc.all_text().expect("extraction");
    assert!(
        text.contains("Hello YouPDF"),
        "texte extrait inattendu : {text:?}"
    );
}

#[test]
fn annotation_roundtrip() {
    let bytes = tiny_pdf("annotable").expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    let ann = Annotation::new(1, AnnotationKind::Highlight, [50.0, 50.0, 200.0, 80.0]);
    let id = ann.id.clone();
    doc.apply_annotations(&[ann]).expect("application");

    let read = doc.read_annotations();
    assert_eq!(
        read.len(),
        1,
        "une annotation doit survivre à l'aller-retour"
    );
    assert_eq!(read[0].id, id);
    assert_eq!(read[0].kind, AnnotationKind::Highlight);

    // Le second passage remplace (et ne duplique pas) les annotations.
    doc.apply_annotations(&[]).expect("nettoyage");
    assert!(doc.read_annotations().is_empty());
}

#[test]
fn search_and_compare() {
    let bytes = tiny_pdf("Bonjour le monde").expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");

    let index = SearchIndex::build(&doc).expect("index");
    let hits = index.search("monde", 10);
    assert!(!hits.is_empty(), "« monde » doit être trouvé");
    assert_eq!(hits[0].page, 1);

    // Un document identique ne produit aucune différence.
    let other = Document::from_bytes(&bytes).expect("ouverture");
    let report = doc.compare_with(&other).expect("comparaison");
    assert!(
        report.is_identical(),
        "différences inattendues : {:?}",
        report.differences
    );
}

/// Le chiffrement AES-256 (révision 6) doit produire la mise en forme
/// /U, /O de 48 octets (hash + 2 sels) et /UE, /OE de 32 octets.
#[test]
fn aes256_encrypt_layout() {
    use youpdf_core::{EncryptionOptions, EncryptionStrength, Permissions};

    let bytes = tiny_pdf("secret").expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    doc.encrypt(&EncryptionOptions {
        user_password: "user".into(),
        owner_password: "owner".into(),
        permissions: Permissions::all(),
        strength: EncryptionStrength::Aes256,
    })
    .expect("chiffrement");

    let out = doc.to_bytes().expect("sérialisation");
    let parsed = lopdf::Document::load_mem(&out).expect("relecture");
    let encrypt_ref = parsed
        .trailer
        .get(b"Encrypt")
        .and_then(|o| o.as_reference())
        .expect("référence /Encrypt");
    let encrypt = parsed
        .get_dictionary(encrypt_ref)
        .expect("dictionnaire /Encrypt");

    assert_eq!(encrypt.get(b"V").unwrap().as_i64().unwrap(), 5);
    assert_eq!(encrypt.get(b"R").unwrap().as_i64().unwrap(), 6);
    assert_eq!(
        str_len(encrypt.get(b"U").unwrap()),
        48,
        "/U = hash + 2 sels"
    );
    assert_eq!(
        str_len(encrypt.get(b"O").unwrap()),
        48,
        "/O = hash + 2 sels"
    );
    assert_eq!(
        str_len(encrypt.get(b"UE").unwrap()),
        32,
        "/UE = clé chiffrée"
    );
    assert_eq!(
        str_len(encrypt.get(b"OE").unwrap()),
        32,
        "/OE = clé chiffrée"
    );
}

/// Régression : la bibliothèque doit être découverte dans `pdfium/` à la
/// racine du dépôt même quand le binaire vit dans `target/debug/deps/`, et
/// doit se charger (sur macOS, un fichier marqué « quarantine » ou à la
/// signature invalide faisait retomber le renderer sur `NullRenderer`).
#[test]
fn pdfium_library_is_found_and_loaded() {
    let lib = "libpdfium".to_string();
    let mut dir = std::env::current_dir().expect("cwd");
    let repo_pdfium = loop {
        let cand = dir.join("pdfium");
        if cand.is_dir() {
            break cand;
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => break std::path::PathBuf::from("/nonexistent-pdfium"),
        }
    };

    let present = repo_pdfium.join(format!("{lib}.dylib")).is_file()
        || repo_pdfium.join(format!("{lib}.so")).is_file()
        || repo_pdfium.join("pdfium.dll").is_file();

    if !present {
        eprintln!(
            "pdfium absent de {} : exécutez ./scripts/install-pdfium.sh",
            repo_pdfium.display()
        );
        return;
    }

    youpdf_core::PdfiumRenderer::load()
        .expect("pdfium doit se charger depuis ./pdfium (sinon : quarantine/signature)");
}

/// Le rendu pdfium doit produire de vrais pixels (et non le gabarit gris
/// de `NullRenderer`) : on vérifie que du contenu non uniforme apparaît.
#[test]
fn pdfium_renders_real_pixels() {
    use youpdf_core::{PageBitmap, PdfiumRenderer, RenderOptions, Renderer};

    if youpdf_core::PdfiumRenderer::load().is_err() {
        eprintln!("pdfium indisponible : rendu réel non testé");
        return;
    }
    let bytes = tiny_pdf("Render me").expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");

    let renderer = PdfiumRenderer::load().expect("chargement pdfium");
    let bmp: PageBitmap = renderer
        .render_page(
            &doc,
            1,
            &RenderOptions {
                scale: 1.0,
                ..Default::default()
            },
        )
        .expect("rendu");

    assert!(bmp.width > 0 && bmp.height > 0, "bitmap vide");
    let distinct = bmp
        .pixels
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2]])
        .collect::<std::collections::HashSet<_>>();
    assert!(
        distinct.len() > 1,
        "rendu uniforme ({distinct:?}) : pdfium n'a pas dessiné le texte"
    );
}

fn str_len(obj: &lopdf::Object) -> usize {
    obj.as_str().map(|b| b.len()).unwrap_or(0)
}
