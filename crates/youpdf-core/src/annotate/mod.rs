//! Annotations PDF natives.
//!
//! ## Approche retenue
//!
//! YouPDF crée de **véritables annotations PDF** (entrées dans le tableau
//! `/Annots` de chaque page), plutôt que de dessiner sur le contenu de la
//! page. C'est l'approche standard des éditeurs PDF professionnels :
//!
//! - les annotations restent éditables dans n'importe quel lecteur ;
//! - elles peuvent être listées, filtrées, exportées séparément ;
//! - le contenu original de la page n'est pas modifié.
//!
//! ## Types supportés
//!
//! | Type          | Subtype PDF  |
//! |---------------|--------------|
//! | Surlignage    | `/Highlight` |
//! | Soulignage    | `/Underline` |
//! | Barré         | `/StrikeOut` |
//! | Note          | `/Text`      |
//! | Texte libre   | `/FreeText`  |
//! | Rectangle     | `/Square`    |
//! | Cercle        | `/Circle`    |
//! | Crayon        | `/Ink`       |
//!
//! ## Marquage
//!
//! Toutes les annotations créées par YouPDF portent un `/NM` (nom) qui
//! commence par `youpdf-`. Cela permet de les identifier et de les
//! remplacer sans toucher aux annotations créées par d'autres outils.


mod render;
mod types;

pub use render::{
    dynamic_stamp_text, quad_from_rect, smooth_polyline, stamp_id_for_pdf_name, stamp_name,
    stamp_template,
};
pub use types::*;

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use crate::document::Document;
pub(crate) use render::push_text_literal;
use render::{Paintable, form_bbox, pdf_date};
use youpdf_shared::{Error, Result};

// ────────────────────────────────────────────────────────────────────────
// Intégration dans un Document
// ────────────────────────────────────────────────────────────────────────

/// Préfixe d'identification des annotations créées par YouPDF.
const NM_PREFIX: &str = "youpdf-";

impl Document {
    /// Lit les annotations YouPDF présentes dans le document.
    ///
    /// Utile pour recharger un document précédemment annoté.
    pub fn read_annotations(&self) -> Vec<Annotation> {
        let mut out = Vec::new();
        let pages = self.inner().get_pages();

        for (page_num, page_id) in pages {
            let Ok(page) = self.inner().get_dictionary(page_id) else {
                continue;
            };
            let Ok(annots_ref) = page.get(b"Annots") else {
                continue;
            };
            let Ok(annots) = annots_ref.as_array() else {
                continue;
            };

            for annot_ref in annots {
                let Ok(annot_id) = annot_ref.as_reference() else {
                    continue;
                };
                let Ok(annot_obj) = self.inner().get_object(annot_id) else {
                    continue;
                };
                let Ok(annot_dict) = annot_obj.as_dict() else {
                    continue;
                };

                // Ignore les annotations qui ne sont pas de nous.
                // lopdf 0.34 : `as_str()` rend des octets.
                let nm = annot_dict
                    .get(b"NM")
                    .ok()
                    .and_then(|o| o.as_str().ok())
                    .map(|b| String::from_utf8_lossy(b).to_string())
                    .unwrap_or_default();
                if !nm.starts_with(NM_PREFIX) {
                    continue;
                }

                // Reconstitue l'annotation.
                let Some(mut ann) = parse_annotation(annot_dict, &nm, page_num as usize) else {
                    continue;
                };
                // `/IRT` vise l'objet du parent ; on remonte à son `/NM`.
                ann.reply_to = annot_dict
                    .get(b"IRT")
                    .ok()
                    .and_then(|o| o.as_reference().ok())
                    .and_then(|parent_id| self.inner().get_dictionary(parent_id).ok())
                    .and_then(|parent| parent.get(b"NM").ok())
                    .and_then(|o| o.as_str().ok())
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string());
                out.push(ann);
            }
        }

