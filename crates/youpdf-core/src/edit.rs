//! Édition de contenu : extraction de texte positionné et d'images.
//!
//! ## Pourquoi ce module
//!
//! Modifier le texte dans un PDF est **fondamentalement difficile** :
//! le contenu est une séquence d'instructions graphiques (`BT`, `Tj`,
//! `Td`…), pas une structure éditable. Pour changer un mot, il faut
//! soit régénérer le flux de contenu, soit recouvrir le texte original
//! par un rectangle de couleur et y placer du nouveau texte.
//!
//! YouPDF adopte une approche pragmatique et **transparente** :
//!
//! 1. **Extraction** : on identifie chaque fragment de texte avec sa
//!    position, sa police et sa taille. Cela alimente l'interface
//!    d'édition (cliquer sur un mot, le modifier).
//!
//! 2. **Remplacement** : on applique un « patch » qui consiste à
//!    dessiner un rectangle blanc sur l'ancien texte, puis à écrire
//!    le nouveau au même endroit, avec une police standard.
//!
//! Cette approche est **visuellement fiable** pour les corrections
//! simples (coquilles, remplacements ponctuels). Elle n'est **pas**
//! un vrai retraitement typographique, et l'utilisateur doit le savoir.
//!
//! ## Ce qui n'est pas fait
//!
//! - Reprogrammation complète des flux de contenu (trop fragile).
//! - Préservation exacte des polices non standard.
//! - Réécriture des paragraphes avec retour à la ligne.

use lopdf::{Dictionary, Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;

/// Un fragment de texte avec sa position et son style.
#[derive(Debug, Clone)]
pub struct TextFragment {
    /// Numéro de page (1-indexé).
    pub page: usize,
    /// Texte.
    pub text: String,
    /// Rectangle englobant en coordonnées PDF `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
    /// Nom de la police (informative).
    pub font: Option<String>,
    /// Taille de police en points.
    pub font_size: f32,
    /// Couleur RGB (0–255).
    pub color: [u8; 3],
}

/// Une plage de texte à l'intérieur d'un fragment.
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub fragment_index: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Informations sur une image embarquée dans un document.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    /// Page contenant l'image.
    pub page: usize,
    /// Nom de ressource (`/Im0`, `/Im1`…).
    pub resource_name: String,
    /// Largeur en pixels.
    pub width: u32,
    /// Hauteur en pixels.
    pub height: u32,
    /// Nombre de composantes couleur (1, 3 ou 4).
    pub components: u8,
    /// Référence de l'objet image dans le document.
    pub object_id: (u32, u16),
}

// ────────────────────────────────────────────────────────────────────────
// Extraction
// ────────────────────────────────────────────────────────────────────────

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

            let width = dict
                .get(b"Width")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as u32;
            let height = dict
                .get(b"Height")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0) as u32;

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
}

// ────────────────────────────────────────────────────────────────────────
// Aides internes
// ────────────────────────────────────────────────────────────────────────

/// Découpe grossièrement un flux de contenu en tokens.
///
/// Reconnaît : noms (`/X`), chaînes `(…)`, tableaux `[…]`, nombres,
/// opérateurs. Ne gère pas les chaînes hexadécimales `<…>` (rare pour
/// du texte).
fn tokenize_content(content: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(content);
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        // Chaîne littérale (…)
        if c == '(' {
            chars.next();
            let mut depth = 1;
            let mut s = String::new();
            while let Some(&nc) = chars.peek() {
                chars.next();
                match nc {
                    '\\' => {
                        // Caractère d'échappement : on le consomme et on
                        // ajoute le suivant.
                        if let Some(&escaped) = chars.peek() {
                            chars.next();
                            match escaped {
                                'n' => s.push('\n'),
                                'r' => s.push('\r'),
                                't' => s.push('\t'),
                                other => s.push(other),
                            }
                        }
                    }
                    '(' => {
                        depth += 1;
                        s.push('(');
                    }
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        s.push(')');
                    }
                    other => s.push(other),
                }
            }
            tokens.push(format!("({s})"));
            continue;
        }

        // Nom /X
        if c == '/' {
            chars.next();
            let mut s = String::from("/");
            while let Some(&nc) = chars.peek() {
                if nc.is_whitespace() || nc == '/' || nc == '(' || nc == '[' {
                    break;
                }
                s.push(nc);
                chars.next();
            }
            tokens.push(s);
            continue;
        }

        // Opérateur ou nombre
        let mut s = String::new();
        while let Some(&nc) = chars.peek() {
            if nc.is_whitespace() || nc == '(' || nc == '/' || nc == '[' || nc == ']' {
                break;
            }
            s.push(nc);
            chars.next();
        }
        if !s.is_empty() {
            tokens.push(s);
        }
    }

    tokens
}

