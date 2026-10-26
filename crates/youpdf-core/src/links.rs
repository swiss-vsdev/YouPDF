//! Liens des annotations : ce qu'un clic sur un lien doit déclencher.
//!
//! Un lien PDF porte son action dans `/A` (dictionnaire d'action) ou dans
//! `/Dest` (destination directe). L'action la plus courante est `/URI`, qui
//! porte l'adresse à ouvrir ; les autres sont les sauts internes (`/GoTo`),
//! les lancements d'application (`/Launch`), les actions de document
//! (`/Named`) et le son (`/Sound`).
//!
//! Le lecteur ne connaît que ce qu'il peut faire : il rend l'action *sous la
//! forme* d'un objet, et c'est l'appelant qui décide ce qu'il en fait — ouvrir
//! un navigateur, changer de page, ou ne rien faire.

use lopdf::{Dictionary, Object};

use crate::document::Document;

/// Ce qu'un clic sur un lien déclenche.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkAction {
    /// Ouvrir une adresse (`/URI`).
    Uri(String),
    /// Aller à une page du document (`/GoTo` ou `/Dest`).
    GoTo {
        /// Numéro de page, 1-indexé.
        page: usize,
    },
    /// Une action que l'application ne sait pas exécuter (`/Launch`,
    /// `/Named`, `/Sound`, `/Rendition`…).
    ///
    /// Le nom de l'action est gardé pour pouvoir l'afficher.
    Unsupported(String),
}

impl LinkAction {
    /// L'adresse à ouvrir, si l'action en est une.
    pub fn uri(&self) -> Option<&str> {
        match self {
            LinkAction::Uri(uri) => Some(uri),
            _ => None,
        }
    }

    /// La page visée, si l'action en est une.
    pub fn page(&self) -> Option<usize> {
        match self {
            LinkAction::GoTo { page } => Some(*page),
            _ => None,
        }
    }
}

/// Un lien trouvé sur une page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLink {
    /// Numéro de page, 1-indexé.
    pub page: usize,
    /// Rectangle du lien, en points, dans le repère de la page.
    pub rect: [f32; 4],
    /// Ce que le lien déclenche.
    pub action: LinkAction,
}