        out
    }

    /// Enregistre le document en y ajoutant `annots`.
    ///
    /// Les annotations en cours d'édition vivent hors du document : c'est au
    /// moment d'écrire le fichier qu'elles y prennent place, apparence
    /// comprise. L'état en mémoire est rendu tel qu'il était — sans quoi le
    /// moteur de rendu les peindrait lui-même, en plus du dessin de
    /// l'interface, et chaque marque apparaîtrait deux fois.
    pub fn save_with_annotations(
        &mut self,
        path: impl AsRef<std::path::Path>,
        annots: &[Annotation],
    ) -> Result<()> {
        if !annots.is_empty() {
            self.apply_annotations(annots)?;
        }
        let saved = self.save(path);
        // Même si l'écriture a échoué, le document ne doit pas rester dans
        // l'état « annotations posées » : l'interface dessine à partir de la
        // liste de l'onglet, pas du document.
        let cleared = self.clear_youpdf_annotations();
        saved.and(cleared)
    }

    /// Applique un ensemble d'annotations au document.
    ///
    /// **Idempotent** : les annotations préfixées `youpdf-` déjà présentes
    /// sont supprimées, puis remplacées par celles fournies. Les autres
    /// annotations ne sont pas touchées.
    pub fn apply_annotations(&mut self, annotations: &[Annotation]) -> Result<()> {
        // 1. Retirer les anciennes annotations YouPDF.
        self.clear_youpdf_annotations()?;

        // 2. Grouper par page.
        let mut by_page: std::collections::HashMap<usize, Vec<&Annotation>> =
            std::collections::HashMap::new();
        for ann in annotations {
            by_page.entry(ann.page).or_default().push(ann);
        }

        // 3. Ajouter les nouvelles.
        // Chaque annotation créée garde de quoi résoudre `/IRT` : (objet,
        // identifiant, identifiant du parent visé).
        let mut created: Vec<(ObjectId, String, Option<String>)> = Vec::new();
        for (page_num, anns) in by_page {
            let page_id = self
                .page_id(page_num as u32)
                .map_err(|e| Error::Pdf(format!("page {page_num} : {e}")))?;

            // Créer les objets annotation.
            let mut annot_refs: Vec<Object> = Vec::new();
            // Police des apparences : créée une fois pour tout l'appel.
            let mut appearance_font: Option<Object> = None;
            for ann in anns {
                let mut dict = ann.to_pdf_dict();

                // Pièce jointe : embarquer le fichier dans le PDF.
                if ann.kind == AnnotationKind::FileAttachment {
                    if let Some(fs) = self.build_embedded_file(&ann.content)? {
                        dict.set("FS", fs);
                    }
                }

                // Apparence : sans `/AP`, les autres lecteurs n'affichent
                // rien. YouPDF dessine les siennes à l'écran, mais un PDF
                // partagé doit se voir partout.
                if let Some(content) = ann.appearance_content() {
                    let (_, _, w, h) = ann.frame();
                    let font = match &appearance_font {
                        Some(font) => font.clone(),
                        None => {
                            let font = Object::Reference(self.ensure_appearance_font()?);
                            appearance_font = Some(font.clone());
                            font
                        }
                    };
                    let mut xobject = Dictionary::new();
                    xobject.set("Type", Object::Name(b"XObject".to_vec()));
                    xobject.set("Subtype", Object::Name(b"Form".to_vec()));
                    xobject.set("FormType", Object::Integer(1));
                    xobject.set(
                        "BBox",
                        Object::Array(vec![
                            Object::Real(0.0),
                            Object::Real(0.0),
                            Object::Real(w),
                            Object::Real(h),
                        ]),
                    );
                    xobject.set("Resources", Object::Dictionary(ann.appearance_resources(font)));
                    let stream_id = self.inner_mut().add_object(Stream::new(xobject, content));
                    let mut ap = Dictionary::new();
                    ap.set("N", Object::Reference(stream_id));
                    dict.set("AP", Object::Dictionary(ap));
                }

                let id = self.inner_mut().add_object(Object::Dictionary(dict));
                annot_refs.push(Object::Reference(id));
                created.push((id, ann.id.clone(), ann.reply_to.clone()));
            }

            // Récupérer / créer le tableau /Annots de la page.
            let page = self
                .inner_mut()
                .get_object_mut(page_id)
                .map_err(|e| Error::Pdf(format!("page {page_num} : {e}")))?;
            let dict = page
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("page {page_num} : {e}")))?;

            match dict.get_mut(b"Annots") {
                Ok(Object::Array(existing)) => {
                    existing.extend(annot_refs);
                }
                Ok(Object::Reference(existing_ref)) => {
                    // Cas rare : /Annots est une référence indirecte.
                    let existing_id = *existing_ref;
                    let existing_obj = self
                        .inner_mut()
                        .get_object_mut(existing_id)
                        .map_err(|e| Error::Pdf(format!("Annots indirects : {e}")))?;
                    if let Object::Array(arr) = existing_obj {
                        arr.extend(annot_refs);
                    }
                }
                _ => {
                    dict.set("Annots", Object::Array(annot_refs));
                }
            }
        }

        // 4. Réponses : `/IRT` est une référence indirecte vers l'objet du
        //    parent. Elle se pose au second passage, une fois tous les objets
        //    créés — un parent peut se trouver sur une autre page.
        if created.iter().any(|(_, _, reply)| reply.is_some()) {
            let by_name: std::collections::HashMap<&str, ObjectId> = created
                .iter()
                .map(|(id, name, _)| (name.as_str(), *id))
                .collect();
            for (id, _, reply) in &created {
                let Some(parent_name) = reply else {
                    continue;
                };
                let Some(&parent_id) = by_name.get(parent_name.as_str()) else {
                    // Parent absent : la réponse reste sans lien plutôt que
                    // de pointer un objet inexistant.
                    continue;
                };
                if let Ok(dict) = self.inner_mut().get_dictionary_mut(*id) {
                    dict.set("IRT", Object::Reference(parent_id));
                    // `/RT R` : réponse proprement dite (et non un simple
                    // commentaire « en réponse à »).
                    dict.set("RT", Object::Name(b"R".to_vec()));
                }
            }
        }

        Ok(())
    }

    /// Construit une /Filespec embarquant le fichier `path`, et renvoie
    /// sa référence (`/FS`). `None` si le fichier est illisible.
    fn build_embedded_file(&mut self, path: &str) -> Result<Option<Object>> {
        use std::path::Path;
        let path = Path::new(path);
        let bytes = std::fs::read(path).map_err(Error::Io)?;
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("piece_jointe");

        let stream = Stream::new(
            dictionary! {
                "Type" => Object::Name(b"EmbeddedFile".to_vec()),
                "Subtype" => Object::Name(b"application/octet-stream".to_vec()),
            },
            bytes,
        );
        let stream_id = self.inner_mut().add_object(stream);

        let mut ef = Dictionary::new();
        ef.set("F", Object::Reference(stream_id));
        let mut filespec = dictionary! {
            "Type" => Object::Name(b"Filespec".to_vec()),
            "F" => Object::string_literal(file_name),
            "UF" => Object::string_literal(file_name),
        };
        filespec.set("EF", Object::Dictionary(ef));

        let fs_id = self.inner_mut().add_object(Object::Dictionary(filespec));
        Ok(Some(Object::Reference(fs_id)))
    }

    /// Supprime du document toutes les annotations préfixées `youpdf-`.
    ///
    /// Sert à sortir les annotations du document pour les éditer : elles
    /// reviennent dans le fichier écrit par [`Self::save_with_annotations`].
    /// Les annotations des autres producteurs ne sont pas touchées.
    pub fn clear_youpdf_annotations(&mut self) -> Result<()> {
        let pages = self.inner().get_pages();
        // (page_id, annotations à conserver) — liste déjà filtrée au
        // premier passage, réécrite telle quelle au second.
        let mut updates: Vec<(ObjectId, Vec<Object>)> = Vec::new();

        for (_page_num, page_id) in pages {
            let Ok(page) = self.inner().get_dictionary(page_id) else {
                continue;
            };
            let Ok(annots_ref) = page.get(b"Annots") else {
                continue;
            };
            let Ok(annots) = annots_ref.as_array() else {
                continue;
            };

            // Filtrer : garder tout ce qui n'est pas youpdf-.
            let mut kept = Vec::with_capacity(annots.len());
            for annot_ref in annots {
                let is_ours = match annot_ref.as_reference() {
                    Ok(id) => self
                        .inner()
                        .get_object(id)
                        .ok()
                        .and_then(|o| o.as_dict().ok())
                        .and_then(|d| d.get(b"NM").ok())
                        .and_then(|nm| nm.as_str().ok())
                        .map(|b| String::from_utf8_lossy(b).starts_with(NM_PREFIX))
                        .unwrap_or(false),
                    Err(_) => false,
                };
                if !is_ours {
                    kept.push(annot_ref.clone());
                }
            }

            if kept.len() != annots.len() {
                updates.push((page_id, kept));
            }
        }

        for (page_id, kept) in updates {
            let page = self
                .inner_mut()
                .get_object_mut(page_id)
                .map_err(|e| Error::Pdf(format!("mise à jour page : {e}")))?;
            if let Object::Dictionary(dict) = page {
                dict.set("Annots", Object::Array(kept));
            }
        }

        Ok(())
    }

    /// Aplatit les annotations dans le contenu des pages.
    ///
    /// Chaque annotation dont l'apparence (`/AP /N`) est un flux de
    /// formulaire est peinte dans la page par-dessus le contenu existant,
    /// puis retirée de `/Annots`. Les annotations sans apparence exploitable
    /// — liens, et ce que d'autres outils laissent sans `/AP` — restent en
    /// place : les aplatir effacerait un dessin que seul le lecteur sait
    /// produire.
    ///
    /// Renvoie le nombre d'annotations réellement aplaties.
    pub fn flatten_annotations(&mut self) -> Result<usize> {
        let pages = self.inner().get_pages();
        let mut flattened = 0usize;

        for (_page_num, page_id) in pages {
            let annots = self.page_annots(page_id);
            if annots.is_empty() {
                continue;
            }

            let mut kept: Vec<Object> = Vec::new();
            let mut painted: Vec<Paintable> = Vec::new();
            for annot_ref in annots {
                match self.paintable_for(&annot_ref, painted.len()) {
                    Some(appearance) => painted.push(appearance),
                    None => kept.push(annot_ref),
                }
            }
            if painted.is_empty() {
                continue;
            }

            // Les apparences sont ajoutées à la fin du contenu : dans un
            // lecteur, une annotation se dessine par-dessus la page.
            let mut ops: Vec<u8> = Vec::new();
            let mut xobjects = Dictionary::new();
            for appearance in &painted {
                let [a, b, c, d, e, f] = appearance.matrix;
                ops.extend_from_slice(
                    format!(
                        "q {a:.4} {b:.4} {c:.4} {d:.4} {e:.2} {f:.2} cm /{} Do Q\n",
                        appearance.name
                    )
                    .as_bytes(),
                );
                xobjects.set(appearance.name.as_str(), appearance.stream.clone());
            }

            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(Dictionary::new(), ops));
            self.add_page_xobjects(page_id, &xobjects)?;
            self.append_page_content(page_id, stream_id)?;
            self.set_page_annots(page_id, &kept)?;
            flattened += painted.len();
        }

        if flattened > 0 {
            tracing::info!(flattened, "annotations aplaties dans les pages");
        }
        Ok(flattened)
    }

    /// Annotations d'une page, tableau déjà déplié.
    pub(crate) fn page_annots(&self, page_id: ObjectId) -> Vec<Object> {
        let Ok(page) = self.inner().get_dictionary(page_id) else {
            return Vec::new();
        };
        match page.get(b"Annots") {
            Ok(Object::Array(arr)) => arr.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_object(*id)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// Réécrit `/Annots` : la clé disparaît quand il ne reste rien.
    pub(crate) fn set_page_annots(&mut self, page_id: ObjectId, annots: &[Object]) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        if annots.is_empty() {
            dict.remove(b"Annots");
        } else {
            dict.set("Annots", Object::Array(annots.to_vec()));
        }
        Ok(())
    }

    /// Déplie l'apparence d'une annotation, si elle est peignable.
    fn paintable_for(&mut self, annot_ref: &Object, index: usize) -> Option<Paintable> {
        let dict = match annot_ref {
            Object::Reference(id) => self.inner().get_dictionary(*id).ok()?.clone(),
            Object::Dictionary(d) => d.clone(),
            _ => return None,
        };

        // Sans `/Rect` exploitable, il n'y a pas de place où peindre.
        let rect = dict.get(b"Rect").ok()?.as_array().ok()?;
        if rect.len() != 4 {
            return None;
        }
        let mut r = [0.0f32; 4];
        for (i, obj) in rect.iter().enumerate() {
            r[i] = obj.as_float().ok()?;
        }
        if (r[2] - r[0]).abs() < 0.5 || (r[3] - r[1]).abs() < 0.5 {
            return None;
        }

        // `/AP /N` doit être un flux de formulaire. Un dictionnaire d'états
        // (cases à cocher, boutons radio) demande de choisir l'état courant :
        // ces annotations restent en place.
        let ap = match dict.get(b"AP").ok()? {
            Object::Reference(id) => self.inner().get_dictionary(*id).ok()?.clone(),
            Object::Dictionary(d) => d.clone(),
            _ => return None,
        };
        let (stream, bbox) = match ap.get(b"N").ok()?.clone() {
            Object::Reference(id) => {
                let stream = self.inner().get_object(id).ok()?.as_stream().ok()?.clone();
                (Object::Reference(id), form_bbox(&stream)?)
            }
            Object::Stream(stream) => {
                let bbox = form_bbox(&stream)?;
                let id = self.inner_mut().add_object(stream);
                (Object::Reference(id), bbox)
            }
            _ => return None,
        };

        let bw = (bbox[2] - bbox[0]).max(0.5);
        let bh = (bbox[3] - bbox[1]).max(0.5);
        let sx = (r[2] - r[0]).abs() / bw;
        let sy = (r[3] - r[1]).abs() / bh;
        let tx = r[0].min(r[2]) - bbox[0].min(bbox[2]) * sx;
        let ty = r[1].min(r[3]) - bbox[1].min(bbox[3]) * sy;
        Some(Paintable {
            name: format!("YouPdfFlat{index}"),
            stream,
            matrix: [sx, 0.0, 0.0, sy, tx, ty],
        })
    }

    /// Ajoute des XObjects aux ressources d'une page.
    fn add_page_xobjects(&mut self, page_id: ObjectId, xobjects: &Dictionary) -> Result<()> {
        let mut resources = self.read_page_resources(page_id)?;
        let mut existing = resources
            .get(b"XObject")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        for (name, obj) in xobjects.iter() {
            existing.set(name.to_vec(), obj.clone());
        }
        resources.set("XObject", Object::Dictionary(existing));
        self.write_page_resources(page_id, resources)
    }

    /// Ressources d'une page, résolues en dictionnaire direct.
    fn read_page_resources(&self, page_id: ObjectId) -> Result<Dictionary> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        match page.get(b"Resources") {
            Ok(Object::Dictionary(d)) => Ok(d.clone()),
            Ok(Object::Reference(id)) => Ok(self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("ressources : {e}")))?
                .clone()),
            _ => Ok(Dictionary::new()),
        }
    }

    /// Écrit les ressources d'une page.
    fn write_page_resources(&mut self, page_id: ObjectId, resources: Dictionary) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", Object::Dictionary(resources));
        Ok(())
    }

    /// Concatène un flux à la fin du contenu d'une page.
    fn append_page_content(&mut self, page_id: ObjectId, stream_id: ObjectId) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        match dict.get_mut(b"Contents") {
            Ok(Object::Reference(id)) => {
                let existing = *id;
                dict.set(
                    "Contents",
                    Object::Array(vec![
                        Object::Reference(existing),
                        Object::Reference(stream_id),
                    ]),
                );
            }
            Ok(Object::Array(arr)) => arr.push(Object::Reference(stream_id)),
            _ => dict.set("Contents", Object::Reference(stream_id)),
        }
        Ok(())
    }

    /// Crée la police Helvetica des apparences.
    pub(crate) fn ensure_appearance_font(&mut self) -> Result<ObjectId> {
        let font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        };
        Ok(self.inner_mut().add_object(Object::Dictionary(font)))
    }
}