/// Extrait le contenu d'un token de type `(…)` s'il y en a un.
fn extract_string_literal(token: &str) -> Option<String> {
    if token.starts_with('(') && token.ends_with(')') {
        Some(token[1..token.len() - 1].to_string())
    } else {
        None
    }
}

/// Longueur du dernier fragment poussé (pour l'avance horizontale).
fn current_text_len(fragments: &[TextFragment]) -> usize {
    fragments
        .last()
        .map(|f| f.text.chars().count())
        .unwrap_or(0)
}

/// Pousse ou accumule un fragment de texte.
#[allow(clippy::too_many_arguments)]
fn push_fragment(
    fragments: &mut Vec<TextFragment>,
    current: &mut Option<TextFragment>,
    text: String,
    page: usize,
    tx: f32,
    ty: f32,
    font_size: f32,
    font_name: Option<String>,
) {
    if text.trim().is_empty() {
        return;
    }

    match current {
        Some(frag) => {
            // On continue le fragment courant : le texte suit sur la
            // même ligne (même ty à la tolérance près).
            if (frag.rect[1] - ty).abs() < font_size * 0.3 {
                frag.text.push_str(&text);
                // On étend le rectangle.
                frag.rect[2] += font_size * 0.5 * text.chars().count() as f32;
            } else {
                fragments.push(frag.clone());
                *frag = new_fragment(text, page, tx, ty, font_size, font_name);
            }
        }
        None => {
            *current = Some(new_fragment(text, page, tx, ty, font_size, font_name));
        }
    }
}

fn new_fragment(
    text: String,
    page: usize,
    tx: f32,
    ty: f32,
    font_size: f32,
    font_name: Option<String>,
) -> TextFragment {
    let width = font_size * 0.5 * text.chars().count() as f32;
    TextFragment {
        page,
        text,
        rect: [tx, ty, tx + width, ty + font_size],
        font: font_name,
        font_size,
        color: [0, 0, 0],
    }
}

// ────────────────────────────────────────────────────────────────────────
// Remplacement de texte par « patch »
// ────────────────────────────────────────────────────────────────────────

