//! Extraction de texte positionné et d'images embarquées.

use lopdf::Object;

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::{ImageInfo, TextFragment};
use crate::edit::text_helpers::{
    current_text_len, extract_string_literal, push_fragment, tokenize_content,
};

impl Document {
    /// Extrait tous les fragments de texte positionnés d'une page.
    ///
    /// ## Limites
    ///
    /// L'analyse repose sur `lopdf` pour reconstruire les positions à
    /// partir des opérateurs `Td`, `TD`, `Tm`, `T*`. Les polices avec
    /// encodage non standard (CID, Type0 complexe) peuvent produire
    /// des positions imprécises. Les fragments retournés sont donc
    /// **indicatifs** : utiles pour localiser un mot à l'écran, moins
    /// fiables pour un repositionnement au point près.
    pub fn extract_text_fragments(&self, page: u32) -> Result<Vec<TextFragment>> {
        let page_id = self
            .page_id(page)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
        let page_dict = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;

        // Résolution de la police par défaut (peut être None).
        let default_font = page_dict
            .get(b"Resources")
            .ok()
            .and_then(|r| match r {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Reference(id) => self.inner().get_dictionary(*id).ok().cloned(),
                _ => None,
            })
            .and_then(|res| res.get(b"Font").ok().cloned())
            .and_then(|f| match f {
                Object::Dictionary(d) => Some(d),
                Object::Reference(id) => self.inner().get_dictionary(id).ok().cloned(),
                _ => None,
            });

        // On parcourt le flux de contenu, à la recherche des blocs BT…ET
        // et des opérateurs de positionnement.
        let content = self
            .inner()
            .get_page_content(page_id)
            .map_err(|e| Error::Pdf(format!("contenu page {page} : {e}")))?;

        let mut fragments = Vec::new();
        let mut current: Option<TextFragment> = None;
        let mut tx: f32 = 0.0;
        let mut ty: f32 = 0.0;
        let mut font_size: f32 = 11.0;
        let mut font_name: Option<String> = None;

        // Analyse naïve du flux texte. On n'essaie pas d'être exhaustif :
        // on couvre les cas usuels (Tj, TJ, Td, TD, Tm, Tf, T*).
        let tokens = tokenize_content(&content);

        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].as_str() {
                "BT" => {
                    tx = 0.0;
                    ty = 0.0;
                }
                "Tf" => {
                    // Séquence : /Nom taille Tf
                    if i >= 2 {
                        if let Some(name) = tokens.get(i - 2) {
                            if name.starts_with('/') {
                                font_name = Some(name.trim_start_matches('/').to_string());
                            }
                        }
                        if let Some(size) = tokens.get(i - 1).and_then(|s| s.parse::<f32>().ok()) {
                            font_size = size;
                        }
                    }
                }
                "Td" | "TD" => {
                    // dx dy Td
                    if i >= 2 {
                        if let (Some(dx), Some(dy)) = (
                            tokens.get(i - 2).and_then(|s| s.parse::<f32>().ok()),
                            tokens.get(i - 1).and_then(|s| s.parse::<f32>().ok()),
                        ) {
                            tx += dx;
                            ty += dy;
                        }
                    }
                }
                "Tm" => {
                    // a b c d e f Tm : on ne garde que e, f
                    if i >= 6 {
                        if let (Some(e), Some(f)) = (
                            tokens.get(i - 2).and_then(|s| s.parse::<f32>().ok()),
                            tokens.get(i - 1).and_then(|s| s.parse::<f32>().ok()),
                        ) {
                            tx = e;
                            ty = f;
                        }
                    }
                }
                "T*" => {
                    // Retour à la ligne : on avance de la taille de police.
                    ty -= font_size * 1.2;
                }
                "Tj" | "'" | "\"" => {
                    // Le texte est le token précédent.
                    if i >= 1 {
                        if let Some(text) = extract_string_literal(&tokens[i - 1]) {
                            push_fragment(
                                &mut fragments,
                                &mut current,
                                text,
                                page as usize,
                                tx,
                                ty,
                                font_size,
                                font_name.clone(),
                            );
                            // Avance horizontale approximative.
                            tx += font_size * 0.5 * current_text_len(&fragments) as f32;
                        }
                    }
                }
                "ET" => {
                    if let Some(f) = current.take() {
                        fragments.push(f);
                    }
                }
                "TJ" => {
                    // Tableau `[ (mot) -250 (mot) ] TJ` : les espacements
                    // numériques ne sont pas du texte, seulement les chaînes.
                    // On remonte jusqu'au `[` ouvrant pour les rassembler ;
                    // sans `[` trouvé, on ne prend rien : remonter plus loin
                    // ramasserait les chaînes d'un bloc précédent.
                    let mut parts: Vec<String> = Vec::new();
                    let mut opened = false;
                    let mut j = i;
                    while j > 0 {
                        j -= 1;
                        if tokens[j] == "[" {
                            opened = true;
                            break;
                        }
                        if let Some(text) = extract_string_literal(&tokens[j]) {
                            parts.push(text);
                        }
                    }
                    if opened && !parts.is_empty() {
                        parts.reverse();
                        push_fragment(
                            &mut fragments,
                            &mut current,
                            parts.join(""),
                            page as usize,
                            tx,
                            ty,
                            font_size,
                            font_name.clone(),
                        );
                        tx += font_size * 0.5 * current_text_len(&fragments) as f32;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        // Polices non résolues : on ignore.
        let _ = default_font;

        Ok(fragments)
    }

    /// Liste les images embarquées dans une page.
    pub fn extract_images(&self, page: u32) -> Result<Vec<ImageInfo>> {
        let page_id = self
            .page_id(page)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
        let page_dict = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;

        let resources = match page_dict.get(b"Resources") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("Ressources : {e}")))?
                .clone(),
            _ => return Ok(Vec::new()),
        };