impl Document {
    /// Liens d'une page, avec l'action de chacun.
    ///
    /// Les annotations sans action lisible sont ignorées : un lien sans
    /// destination ne déclenche rien, l'afficher serait du bruit.
    pub fn page_links(&self, page: usize) -> Vec<PageLink> {
        let Ok(page_id) = self.page_id(page as u32) else {
            return Vec::new();
        };
        let Ok(page_dict) = self.inner().get_dictionary(page_id) else {
            return Vec::new();
        };
        let Ok(annots) = page_dict.get(b"Annots") else {
            return Vec::new();
        };
        let annots = match annots {
            Object::Array(items) => items.clone(),
            Object::Reference(id) => self
                .inner()
                .get_object(*id)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };

        let mut out = Vec::new();
        for annot in annots {
            let Some(dict) = self.annot_dict(&annot) else {
                continue;
            };
            if dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Link") {
                continue;
            }
            let Some(rect) = annot_rect(&dict) else {
                continue;
            };
            let Some(action) = self.link_action(&dict) else {
                continue;
            };
            out.push(PageLink {
                page,
                rect,
                action,
            });
        }
        out
    }

    /// Dictionnaire d'une annotation, qu'elle soit en ligne ou référencée.
    fn annot_dict(&self, annot: &Object) -> Option<Dictionary> {
        match annot {
            Object::Dictionary(dict) => Some(dict.clone()),
            Object::Reference(id) => self.inner().get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }

    /// Action d'un lien : `/A` d'abord, `/Dest` ensuite.
    ///
    /// `/A` est prioritaire : une annotation qui porte les deux est rare, et
    /// `/A` est ce que les lecteurs d'écran et les autres outils suivent en
    /// premier.
    fn link_action(&self, dict: &Dictionary) -> Option<LinkAction> {
        if let Some(action) = dict.get(b"A").ok().and_then(|o| self.dict_of(o)) {
            if let Some(action) = self.action_from(&action) {
                return Some(action);
            }
        }
        // Pas d'action : le `/Dest` direct joue ce rôle.
        let dest = dict.get(b"Dest").ok()?;
        self.destination_page(dest)
    }

    /// Action lue dans un dictionnaire `/A`.
    fn action_from(&self, action: &Dictionary) -> Option<LinkAction> {
        let kind = action.get(b"S").ok().and_then(|o| o.as_name().ok())?;
        match kind {
            b"URI" => {
                let uri = action.get(b"URI").ok().and_then(|o| o.as_str().ok())?;
                let uri = String::from_utf8_lossy(uri);
                let uri = uri.trim();
                if uri.is_empty() {
                    None
                } else {
                    Some(LinkAction::Uri(uri.to_owned()))
                }
            }
            b"GoTo" => {
                let dest = action.get(b"D").ok()?;
                self.destination_page(dest)
            }
            b"GoToR" | b"Launch" | b"Named" | b"Sound" | b"Rendition" | b"Movie" => {
                Some(LinkAction::Unsupported(
                    String::from_utf8_lossy(kind).into_owned(),
                ))
            }
            _ => None,
        }
    }

    /// Page visée par une destination.
    ///
    /// Une destination est un tableau dont le premier élément désigne la page :
    /// un objet indirect (référence), un objet page en ligne, ou un nom. Les
    /// deux premiers se résolvent ; un nom (`/Dest` nommé) ne se résout pas
    /// sans le catalogue des destinations nommées, que le lecteur n'indexe
    /// pas.
    fn destination_page(&self, dest: &Object) -> Option<LinkAction> {
        let target = match dest {
            Object::Array(items) => items.first()?,
            Object::Reference(_) => dest,
            _ => return None,
        };
        // Une référence directe vers un objet page.
        if let Object::Reference(id) = target {
            if let Some(number) = self.page_number_of(*id) {
                return Some(LinkAction::GoTo {
                    page: number as usize,
                });
            }
            // La référence peut pointer vers un dictionnaire page.
            if let Ok(dict) = self.inner().get_dictionary(*id) {
                if let Ok(Object::Reference(page_id)) = dict.get(b"Parent") {
                    if let Some(number) = self.page_number_of(*page_id) {
                        return Some(LinkAction::GoTo {
                            page: number as usize,
                        });
                    }
                }
            }
            return None;
        }
        // Un objet page en ligne : son parent désigne la page du catalogue.
        let dict = target.as_dict().ok()?;
        let parent = dict.get(b"Parent").ok()?;
        let id = match parent {
            Object::Reference(id) => *id,
            _ => return None,
        };
        let number = self.page_number_of(id)?;
        Some(LinkAction::GoTo {
            page: number as usize,
        })
    }

    /// Dictionnaire derrière un objet, qu'il soit en ligne ou référencé.
    fn dict_of(&self, object: &Object) -> Option<Dictionary> {
        match object {
            Object::Dictionary(dict) => Some(dict.clone()),
            Object::Reference(id) => self.inner().get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }
}

/// Rectangle `/Rect` d'une annotation, en points.
fn annot_rect(dict: &Dictionary) -> Option<[f32; 4]> {
    let items = dict.get(b"Rect").ok()?.as_array().ok()?;
    if items.len() < 4 {
        return None;
    }
    let number = |index: usize| -> Option<f32> {
        items[index]
            .as_float()
            .ok()
            .or_else(|| items[index].as_i64().ok().map(|v| v as f32))
    };
    let x1 = number(0)?;
    let y1 = number(1)?;
    let x2 = number(2)?;
    let y2 = number(3)?;
    Some([x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)])
}

/// Apparence d'un lien : ce que l'on voit de son cadre.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkAppearance {
    /// Le trait est-il visible ?
    pub visible: bool,
    /// Épaisseur du trait, en points.
    pub width: f32,
    /// Pointillé `/D` : `[dash, gap]`. `None` = trait plein.
    pub dash: Option<Vec<f32>>,
    /// Couleur RVB du trait.
    pub color: [u8; 3],
}

impl Default for LinkAppearance {
    fn default() -> Self {
        Self {
            visible: true,
            width: 1.0,
            dash: None,
            color: [0, 0, 255],
        }
    }
}

impl LinkAppearance {
    /// Un lien visible, fin, bleu, sans pointillé.
    pub fn default_visible() -> Self {
        Self::default()
    }
}