impl Document {
    /// Remplace un fragment de texte par un nouveau contenu.
    ///
    /// ## Méthode
    ///
    /// 1. On dessine un rectangle blanc sur l'ancien fragment.
    /// 2. On écrit le nouveau texte au même emplacement, avec Helvetica.
    ///
    /// ## Limites
    ///
    /// - La police d'origine n'est pas reproduite : on utilise
    ///   Helvetica, l'une des 14 polices standard PDF.
    /// - Le style (gras, italique) n'est pas préservé.
    /// - Le texte n'est pas ré-encodable dans le flux d'origine : il est
    ///   écrit par-dessus. Le contenu original reste présent dans le PDF
    ///   (récupérable par extraction), ce qui **doit** être su si le but
    ///   est de supprimer une information sensible (utiliser le
    ///   caviardage pour ça).
    pub fn replace_text_fragment(&mut self, fragment: &TextFragment, new_text: &str) -> Result<()> {
        let page_id = self
            .page_id(fragment.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        // 1. Préparer le flux de patch.
        let [x1, y1, x2, y2] = fragment.rect;
        let w = x2 - x1;
        let h = y2 - y1;

        // Le flux est construit en **octets** : un `String` intermédiaire puis
        // `into_bytes()` ré-encoderait chaque octet WinAnsi
        // >= 0x80 en deux octets UTF-8, ce qui reproduirait exactement le
        // défaut d'affichage des accents que ce correctif supprime.
        let mut content: Vec<u8> = Vec::new();

        // Rectangle blanc de masquage (légèrement plus grand pour être sûr).
        content.extend_from_slice(
            format!("q\n1 1 1 rg\n{:.2} {:.2} {:.2} {:.2} re\nf\nQ\n",
                x1 - 1.0,
                y1 - 1.0,
                w + 2.0,
                h + 2.0)
            .as_bytes(),
        );

        // Texte de remplacement.
        let safe = encode_pdf_literal(new_text);
        content.extend_from_slice(
            format!(
                "BT\n/YouPdfF1 {:.2} Tf\n{:.2} {:.2} Td\n",
                fragment.font_size,
                x1,
                y1 + fragment.font_size * 0.2
            )
            .as_bytes(),
        );
        content.push(b'(');
        content.extend_from_slice(&safe);
        content.extend_from_slice(b") Tj\nET\n");

        let patch = Stream::new(Dictionary::new(), content);
        let patch_id = self.inner_mut().add_object(patch);

        // 2. Police dédiée aux patchs (Helvetica + WinAnsi), indépendante
        //    des polices déjà présentes sur la page.
        self.ensure_patch_font(page_id)?;

        // 3. Concaténer le patch au flux de contenu.
        self.append_to_page_content(page_id, patch_id)?;

        Ok(())
    }

    /// Garantit qu'une police Helvetica est disponible dans le document.
    ///
    /// Retourne sa référence. Réutilise une police existante si elle
    /// est déjà présente.
    /// Nom de ressource de la police posée par les patchs YouPDF.
    ///
    /// On n'utilise **jamais** le `/F1` de la page : cette police peut être
    /// une Typ1 sans `/Encoding` (StandardEncoding), une police sous-ensemblée
    /// en `Identity-H` (UTF-16) ou toute autre encodage. Y écrire des octets
    /// WinAnsi produit du texte illisible. Une police dédiée garantit
    /// l'encodage et n'écrase rien.
    const PATCH_FONT_NAME: &[u8] = b"YouPdfF1";

    /// Renvoie une Helvetica /WinAnsiEncoding dédiée aux patchs, créée une
    /// seule fois par page.
    fn ensure_patch_font(&mut self, page_id: (u32, u16)) -> Result<(u32, u16)> {
        // Déjà posée sur cette page ?
        if let Ok(fonts) = self
            .inner()
            .get_dictionary(page_id)
            .and_then(|p| p.get(b"Resources"))
            .and_then(|r| r.as_dict())
            .and_then(|r| r.get(b"Font"))
            .and_then(|f| f.as_dict())
        {
            if let Ok(Object::Reference(id)) = fonts.get(Self::PATCH_FONT_NAME) {
                return Ok(*id);
            }
        }

        let font = Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type1".to_vec())),
            ("BaseFont", Object::Name(b"Helvetica".to_vec())),
            ("Encoding", Object::Name(b"WinAnsiEncoding".to_vec())),
        ]);
        let font_id = self.inner_mut().add_object(Object::Dictionary(font));
        self.link_font_to_page(page_id, "YouPdfF1", font_id)?;
        Ok(font_id)
    }

    /// Enregistre une police dans les ressources `/Font` d'une page.
    fn link_font_to_page(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        font_id: (u32, u16),
    ) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        // Récupère ou crée /Resources.
        // (On clone d'abord l'objet pour libérer l'emprunt mutable sur
        // la page, ce qui permet ensuite d'accéder à `self.inner()`.)
        let resources_obj = dict.get(b"Resources").ok().cloned();
        let mut resources = match resources_obj {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(id)) => self
                .inner()
                .get_dictionary(id)
                .map_err(|e| Error::Pdf(format!("Ressources : {e}")))?
                .clone(),
            _ => Dictionary::new(),
        };

        // Récupère ou crée /Font.
        let mut fonts = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("Font : {e}")))?
                .clone(),
            _ => Dictionary::new(),
        };

        // Associe le nom à la police.
        fonts.set(name, Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(fonts));

        // Écrit dans la page.
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
    fn append_to_page_content(&mut self, page_id: (u32, u16), patch_id: (u32, u16)) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        match dict.get_mut(b"Contents") {
            Ok(Object::Reference(id)) => {
                // Contenu unique : on transforme en tableau.
                let existing = *id;
                dict.set(
                    "Contents",
                    Object::Array(vec![
                        Object::Reference(existing),
                        Object::Reference(patch_id),
                    ]),
                );
            }
            Ok(Object::Array(arr)) => {
                arr.push(Object::Reference(patch_id));
            }
            _ => {
                dict.set("Contents", Object::Reference(patch_id));
            }
        }

        Ok(())
    }
}

