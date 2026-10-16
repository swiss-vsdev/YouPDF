//! Vidage structurel d'un PDF : pages, annotations, apparences.
//!
//! usage: cargo run -p youpdf-core --example pdfdump -- <fichier.pdf>

use youpdf_core::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("chemin du PDF");
    let bytes = std::fs::read(&path)?;
    let doc = lopdf::Document::load_mem(&bytes)?;
    let annots = Document::from_bytes(&bytes)?.read_annotations();
    println!("annotations relues par YouPDF : {}", annots.len());
    for (num, page_id) in doc.get_pages() {
        let page = doc.get_dictionary(page_id)?;
        let count = match page.get(b"Annots") {
            Ok(lopdf::Object::Array(arr)) => arr.len(),
            Ok(lopdf::Object::Reference(id)) => doc.get_object(*id)?.as_array()?.len(),
            _ => 0,
        };
        let contents = match page.get(b"Contents") {
            Ok(lopdf::Object::Array(arr)) => format!("{} flux", arr.len()),
            Ok(_) => "1 flux".to_string(),
            Err(_) => "aucun".to_string(),
        };
        let xobjects = page
            .get(b"Resources")
            .ok()
            .and_then(|r| r.as_dict().ok())
            .and_then(|r| r.get(b"XObject").ok())
            .and_then(|x| x.as_dict().ok())
            .map(|x| x.len())
            .unwrap_or(0);
        println!("page {num} : {count} annotation(s), contenu {contents}, {xobjects} XObject");
        if let Ok(lopdf::Object::Array(arr)) = page.get(b"Annots") {
            for item in arr {
                if let Ok(id) = item.as_reference() {
                    let dict = doc.get_dictionary(id)?;
                    let ap = dict.get(b"AP").is_ok();
                    let subtype = dict
                        .get(b"Subtype")
                        .and_then(|o| o.as_name_str())
                        .unwrap_or("?");
                    println!("   {id:?} {subtype} /AP={ap}");
                }
            }
        }
    }
    Ok(())
}
