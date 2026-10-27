//! Signature PKCS#7 : émission, vérification et détection de modification.

use std::sync::OnceLock;

use youpdf_core::signature::{
    validate_signatures, CertificateRequest, SignOptions, SigningIdentity,
};
use youpdf_core::Document;

/// Petit PDF d'une page, sans dépendance à pdfium.
fn tiny_pdf() -> Vec<u8> {
    use lopdf::{content::Content, dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = Content {
        operations: vec![
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
            lopdf::content::Operation::new("Td", vec![100.into(), 700.into()]),
            lopdf::content::Operation::new("Tj", vec![Object::string_literal("Bonjour")]),
            lopdf::content::Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
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
    let mut buffer = Vec::new();
    doc.save_to(&mut buffer).unwrap();
    buffer
}

/// Une seule clé RSA pour tous les tests (la génération est lente).
fn identity() -> &'static SigningIdentity {
    static IDENTITY: OnceLock<SigningIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        SigningIdentity::create_self_signed(&CertificateRequest {
            common_name: "Test Signer".into(),
            organization: "YouPDF".into(),
            email: "test@example.com".into(),
            country: "FR".into(),
            years: 1,
        })
        .expect("certificat auto-signé")
    })
}

fn options() -> SignOptions {
    SignOptions {
        page: 1,
        rect: [40.0, 40.0, 240.0, 100.0],
        field_name: "Signature1".into(),
        name: "Test Signer".into(),
        reason: "Tests".into(),
        location: "Paris".into(),
        contact: "test@example.com".into(),
        ..Default::default()
    }
}

fn signed_pdf() -> Vec<u8> {
    let document = Document::from_bytes(&tiny_pdf()).expect("document");
    document.sign(identity(), &options()).expect("signature")
}

#[test]
fn signed_document_validates() {
    let bytes = signed_pdf();
    let signatures = validate_signatures(&bytes);
    assert_eq!(signatures.len(), 1, "{signatures:?}");
    let signature = &signatures[0];
    assert!(signature.valid, "{signature:?}");
    assert!(signature.covers_whole_document);
    assert_eq!(signature.field, "Signature1");
    assert!(signature.signer.contains("Test Signer"), "{signature:?}");
    assert!(signature.self_signed);
    assert!(signature.signed_at.is_some());
    assert_eq!(signature.reason, "Tests");
}

#[test]
fn tampering_invalidates_signature() {
    let mut bytes = signed_pdf();
    // Le premier octet du flux de page est couvert par la signature.
    let needle = b"Bonjour";
    let position = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("texte présent");
    bytes[position] = b'b';
    let signatures = validate_signatures(&bytes);
    assert_eq!(signatures.len(), 1);
    assert!(!signatures[0].valid, "{:?}", signatures[0]);
    assert!(signatures[0].error.is_some());
}

#[test]
fn second_signature_is_rejected_cleanly() {
    // Re-signer un document déjà signé réécrit la base : l'ancienne
    // signature doit être signalée invalide, jamais silencieusement acceptée.
    let first = signed_pdf();
    let document = Document::from_bytes(&first).expect("document signé");
    let second = document.sign(identity(), &options()).expect("seconde signature");
    let signatures = validate_signatures(&second);
    assert_eq!(signatures.len(), 2, "{signatures:?}");
    assert!(signatures.iter().any(|signature| signature.valid));
    assert!(signatures.iter().any(|signature| !signature.valid));
}

/// Une signature avec une image PNG doit valider comme une signature texte.
#[test]
fn signed_with_image_validates() {
    let png = signature_png();
    let mut opts = options();
    opts.signature_image = Some(png);
    let document = Document::from_bytes(&tiny_pdf()).expect("document");
    let signed = document.sign(identity(), &opts).expect("signature avec image");
    let signatures = validate_signatures(&signed);
    assert_eq!(signatures.len(), 1, "{signatures:?}");
    assert!(signatures[0].valid, "{signatures:?}");
    assert!(signatures[0].covers_whole_document);
}

/// Génère un petit PNG (1×1 pixel blanc) pour les tests.
fn signature_png() -> Vec<u8> {
    use std::io::Cursor;
    use image::{ImageBuffer, Rgba, ImageFormat};
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(2, 2, |_, _| Rgba([255u8, 255, 255, 255]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).expect("PNG");
    out.into_inner()
}