/// Reconstruit une `Annotation` à partir d'un dictionnaire PDF.
fn parse_annotation(dict: &Dictionary, id: &str, page: usize) -> Option<Annotation> {
    let subtype = dict.get(b"Subtype").ok()?.as_name().ok()?;
    let kind = match subtype {
        b"Highlight" => AnnotationKind::Highlight,
        b"Underline" => AnnotationKind::Underline,
        b"StrikeOut" => AnnotationKind::StrikeOut,
        b"Squiggly" => AnnotationKind::Squiggly,
        b"Text" => AnnotationKind::Note,
        b"FreeText" => AnnotationKind::FreeText,
        b"Square" => AnnotationKind::Square,
        b"Circle" => AnnotationKind::Circle,
        b"Line" => {
            // Une flèche a un /LE contenant OpenArrow.
            if dict.get(b"LE").ok().and_then(|o| o.as_array().ok()).is_some() {
                AnnotationKind::Arrow
            } else {
                AnnotationKind::Line
            }
        }
        b"Ink" => AnnotationKind::Ink,
        b"Stamp" => AnnotationKind::Stamp,
        b"Link" => AnnotationKind::Link,
        b"FileAttachment" => AnnotationKind::FileAttachment,
        _ => return None,
    };

    let rect_arr = dict.get(b"Rect").ok()?.as_array().ok()?;
    if rect_arr.len() < 4 {
        return None;
    }
    let num = |i: usize| -> f32 {
        rect_arr[i]
            .as_float()
            .or_else(|_| rect_arr[i].as_i64().map(|v| v as f32))
            .unwrap_or(0.0)
    };
    let rect = [num(0), num(1), num(2), num(3)];

    // Ligne/flèche : la /L donne les deux points.
    let mut ink_points = Vec::new();
    if matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow) {
        if let Some(l) = dict.get(b"L").ok().and_then(|o| o.as_array().ok()) {
            let mut pts = Vec::new();
            let mut iter = l.iter();
            while let (Some(x), Some(y)) = (iter.next(), iter.next()) {
                pts.push([
                    x.as_float().unwrap_or(0.0),
                    y.as_float().unwrap_or(0.0),
                ]);
            }
            if pts.len() >= 2 {
                ink_points.push(pts);
            }
        }
    }
    // Polygone/polyligne : les sommets sont dans /Vertices.
    if matches!(kind, AnnotationKind::Polygon | AnnotationKind::PolyLine) {
        if let Some(v) = dict.get(b"Vertices").ok().and_then(|o| o.as_array().ok()) {
            let mut pts = Vec::new();
            let mut iter = v.iter();
            while let (Some(x), Some(y)) = (iter.next(), iter.next()) {
                pts.push([
                    x.as_float().unwrap_or(0.0),
                    y.as_float().unwrap_or(0.0),
                ]);
            }
            if pts.len() >= 2 {
                ink_points.push(pts);
            }
        }
    }

    let content = dict
        .get(b"Contents")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_default();

    // Pour un lien, le content est l'URL (dans /A/URI), pas dans /Contents.
    let content = if matches!(kind, AnnotationKind::Link) {
        dict.get(b"A")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"URI").ok())
            .and_then(|o| o.as_str().ok())
            .map(|s| String::from_utf8_lossy(s).to_string())
            .unwrap_or(content)
    } else {
        content
    };

    let author = dict
        .get(b"T")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_else(|| "YouPDF".to_string());

    let created_at = dict
        .get(b"M")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        // Une date PDF redevient une date lisible ; une chaîne illisible est
        // gardée telle quelle plutôt que remplacée par l'heure courante.
        .map(|raw| match crate::metadata::parse_pdf_date(&raw) {
            Some(date) => date.to_rfc3339(),
            None => raw,
        })
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let color = dict
        .get(b"C")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| {
            let mut c = [0u8; 3];
            for (i, v) in arr.iter().take(3).enumerate() {
                let f = v.as_float().unwrap_or(0.0);
                c[i] = (f * 255.0).clamp(0.0, 255.0) as u8;
            }
            c
        })
        .unwrap_or([255, 235, 59]);

    let opacity = dict
        .get(b"CA")
        .ok()
        .and_then(|o| o.as_float().ok())
        .unwrap_or(0.4);

    let line_width = dict
        .get(b"BS")
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"W").ok())
        .and_then(|o| o.as_float().ok())
        .unwrap_or(1.5);

    let font_size = dict
        .get(b"DA")
        .ok()
        .and_then(|o| o.as_str().ok())
        .and_then(|s| {
            let text = String::from_utf8_lossy(s);
            // Format attendu : "/Helv 11.0 Tf ..." → on extrait le nombre.
            text.split_whitespace()
                .skip_while(|w| !w.ends_with("Tf"))
                .nth(1)
                .and_then(|n| n.parse::<f32>().ok())
                .or_else(|| text.split_whitespace().find_map(|w| w.parse::<f32>().ok()))
        })
        .unwrap_or(11.0);

    let mut ink_points = Vec::new();
    if kind == AnnotationKind::Ink {
        if let Ok(list) = dict.get(b"InkList").and_then(|o| o.as_array()) {
            for stroke_obj in list {
                if let Ok(stroke) = stroke_obj.as_array() {
                    let mut points = Vec::new();
                    let mut iter = stroke.iter();
                    while let (Some(x), Some(y)) = (iter.next(), iter.next()) {
                        points.push([x.as_float().unwrap_or(0.0), y.as_float().unwrap_or(0.0)]);
                    }
                    if points.len() >= 2 {
                        ink_points.push(points);
                    }
                }
            }
        }
    }

    let mut quads = Vec::new();
    if kind.is_markup() {
        if let Ok(points) = dict.get(b"QuadPoints").and_then(|o| o.as_array()) {
            let values: Vec<f32> = points
                .iter()
                .map(|v| v.as_float().unwrap_or(0.0))
                .collect();
            for chunk in values.chunks_exact(8) {
                let mut quad = [0.0f32; 8];
                quad.copy_from_slice(chunk);
                quads.push(quad);
            }
        }
    }

    let state = dict
        .get(b"State")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(AnnotationState::from_pdf_name)
        .unwrap_or_default();

    Some(Annotation {
        id: id.to_string(),
        page,
        kind,
        rect,
        content,
        author,
        style: AnnotationStyle {
            color,
            opacity,
            line_width,
            font_size,
            fill: None,
            dash: BorderDash::Solid,
            cloudy: false,
        },
        ink_points,
        quads,
        created_at,
        state,
        // `/IRT` est une référence indirecte : elle est résolue par
        // `read_annotations`, qui seule a accès au document.
        reply_to: None,
        // Un tampon relu garde son `/Name` : c'est lui qui dit si le texte
        // vient d'un modèle dynamique.
        stamp: dict
            .get(b"Name")
            .ok()
            .and_then(|o| o.as_name().ok())
            .and_then(|name| std::str::from_utf8(name).ok())
            .and_then(stamp_id_for_pdf_name),
    })
}