        let xobjects = match resources.get(b"XObject") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("XObject : {e}")))?
                .clone(),
            _ => return Ok(Vec::new()),
        };

        let mut out = Vec::new();
        for (name, value) in xobjects.iter() {
            let Object::Reference(obj_id) = value else {
                continue;
            };
            let Ok(obj) = self.inner().get_object(*obj_id) else {
                continue;
            };
            let Ok(stream) = obj.as_stream() else {
                continue;
            };
            let dict = &stream.dict;

            // Sous-type /Image uniquement.
            let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
            if subtype != Some(b"Image") {
                continue;
            }

            // Une dimension absente, nulle ou négative est rejetée : `as u32`
            // transformerait −1 en 4 milliards, et une allocation suivrait.
            let dimension = |key: &[u8]| -> Option<u32> {
                match dict.get(key).ok().and_then(|o| o.as_i64().ok()) {
                    Some(value) if (1..=100_000).contains(&value) => Some(value as u32),
                    _ => None,
                }
            };
            let (Some(width), Some(height)) = (dimension(b"Width"), dimension(b"Height")) else {
                continue;
            };

            let components = match dict.get(b"ColorSpace").ok().and_then(|o| o.as_name().ok()) {
                Some(b"DeviceGray") => 1,
                Some(b"DeviceRGB") => 3,
                Some(b"DeviceCMYK") => 4,
                _ => 3, // inconnu, on suppose RGB
            };

            out.push(ImageInfo {
                page: page as usize,
                resource_name: String::from_utf8_lossy(name).to_string(),
                width,
                height,
                components,
                object_id: *obj_id,
            });
        }

        Ok(out)
    }

    /// Extrait les octets bruts d'une image embarquée.
    pub fn image_bytes(&self, info: &ImageInfo) -> Result<Vec<u8>> {
        let obj = self
            .inner()
            .get_object(info.object_id)
            .map_err(|e| Error::Pdf(format!("objet image : {e}")))?;
        let stream = obj
            .as_stream()
            .map_err(|e| Error::Pdf(format!("flux image : {e}")))?;
        Ok(stream.content.clone())
    }

    /// Extrait une image embarquée et la renvoie encodée en PNG.
    ///
    /// Le contenu du flux n'est pas toujours un fichier image : `DCTDecode`
    /// est du JPEG, mais `FlateDecode` cache des échantillons bruts. On passe
    /// donc par le décodeur commun, puis on ré-encode en PNG.
    pub fn extract_image_png(&self, info: &ImageInfo) -> Result<Vec<u8>> {
        let obj = self
            .inner()
            .get_object(info.object_id)
            .map_err(|e| Error::Pdf(format!("objet image : {e}")))?;
        let stream = obj
            .as_stream()
            .map_err(|e| Error::Pdf(format!("flux image : {e}")))?;
        let image = crate::compress::decode_stream_content(stream).ok_or_else(|| {
            Error::Pdf(format!(
                "image {} ({}) non décodable : format non pris en charge",
                info.resource_name, info.object_id.0
            ))
        })?;
        let mut out = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| Error::Pdf(format!("encodage PNG : {e}")))?;
        Ok(out)
    }
}