/// Cherche l'`ObjectId` de l'annotation `Link` de `doc` dont le `/Rect`
/// est à `rect` (aux erreurs flottantes près), sur la page `page`.
fn find_link_id(doc: &Document, page: usize, rect: [f32; 4]) -> Option<lopdf::ObjectId> {
    let page_id = doc.page_id(page as u32).ok()?;
    let page_dict = doc.inner().get_dictionary(page_id).ok()?;
    let annots = page_dict.get(b"Annots").ok()?;
    let items: Vec<Object> = match annots {
        Object::Array(items) => items.clone(),
        Object::Reference(id) => doc
            .inner()
            .get_object(*id)
            .ok()
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default(),
        _ => return None,
    };
    for item in &items {
        let id = match item {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let Ok(dict) = doc.inner().get_dictionary(id) else {
            continue;
        };
        if dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Link") {
            continue;
        }
        let Some(found) = annot_rect(dict) else {
            continue;
        };
        let close = (found[0] - rect[0]).abs() < 0.01
            && (found[1] - rect[1]).abs() < 0.01
            && (found[2] - rect[2]).abs() < 0.01
            && (found[3] - rect[3]).abs() < 0.01;
        if close {
            return Some(id);
        }
    }
    None
}

/// Cherche le dictionnaire de l'annotation `Link` en lecture seule.
fn find_link_annot(doc: &Document, page: usize, rect: [f32; 4]) -> Option<&Dictionary> {
    let id = find_link_id(doc, page, rect)?;
    doc.inner().get_dictionary(id).ok()
}

/// Écris l'apparence (`/Border`, `/C`) dans le dictionnaire de l'annotation.
///
/// `/Border` = `[dash_array_width h v dash_pattern]`, où `width=0` rend le
/// trait invisible. `/C` porte la couleur RVB normalisée.
pub fn set_link_appearance(
    doc: &mut Document,
    page: usize,
    rect: [f32; 4],
    appearance: &LinkAppearance,
) -> youpdf_shared::Result<()> {
    let width = if appearance.visible { appearance.width } else { 0.0 };
    let dash_array: Vec<Object> = appearance
        .dash
        .as_ref()
        .map(|v| v.iter().map(|&x| Object::Real(x)).collect())
        .unwrap_or_default();
    let border = Object::Array(vec![
        Object::Real(0.0),
        Object::Real(0.0),
        Object::Real(width),
        Object::Array(dash_array),
    ]);
    let color = Object::Array(vec![
        Object::Real(appearance.color[0] as f32 / 255.0),
        Object::Real(appearance.color[1] as f32 / 255.0),
        Object::Real(appearance.color[2] as f32 / 255.0),
    ]);
    let dict = {
        let id = find_link_id(doc, page, rect).ok_or_else(|| {
            youpdf_shared::Error::Pdf("link annotation not found".into())
        })?;
        doc.inner_mut().get_dictionary_mut(id).map_err(|e| {
            youpdf_shared::Error::Pdf(e.to_string())
        })?
    };
    dict.set(b"Border", border);
    dict.set(b"C", color);
    Ok(())
}

/// Relit l'apparence depuis `/Border` et `/C`.
pub fn get_link_appearance(
    doc: &Document,
    page: usize,
    rect: [f32; 4],
) -> Option<LinkAppearance> {
    let dict = find_link_annot(doc, page, rect)?;
    let mut appearance = LinkAppearance::default();
    if let Ok(Object::Array(items)) = dict.get(b"Border") {
        if let Some(w) = items.get(2).and_then(|o| o.as_float().ok()) {
            appearance.visible = w > 0.0;
            appearance.width = w;
        }
        if let Some(Object::Array(dash)) = items.get(3) {
            if !dash.is_empty() {
                appearance.dash = Some(
                    dash.iter()
                        .filter_map(|o| o.as_float().ok())
                        .collect(),
                );
            }
        }
    }
    if let Ok(Object::Array(items)) = dict.get(b"C") {
        let c: Vec<f32> = items
            .iter()
            .filter_map(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|v| v as f32)))
            .collect();
        if c.len() >= 3 {
            appearance.color = [
                (c[0] * 255.0).round() as u8,
                (c[1] * 255.0).round() as u8,
                (c[2] * 255.0).round() as u8,
            ];
        }
    }
    Some(appearance)
}