// ────────────────────────────────────────────────────────────────────────
// Export d'une liste de commentaires
// ────────────────────────────────────────────────────────────────────────

/// Exporte une liste de commentaires vers `format`.
///
/// L'ordre reçu est conservé : l'appelant l'a déjà trié. Les réponses
/// (`reply_to`) sont indentées en Markdown et portent leur lien `/IRT`
/// ailleurs.
pub fn export_comments(annotations: &[Annotation], format: CommentFormat) -> Result<Vec<u8>> {
    match format {
        CommentFormat::Markdown => Ok(export_comments_markdown(annotations).into_bytes()),
        CommentFormat::Csv => Ok(export_comments_csv(annotations).into_bytes()),
        CommentFormat::Xfdf => Ok(export_comments_xfdf(annotations).into_bytes()),
        CommentFormat::Fdf => export_comments_fdf(annotations),
    }
}

/// Nom du sous-type PDF, stable et lisible par les autres outils.
fn comment_type_name(annotation: &Annotation) -> String {
    String::from_utf8_lossy(annotation.kind.pdf_subtype()).to_string()
}

/// Jour seul de l'horodatage (`AAAA-MM-JJ`).
fn comment_day(annotation: &Annotation) -> &str {
    annotation.created_at.get(..10).unwrap_or("")
}

