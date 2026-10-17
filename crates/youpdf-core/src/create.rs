//! Création de documents : page vierge, PDF à partir d'images, fusion.
//!
//! Trois entrées :
//!
//! - [`blank_document`] construit un PDF en mémoire, sans passer par un
//!   fichier intermédiaire ;
//! - [`document_from_images`] fabrique une page par image, à la taille
//!   déduite du DPI demandé ;
//! - [`Document::insert_document`] recopie les pages d'un autre document
//!   dans celui-ci, en profondeur (objets partagés compris).

use std::path::PathBuf;

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};
use youpdf_shared::{Error, Result};

use crate::document::Document;

/// Crée un document de `pages` pages vierges.
///
/// `pages` doit valoir au moins 1 ; `width` et `height` sont en points PDF
/// et sont ramenés à 1.0 minimum (une `MediaBox` dégénérée fait échouer
/// certains lecteurs).
pub fn blank_document(pages: usize, width: f32, height: f32) -> Result<Document> {
    if pages == 0 {
        return Err(Error::InvalidInput(
            "un document doit contenir au moins une page".into(),
        ));
    }
    let width = width.max(1.0);
    let height = height.max(1.0);

    let mut doc = lopdf::Document::with_version("1.4");

    // Le nœud `/Pages` est créé avant ses enfants pour que chaque page
    // puisse porter sa référence `/Parent`.
    let pages_id = doc.new_object_id();

    let mut kids: Vec<Object> = Vec::with_capacity(pages);
    for _ in 0..pages {
        let content_id = doc.add_object(Stream::new(Dictionary::new(), Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                0.into(),
                0.into(),
                Object::Real(width),
                Object::Real(height),
            ]),
            "Contents" => Object::Reference(content_id),
        });
        kids.push(Object::Reference(page_id));
    }

    doc.set_object(
        pages_id,
        dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(pages as i64),
        },
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));

    Ok(Document::from_lopdf(doc))
}

/// Crée un document dont chaque page porte une image.
///
/// Une page par entrée de `paths`, dans l'ordre. La taille de page est
/// `pixels / dpi * 72` points (minimum 1.0). Un JPEG est embarqué tel quel
/// (`/DCTDecode`, pas de ré-encodage, donc pas de perte) ; les autres
/// formats sont décodés en RGB 8 bits puis compressés en zlib
/// (`/FlateDecode`).
pub fn document_from_images(paths: &[PathBuf], dpi: f32) -> Result<Document> {
    if paths.is_empty() {
        return Err(Error::InvalidInput("aucune image fournie".into()));
    }
    let dpi = dpi.max(36.0);

    let mut doc = lopdf::Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::with_capacity(paths.len());

    for path in paths {
        let bytes = std::fs::read(path)
            .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?;
        let format = image::guess_format(&bytes)
            .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?;
        let decoded = crate::compress::decode_limited(&bytes, format).ok_or_else(|| {
            Error::InvalidInput(format!(
                "{} : image illisible ou trop grande (plafond 20000 px, 256 Mio)",
                path.display()
            ))
        })?;

        let pixel_width = decoded.width();
        let pixel_height = decoded.height();
        let width = (pixel_width as f32 / dpi * 72.0).max(1.0);
        let height = (pixel_height as f32 / dpi * 72.0).max(1.0);

        // Un JPEG se pose sans ré-encodage : ses octets sont déjà un flux
        // `/DCTDecode`. Tout le reste passe par un décodage RGB 8 bits.
        let image_stream = if format == image::ImageFormat::Jpeg {
            Stream::new(
                dictionary! {
                    "Type" => Object::Name(b"XObject".to_vec()),
                    "Subtype" => Object::Name(b"Image".to_vec()),
                    "Width" => Object::Integer(pixel_width as i64),
                    "Height" => Object::Integer(pixel_height as i64),
                    "ColorSpace" => Object::Name(color_space(decoded.color()).to_vec()),
                    "BitsPerComponent" => Object::Integer(8),
                    "Filter" => Object::Name(b"DCTDecode".to_vec()),
                },
                bytes,
            )
        } else {
            let rgb = decoded.to_rgb8();
            let (rgb_width, rgb_height) = rgb.dimensions();
            Stream::new(
                dictionary! {
                    "Type" => Object::Name(b"XObject".to_vec()),
                    "Subtype" => Object::Name(b"Image".to_vec()),
                    "Width" => Object::Integer(rgb_width as i64),
                    "Height" => Object::Integer(rgb_height as i64),
                    "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                    "BitsPerComponent" => Object::Integer(8),
                    "Filter" => Object::Name(b"FlateDecode".to_vec()),
                },
                flate_compress(rgb.as_raw())?,
            )
        };
        let image_id = doc.add_object(image_stream);

        let content = format!("q {width:.2} 0 0 {height:.2} 0 0 cm /Im0 Do Q\n");
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));

        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                0.into(),
                0.into(),
                Object::Real(width),
                Object::Real(height),
            ]),
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Im0" => Object::Reference(image_id) },
            },
            "Contents" => Object::Reference(content_id),
        });
        kids.push(Object::Reference(page_id));
    }

    doc.set_object(
        pages_id,
        dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(paths.len() as i64),
        },
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));

    Ok(Document::from_lopdf(doc))
}

/// Espace colorimétrique PDF correspondant à un type de couleur décodé.
fn color_space(color: image::ColorType) -> &'static [u8] {
    match color {
        image::ColorType::L8
        | image::ColorType::L16
        | image::ColorType::La8
        | image::ColorType::La16 => b"DeviceGray",
        _ => b"DeviceRGB",
    }
}

