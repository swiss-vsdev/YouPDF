//! Écriture des annotations natives dans un `Document`.

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use crate::document::Document;

use super::render::{form_bbox, Paintable};
use super::NM_PREFIX;
use youpdf_shared::{Error, Result};

use super::{Annotation, AnnotationKind, AnnotationState, AnnotationStyle, BorderDash, stamp_id_for_pdf_name};

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
impl Document {
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