/// Date PDF (`D:AAAAMMJJHHmmSSZ`) depuis l'horodatage interne.
///
/// Les formats d'échange de commentaires attendent une date PDF, pas une
/// chaîne RFC 3339.
fn comment_pdf_date(annotation: &Annotation) -> String {
    chrono::DateTime::parse_from_rfc3339(&annotation.created_at)
        .map(|date| pdf_date(date.with_timezone(&chrono::Utc)))
        .unwrap_or_else(|_| annotation.created_at.clone())
}

fn export_comments_markdown(annotations: &[Annotation]) -> String {
    let mut out = String::from("# Comments\n");
    for annotation in annotations {
        out.push('\n');
        if annotation.reply_to.is_some() {
            // Une réponse se place sous son commentaire parent.
            out.push_str(&format!(
                "#### ↳ Reply · {} · {}\n",
                annotation.author,
                comment_day(annotation)
            ));
        } else {
            out.push_str(&format!(
                "## p.{} · {} · {} · {}\n",
                annotation.page,
                comment_type_name(annotation),
                annotation.author,
                comment_day(annotation)
            ));
            if annotation.state != AnnotationState::None {
                out.push_str(&format!("\n**Status:** {}\n", annotation.state.display_name()));
            }
        }
        if !annotation.content.is_empty() {
            out.push('\n');
            out.push_str(&annotation.content);
            out.push('\n');
        }
    }
    out
}