/// Compresse en zlib (flux `/FlateDecode`).
fn flate_compress(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| Error::Internal(format!("compression image : {e}")))?;
    encoder
        .finish()
        .map_err(|e| Error::Internal(format!("compression image : {e}")))
}

impl Document {
    /// Insère les pages d'un autre document après la page `after`.
    ///
    /// `after == 0` place les pages en tête ; `after >= page_count` les
    /// ajoute à la fin. Les objets sont recopiés en profondeur : la source
    /// est d'abord sérialisée en mémoire, puis rechargée et renumérotée,
    /// ce qui rend l'opération sûre même si la source est une copie de
    /// `self` (les octets sont figés avant toute mutation).
    ///
    /// Retourne le nombre de pages insérées.
    pub fn insert_document(&mut self, after: usize, other: &Document) -> Result<usize> {
        // Sérialiser la source AVANT de toucher à `self` : si elle partage
        // des octets avec `self`, le résultat reste celui d'avant mutation.
        let bytes = other.to_bytes()?;
        let mut foreign = lopdf::Document::load_mem(&bytes)
            .map_err(|e| Error::Pdf(format!("document source : {e}")))?;

        let page_count = foreign.get_pages().len();
        if page_count == 0 {
            return Ok(0);
        }

        // Renumérote tous les objets de la source au-dessus de l'espace
        // d'identifiants de `self`, et réécrit les références en interne.
        let start = self.inner.max_id + 1;
        foreign.renumber_objects_with(start);
        let new_max_id = foreign.max_id;
        let new_pages: Vec<ObjectId> = foreign.get_pages().values().copied().collect();

        let existing = self.inner.get_pages().len();
        let root = self.pages_root_id()?;

        // Les identifiants sont disjoints : fusion directe sans collision.
        self.inner.objects.extend(foreign.objects);
        self.inner.max_id = new_max_id;

        let anchor = if after == 0 {
            None
        } else if after >= existing {
            self.inner.get_pages().get(&(existing as u32)).copied()
        } else {
            self.inner.get_pages().get(&(after as u32)).copied()
        };

        insert_kids_after(&mut self.inner, root, anchor, &new_pages);
        refresh_page_counts(&mut self.inner, root);

        self.touch();
        Ok(new_pages.len())
    }
}

/// Insère des références de pages dans le `/Kids` qui contient l'ancre.
///
/// `anchor == None` place les pages en tête du nœud racine. Si l'ancre est
/// introuvable, les pages finissent à la fin du nœud racine.
fn insert_kids_after(
    doc: &mut lopdf::Document,
    node_id: ObjectId,
    anchor: Option<ObjectId>,
    new_pages: &[ObjectId],
) -> usize {
    let children: Vec<Object> = doc
        .get_dictionary(node_id)
        .ok()
        .and_then(|node| node.get(b"Kids").and_then(Object::as_array).ok().cloned())
        .unwrap_or_default();

    if anchor.is_none() {
        let mut kids: Vec<Object> = new_pages.iter().map(|id| Object::Reference(*id)).collect();
        kids.extend(children);
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(node_id) {
            dict.set("Kids", Object::Array(kids));
        }
        set_pages_parent(doc, node_id, new_pages);
        return new_pages.len();
    }

    let anchor_id = anchor.expect("ancre vérifiée non nulle");
    let position = children.iter().position(|child| match child {
        Object::Reference(id) => *id == anchor_id,
        _ => false,
    });
    if let Some(position) = position {
        let mut kids = children;
        let tail = kids.split_off(position + 1);
        kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
        kids.extend(tail);
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(node_id) {
            dict.set("Kids", Object::Array(kids));
        }
        set_pages_parent(doc, node_id, new_pages);
        return new_pages.len();
    }

    // L'ancre vit plus bas : descendre dans les sous-arbres.
    for child in &children {
        let Object::Reference(child_id) = child else {
            continue;
        };
        if !is_pages_node(doc, *child_id) {
            continue;
        }
        if insert_kids_after(doc, *child_id, anchor, new_pages) > 0 {
            return new_pages.len();
        }
    }

    // Ancre absente de l'arbre : ajouter en fin de nœud racine.
    let mut kids = children;
    kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(node_id) {
        dict.set("Kids", Object::Array(kids));
    }
    set_pages_parent(doc, node_id, new_pages);
    new_pages.len()
}

/// Rattache les pages insérées au nœud qui les reçoit.
fn set_pages_parent(doc: &mut lopdf::Document, node_id: ObjectId, pages: &[ObjectId]) {
    for id in pages {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(*id) {
            dict.set("Parent", Object::Reference(node_id));
        }
    }
}

/// Vrai si l'objet est un nœud `/Pages`.
fn is_pages_node(doc: &lopdf::Document, id: ObjectId) -> bool {
    doc.get_dictionary(id)
        .map(|dict| {
            dict.get(b"Type")
                .and_then(Object::as_name)
                .map(|name| name == b"Pages")
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Recalcule `/Count` sur tout l'arbre des pages.
fn refresh_page_counts(doc: &mut lopdf::Document, node_id: ObjectId) -> usize {
    let children: Vec<Object> = doc
        .get_dictionary(node_id)
        .ok()
        .and_then(|node| node.get(b"Kids").and_then(Object::as_array).ok().cloned())
        .unwrap_or_default();

    let mut count = 0usize;
    for child in &children {
        let Object::Reference(child_id) = child else {
            count += 1;
            continue;
        };
        if is_pages_node(doc, *child_id) {
            count += refresh_page_counts(doc, *child_id);
        } else {
            count += 1;
        }
    }

    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(node_id) {
        dict.set("Count", Object::Integer(count as i64));
    }
    count
}
