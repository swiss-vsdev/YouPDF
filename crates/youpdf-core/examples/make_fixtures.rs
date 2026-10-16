//! Écrit un PDF de démonstration complet pour les essais d'interface :
//! plusieurs pages, un plan imbriqué, une pièce jointe et un champ de
//! formulaire texte.
//!
//! Usage : `cargo run -p youpdf-core --example make_fixtures [dossier]`

use std::path::PathBuf;

use lopdf::{dictionary, Document, Object, Stream};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&directory)?;

    let mut doc = Document::with_version("1.7");

    let font_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let resources = dictionary! { "Font" => dictionary! { "F1" => font_id } };

    let mut kids: Vec<Object> = Vec::new();
    let mut page_ids: Vec<lopdf::ObjectId> = Vec::new();
    for number in 1..=3 {
        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new("Tf", vec!["F1".into(), 20.into()]),
                lopdf::content::Operation::new("Td", vec![72.into(), 700.into()]),
                lopdf::content::Operation::new(
                    "Tj",
                    vec![Object::string_literal(format!("YouPDF fixture - page {number}"))],
                ),
                lopdf::content::Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => content_id,
            "Resources" => resources.clone(),
        });
        page_ids.push(page_id);
        kids.push(page_id.into());
    }

    // Champ de formulaire texte sur la première page.
    let field_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Widget".to_vec()),
        "FT" => Object::Name(b"Tx".to_vec()),
        "T" => Object::string_literal("nom"),
        "TU" => Object::string_literal("Votre nom"),
        "V" => Object::string_literal(""),
        "Rect" => Object::Array(vec![72.into(), 600.into(), 300.into(), 624.into()]),
        "P" => page_ids[0],
        "F" => 4,
        "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
    });
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_ids[0]) {
        page.set("Annots", Object::Array(vec![field_id.into()]));
    }

    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => 3,
    });

    // Plan : deux entrées, dont une imbriquée.
    let outline_root = doc.new_object_id();
    let child = doc.add_object(dictionary! {
        "Title" => Object::string_literal("Détails"),
        "Parent" => outline_root,
        "Dest" => Object::Array(vec![page_ids[1].into(), Object::Name(b"Fit".to_vec())]),
    });
    let parent = doc.add_object(dictionary! {
        "Title" => Object::string_literal("Introduction"),
        "Parent" => outline_root,
        "First" => child,
        "Last" => child,
        "Next" => Object::Null,
        "Dest" => Object::Array(vec![page_ids[0].into(), Object::Name(b"Fit".to_vec())]),
    });
    doc.set_object(
        outline_root,
        dictionary! {
            "Type" => Object::Name(b"Outlines".to_vec()),
            "First" => parent,
            "Last" => parent,
            "Count" => 2,
        },
    );

    // Pièce jointe.
    let file_stream = doc.add_object(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"EmbeddedFile".to_vec()),
            "Subtype" => Object::Name(b"text/plain".to_vec()),
        },
        b"piece jointe YouPDF\n".to_vec(),
    ));
    let filespec = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal("note.txt"),
        "UF" => Object::string_literal("note.txt"),
        "Desc" => Object::string_literal("Note de demonstration"),
        "EF" => dictionary! { "F" => file_stream, "UF" => file_stream },
    });
    let embedded = doc.add_object(dictionary! {
        "Names" => Object::Array(vec![Object::string_literal("note.txt"), filespec.into()]),
    });

    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
        "Outlines" => outline_root,
        "Names" => dictionary! { "EmbeddedFiles" => embedded },
        "AcroForm" => dictionary! {
            "Fields" => Object::Array(vec![field_id.into()]),
            "NeedAppearances" => true,
        },
    });
    doc.trailer.set("Root", catalog_id);

    let path = directory.join("fixture.pdf");
    doc.save(&path)?;
    println!("fixture écrite : {}", path.display());
    Ok(())
}