fn export_comments_csv(annotations: &[Annotation]) -> String {
    use crate::util::csv_escape;

    let mut out = String::from("page,type,author,date,state,reply_to,content\n");
    for annotation in annotations {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            annotation.page,
            csv_escape(&comment_type_name(annotation)),
            csv_escape(&annotation.author),
            csv_escape(comment_day(annotation)),
            csv_escape(annotation.state.display_name()),
            csv_escape(annotation.reply_to.as_deref().unwrap_or("")),
            csv_escape(&annotation.content),
        ));
    }
    out
}

/// Nom d'élément XFDF associé à un type d'annotation.
fn xfdf_element(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Highlight => "highlight",
        AnnotationKind::Underline => "underline",
        AnnotationKind::StrikeOut => "strikeout",
        AnnotationKind::Squiggly => "squiggly",
        AnnotationKind::Note => "text",
        AnnotationKind::FreeText => "freetext",
        AnnotationKind::Square => "square",
        AnnotationKind::Circle => "circle",
        AnnotationKind::Line | AnnotationKind::Arrow => "line",
        AnnotationKind::Polygon => "polygon",
        AnnotationKind::PolyLine => "polyline",
        AnnotationKind::Callout => "callout",
        AnnotationKind::Ink => "ink",
        AnnotationKind::Stamp => "stamp",
        AnnotationKind::Link => "link",
        AnnotationKind::FileAttachment => "fileattachment",
    }
}