/// Échappe une chaîne et l'encode en octets WinAnsi pour un littéral PDF `(…)`.
///
/// La page de codes est celle de la police posée (`WinAnsiEncoding`) : il
/// faut donc écrire **un octet** par caractère. Renvoyer une `String` et la
/// convertir en UTF-8 produisait deux octets pour chaque accentué — « café »
/// sortait en `caf\xc3\xa9`, que le lecteur affiche « cafÃ© ».
pub fn encode_pdf_literal(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.extend_from_slice(b"\\("),
            ')' => out.extend_from_slice(b"\\)"),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' | '\r' | '\t' => out.push(b' '),
            c if (c as u32) < 0x80 => out.push(c as u8),
            // Hors Latin-1/CP1252 (CJK, cyrillique …) : la police standard
            // n'a pas de glyphe garanti → point d'interrogation plutôt
            // qu'un octet arbitraire rendu avec un glyphe au hasard.
            c => out.push(winansi_byte(c).unwrap_or(b'?')),
        }
    }
    out
}

/// Convertit un caractère en octet WinAnsi, avec les écarts usuels entre
/// Latin-1 et CP1252 (guillemets, tirets, points de suspension…).
pub(crate) fn winansi_byte_pub(c: char) -> Option<u8> {
    winansi_byte(c)
}

fn winansi_byte(c: char) -> Option<u8> {
    const CP1252: [(char, u8); 27] = [
        ('\u{20AC}', 0x80), ('\u{201A}', 0x82), ('\u{0192}', 0x83), ('\u{201E}', 0x84),
        ('\u{2026}', 0x85), ('\u{2020}', 0x86), ('\u{2021}', 0x87), ('\u{02C6}', 0x88),
        ('\u{2030}', 0x89), ('\u{0160}', 0x8A), ('\u{2039}', 0x8B), ('\u{0152}', 0x8C),
        ('\u{017D}', 0x8E), ('\u{2018}', 0x91), ('\u{2019}', 0x92), ('\u{201C}', 0x93),
        ('\u{201D}', 0x94), ('\u{2022}', 0x95), ('\u{2013}', 0x96), ('\u{2014}', 0x97),
        ('\u{02DC}', 0x98), ('\u{2122}', 0x99), ('\u{0161}', 0x9A), ('\u{203A}', 0x9B),
        ('\u{0153}', 0x9C), ('\u{017E}', 0x9E), ('\u{0178}', 0x9F),
    ];
    if let Some((_, b)) = CP1252.iter().find(|(ch, _)| *ch == c) {
        return Some(*b);
    }
    let u = c as u32;
    if (0xA0..=0xFF).contains(&u) {
        Some(u as u8)
    } else {
        None
    }
}