fn export_comments_xfdf(annotations: &[Annotation]) -> String {
    use crate::util::escape_xml;

    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n<annots>\n",
    );
    for annotation in annotations {
        let element = xfdf_element(annotation.kind);
        let color = format!(
            "#{:02X}{:02X}{:02X}",
            annotation.style.color[0], annotation.style.color[1], annotation.style.color[2]
        );
        out.push_str(&format!(
            "<{element} page=\"{}\" rect=\"{:.2},{:.2},{:.2},{:.2}\" color=\"{color}\" \
             date=\"{}\" title=\"{}\" name=\"{}\" subject=\"{}\"",
            annotation.page.saturating_sub(1),
            annotation.rect[0],
            annotation.rect[1],
            annotation.rect[2],
            annotation.rect[3],
            escape_xml(&comment_pdf_date(annotation)),
            escape_xml(&annotation.author),
            escape_xml(&annotation.id),
            escape_xml(&comment_type_name(annotation)),
        ));
        if annotation.state != AnnotationState::None {
            out.push_str(&format!(" state=\"{}\"", annotation.state.display_name()));
        }
        if let Some(parent) = &annotation.reply_to {
            out.push_str(&format!(" inreplyto=\"{}\"", escape_xml(parent)));
        }
        out.push('>');
        if !annotation.content.is_empty() {
            out.push_str(&format!(
                "<contents>{}</contents>",
                escape_xml(&annotation.content)
            ));
        }
        out.push_str(&format!("</{element}>\n"));
    }
    out.push_str("</annots>\n</xfdf>\n");
    out
}

/// Dictionnaire FDF d'une annotation (sans `/IRT`, posé au second passage).
fn fdf_annot_dict(annotation: &Annotation) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(annotation.kind.pdf_subtype().to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(annotation.rect[0]),
            Object::Real(annotation.rect[1]),
            Object::Real(annotation.rect[2]),
            Object::Real(annotation.rect[3]),
        ]),
    );
    // `/Page` est l'indice de page, base zéro, comme dans tout FDF.
    d.set(
        "Page",
        Object::Integer(annotation.page.saturating_sub(1) as i64),
    );
    d.set("NM", Object::string_literal(annotation.id.as_str()));
    d.set("T", Object::string_literal(annotation.author.as_str()));
    // `/M` est une date PDF, comme dans `to_pdf_dict`.
    d.set("M", Object::string_literal(comment_pdf_date(annotation)));
    if !annotation.content.is_empty() {
        d.set("Contents", Object::string_literal(annotation.content.as_str()));
    }
    if annotation.state != AnnotationState::None {
        d.set("StateModel", Object::Name(b"Review".to_vec()));
        d.set("State", Object::Name(annotation.state.pdf_name().to_vec()));
    }
    d
}

fn export_comments_fdf(annotations: &[Annotation]) -> Result<Vec<u8>> {
    let mut doc = lopdf::Document::with_version("1.2");

    // Les annotations sont des objets indirects : `/IRT` peut alors viser
    // l'objet parent plutôt que de le recopier.
    let mut ids: Vec<(String, Option<String>, lopdf::ObjectId)> = Vec::new();
    for annotation in annotations {
        let id = doc.add_object(Object::Dictionary(fdf_annot_dict(annotation)));
        ids.push((annotation.id.clone(), annotation.reply_to.clone(), id));
    }
    for (name, reply, id) in &ids {
        let Some(parent) = reply else { continue };
        let Some((_, _, parent_id)) = ids.iter().find(|(other, _, _)| other == parent) else {
            continue;
        };
        if name == parent {
            continue;
        }
        if let Ok(dict) = doc.get_dictionary_mut(*id) {
            dict.set("IRT", Object::Reference(*parent_id));
        }
    }

    let mut fdf = Dictionary::new();
    fdf.set(
        "Annots",
        Object::Array(ids.iter().map(|(_, _, id)| Object::Reference(*id)).collect()),
    );
    let root = doc.add_object(Object::Dictionary(
        dictionary! { "FDF" => Object::Dictionary(fdf) },
    ));
    doc.trailer.set("Root", Object::Reference(root));

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| Error::Pdf(format!("export FDF : {e}")))?;
    // Un FDF est un PDF dont seul l'en-tête change ; les lecteurs refusent
    // un conteneur étiqueté « PDF ».
    if bytes.starts_with(b"%PDF-1.2") {
        bytes[..8].copy_from_slice(b"%FDF-1.2");
    }
    Ok(bytes)
}
