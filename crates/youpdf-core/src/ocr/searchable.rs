//! Réinjection : génération d'un PDF consultable.
//!
//! Chaque page devient une image XObject + un calque texte invisible,
//! ou, en mode modifiable, un fond blanc avec le texte reconnu.

use std::collections::BTreeMap;

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::encode_pdf_literal;
use crate::render::PageBitmap;

use super::{OcrEngine, OcrMode, OcrWord, validate_language};

// Réinjection : génération d'un PDF consultable
// ────────────────────────────────────────────────────────────────────────

/// Convertit un document scanné en PDF où le texte est sélectionnable.
///
/// ## Principe
///
/// Chaque page devient :
///
/// 1. une image XObject (le rendu original) ;
/// 2. un flux de contenu qui dessine l'image ;
/// 3. un calque texte invisible dessus, à la position exacte de chaque mot.
///
/// Le mode de rendu de texte `3 Tr` signifie « ni rempli, ni contourné » :
/// le texte est présent dans le PDF, sélectionnable et cherchable, mais
/// invisible.
///
/// ## Limites
///
/// - L'image est en résolution fixe (2.5× ≈ 180 DPI).
/// - Le texte positionné est en Helvetica : la police d'origine n'est
///   pas reproduite. Cela n'affecte pas la sélection visuelle.
pub fn make_searchable(
    source: &Document,
    renderer: &dyn crate::render::Renderer,
    engine: &dyn OcrEngine,
    language: &str,
) -> Result<Document> {
    make_ocr(
        source,
        renderer,
        engine,
        language,
        OcrMode::Searchable,
    )
}

/// Variante de [`make_searchable`] qui choisit le mode de sortie.
///
/// `Searchable` et `TextAndImage` gardent le contenu d'origine de chaque page
/// et n'ajoutent que le calque texte : aucune ré-rasterisation, donc aucune
/// perte de netteté. `Editable` reconstruit la page sur fond blanc.
pub fn make_ocr(
    source: &Document,
    renderer: &dyn crate::render::Renderer,
    engine: &dyn OcrEngine,
    language: &str,
    mode: OcrMode,
) -> Result<Document> {
    validate_language(language)?;
    if !renderer.renders_real_content() {
        return Err(Error::NotSupported(
            "OCR impossible sans rendu réel : la bibliothèque pdfium est indisponible, \
             et les pages produites seraient des images vides. Installez pdfium \
             (voir pdfium/README.md) puis relancez l'OCR."
                .into(),
        ));
    }
    if source.page_count() == 0 {
        return Err(Error::InvalidInput("document vide".into()));
    }
    match mode {
        OcrMode::Editable => make_ocr_editable(source, renderer, engine, language),
        OcrMode::Searchable | OcrMode::TextAndImage => {
            make_ocr_overlay(source, renderer, engine, language, mode)
        }
    }
}

/// OCR en superposition : on clone le PDF source et on ajoute uniquement le
/// calque texte. Le contenu d'origine (image scan haute résolution, vecteurs,
/// annotations) reste intact — c'est ce qui évite le flou du mode « consultant ».
fn make_ocr_overlay(
    source: &Document,
    renderer: &dyn crate::render::Renderer,
    engine: &dyn OcrEngine,
    language: &str,
    mode: OcrMode,
) -> Result<Document> {
    let mut out = source.inner().clone();
    let font_id = out.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    });

    let opts = crate::render::RenderOptions {
        scale: 2.5,
        rotation: 0,
        antialias: true,
        transparent: false,
        render_annotations: true,
    };

    for page_num in 1..=source.page_count() {
        let bitmap = renderer.render_page(source, page_num as u32, &opts)?;
        let ocr = engine.recognize(&bitmap, page_num, language, mode)?;
        let display = source.page_size(page_num as u32);
        if bitmap.width == 0 || bitmap.height == 0 {
            return Err(Error::InvalidInput(
                "rendu de page vide : impossible de poser la couche texte".into(),
            ));
        }

        // `image_matrix` place l'image rendue dans le repère page (crop box +
        // rotation). Nos coordonnées texte sont celles du rendu, origine en
        // bas à gauche de la boîte visible, déjà mises à l'échelle en points.
        let view = source.page_view(page_num as u32);
        let [a, b, c, d, e, f] = view.image_matrix();
        let m = [
            a / display.0, b / display.0,
            c / display.1, d / display.1,
            e, f,
        ];
        let content = text_layer(&ocr.words, &bitmap, display, m, mode);
        let stream_id = out.add_object(Stream::new(dictionary! {}, content));

        let page_id = source.page_id(page_num as u32)?;
        let mut resources = inherited_resources(&out, page_id);
        let mut fonts = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => out
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("polices de page : {e}")))?
                .clone(),
            _ => Dictionary::new(),
        };
        // Un nom distinct de ceux de la page d'origine : ne jamais écraser
        // une police déjà utilisée par le contenu scanné.
        let mut name = b"YoPdfOcr".to_vec();
        let mut suffix = 1;
        while fonts.get(&name).is_ok() {
            name = format!("YoPdfOcr{suffix}").into_bytes();
            suffix += 1;
        }
        fonts.set(name.clone(), Object::Reference(font_id));
        resources.set(b"Font", Object::Dictionary(fonts));
        out.get_object_mut(page_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| Error::Pdf(format!("page : {e}")))?
            .set(b"Resources", Object::Dictionary(resources));
        append_content(&mut out, page_id, stream_id)?;
    }

    let bytes = {
        let mut buf = Vec::new();
        out.save_to(&mut buf)
            .map_err(|e| Error::Pdf(format!("sérialisation : {e}")))?;
        buf
    };
    Document::from_bytes(&bytes)
}

/// Ressources d'une page, y compris celles héritées de l'arbre `/Pages`.
/// Une page qui possède ses propres `/Resources` ne voit pas les ressources
/// de ses parents : on les recopie donc avant d'ajouter la police OCR.
fn inherited_resources(doc: &lopdf::Document, page_id: ObjectId) -> Dictionary {
    let mut merged = Dictionary::new();
    let mut current = Some(page_id);
    for _ in 0..16 {
        let Some(id) = current else { break };
        let Ok(dict) = doc.get_dictionary(id) else { break };
        if let Ok(Object::Dictionary(d)) = dict.get_deref(b"Resources", doc) {
            for (key, value) in d.iter() {
                if merged.get(key).is_err() {
                    merged.set(key.clone(), value.clone());
                }
            }
        }
        current = dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    merged
}

/// Ajoute un flux à la fin du contenu d'une page.
fn append_content(doc: &mut lopdf::Document, page_id: ObjectId, stream_id: ObjectId) -> Result<()> {
    let page = doc
        .get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| Error::Pdf(format!("page : {e}")))?;
    match page.get_mut(b"Contents") {
        Ok(Object::Reference(id)) => {
            let existing = *id;
            page.set(
                b"Contents",
                Object::Array(vec![Object::Reference(existing), Object::Reference(stream_id)]),
            );
        }
        Ok(Object::Array(arr)) => arr.push(Object::Reference(stream_id)),
        _ => page.set(b"Contents", Object::Reference(stream_id)),
    }
    Ok(())
}

/// Construit le calque texte en coordonnées de rendu (points, origine bas-gauche
/// de la boîte visible). `matrix` ramène ces coordonnées dans le repère page.
fn text_layer(
    words: &[OcrWord],
    bitmap: &PageBitmap,
    display: (f32, f32),
    matrix: [f32; 6],
    mode: OcrMode,
) -> Vec<u8> {
    let sx = display.0 / bitmap.width as f32;
    let sy = display.1 / bitmap.height as f32;
    let mut content: Vec<u8> = Vec::new();
    content.extend_from_slice(b"q\n");
    content.extend_from_slice(
        format!("{} {} {} {} {} {} cm\n", matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]).as_bytes(),
    );
    content.extend_from_slice(b"BT\n");
    content.extend_from_slice(if mode == OcrMode::Searchable { b"3 Tr\n" } else { b"0 Tr\n" });
    content.extend_from_slice(b"0 g\n");

    // Un paragraphe détecté par tesseract est un bloc de texte cohérent :
    // la police est la même pour tous ses mots (une seule Helvetica ici) et
    // la taille est la médiane des hauteurs d'encre. Sans ce regroupement,
    // chaque mot avait sa propre taille et le calque texte paraissait bancal
    // alors que le scan, lui, était uniforme.
    let mut paragraphs: BTreeMap<(i32, i32), Vec<&OcrWord>> = BTreeMap::new();
    for word in words {
        paragraphs
            .entry((word.block, word.paragraph))
            .or_default()
            .push(word);
    }

    for group in paragraphs.values() {
        let mut heights: Vec<f32> = group.iter().map(|w| w.height as f32 * sy).collect();
        let median_height = median(&mut heights);
        let descent = if group.iter().any(|w| has_descender(&w.text)) {
            0.207
        } else {
            0.0
        };
        // Helvetica : hauteur de capitale ≈ 0.718 em, descente ≈ 0.207 em.
        // La boîte tesseract mesure l'encre, pas l'em.
        let size = (median_height / (0.718 + descent)).max(4.0);

        let mut lines: BTreeMap<i32, Vec<&OcrWord>> = BTreeMap::new();
        for word in group {
            lines.entry(word.line).or_default().push(*word);
        }
        for line_words in lines.values() {
            // Ligne de base commune à tous les mots d'une ligne : la médiane
            // des bas de boîtes absorbe le bruit de quelques glyphes.
            let mut bottoms: Vec<f32> = line_words
                .iter()
                .map(|w| (w.top + w.height) as f32 * sy)
                .collect();
            let y = display.1 - median(&mut bottoms) + descent * size;

            for word in line_words {
                let x = word.left as f32 * sx - 0.06 * size; // avance latérale
                content.extend_from_slice(format!("/YoPdfOcr {size:.2} Tf\n").as_bytes());
                content.extend_from_slice(format!("1 0 0 1 {x:.2} {y:.2} Tm\n").as_bytes());
                content.push(b'(');
                content.extend_from_slice(&encode_pdf_literal(&word.text));
                content.extend_from_slice(b") Tj\n");
                // Chaque mot a son propre Tm/Tj : sans espace explicite, les
                // lecteurs concatènent les mots (« HelloWorld ») et la
                // recherche par phrase échoue.
                content.extend_from_slice(b"( ) Tj\n");
            }
        }
    }

    content.extend_from_slice(b"ET\nQ\n");
    content
}

/// Valeur centrale d'une série de mesures bruitées (hauteurs, bas de boîtes).
fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) * 0.5
    }
}

/// Caractères latins dont le glyphe descend sous la ligne de base.
fn has_descender(text: &str) -> bool {
    text.chars().any(|c| matches!(
        c,
        'g' | 'j' | 'p' | 'q' | 'y' | 'G' | 'J' | 'Q' | 'ç' | 'Ç' |
        ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '_' | '|'
    ))
}

/// Ancien chemin « fond blanc + texte visible » : utilisé par `Editable`.
fn make_ocr_editable(
    source: &Document,
    renderer: &dyn crate::render::Renderer,
    engine: &dyn OcrEngine,
    language: &str,
) -> Result<Document> {
    use lopdf::Document as LopdfDoc;

    // On valide le langage **avant** de rendre la première page : sur un
    // document de 500 pages, l'erreur sinon n'arriverait qu'après des
    // dizaines de secondes de travail.
    validate_language(language)?;

    // Le mode modifiable **reconstruit** le document à partir du rendu : avec
    // un renderer qui ne dessine rien, chaque page devient vide et le texte
    // d'origine est perdu — pour un résultat renvoyé en `Ok`.
    if !renderer.renders_real_content() {
        return Err(Error::NotSupported(
            "OCR impossible sans rendu réel : la bibliothèque pdfium est indisponible, \
             et les pages produites seraient des images vides. Installez pdfium \
             (voir pdfium/README.md) puis relancez l'OCR."
                .into(),
        ));
    }

    let page_count = source.page_count();
    if page_count == 0 {
        return Err(Error::InvalidInput("document vide".into()));
    }

    let mut out = LopdfDoc::with_version("1.7");

    // Objets partagés : polices et arbre de pages.
    let pages_id = out.new_object_id();

    // Helvetica est une des 14 polices standard : pas de fichier à embarquer.
    let helv_id = out.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    });

    let mut page_ids: Vec<Object> = Vec::new();

    for page_num in 1..=page_count {
        // 1. Rendu haute résolution.
        let opts = crate::render::RenderOptions {
            scale: 2.5,
            rotation: 0,
            antialias: true,
            transparent: false,
            // Les annotations font partie de ce que l'utilisateur voit,
            // donc de ce qu'il doit pouvoir retrouver par recherche.
            render_annotations: true,
        };
        let bitmap = renderer.render_page(source, page_num as u32, &opts)?;

        // 2. OCR. Le fond blanc remplace le scan : seul le texte est visible.
        let ocr = engine.recognize(&bitmap, page_num, language, OcrMode::Editable)?;
        let (w_pt, h_pt) = source.page_size(page_num as u32);
        if bitmap.width == 0 || bitmap.height == 0 {
            return Err(Error::InvalidInput(
                "rendu de page vide : impossible de poser la couche texte".into(),
            ));
        }
        let content = text_layer(
            &ocr.words,
            &bitmap,
            (w_pt, h_pt),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            OcrMode::Editable,
        );
        let content_stream = Stream::new(dictionary! {}, content);
        let content_id = out.add_object(content_stream);

        // 6. Dictionnaire de page (fond blanc, texte visible).
        let resources = dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "YoPdfOcr" => Object::Reference(helv_id),
            }),
        };
        let page_dict = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(w_pt),
                Object::Real(h_pt),
            ]),
            "Resources" => Object::Dictionary(resources),
            "Contents" => Object::Reference(content_id),
        };
        let page_id = out.add_object(page_dict);
        page_ids.push(Object::Reference(page_id));
    }

    // 7. Arbre des pages.
    let pages_dict = dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(page_ids),
        "Count" => Object::Integer(page_count as i64),
    };
    out.objects.insert(pages_id, Object::Dictionary(pages_dict));

    // 8. Catalogue.
    let catalog_id = out.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    });
    out.trailer.set("Root", Object::Reference(catalog_id));

    // 9. Charger en mémoire pour reconstruire un `Document` YouPDF.
    let bytes = {
        let mut buf = Vec::new();
        out.save_to(&mut buf)
            .map_err(|e| Error::Pdf(format!("sérialisation : {e}")))?;
        buf
    };
    Document::from_bytes(&bytes)
}

impl Document {
    /// Part de la page couverte par la plus grande image posée par son
    /// contenu, entre 0 et 1.
    ///
    /// Heuristique volontairement simple : seuls les `cm ... Do` de premier
    /// niveau sont lus. Les images imbriquées dans un formulaire XObject ou
    /// dans un `q/Q` transformé ne sont pas mesurées ; elles restent détectées
    /// par [`Document::looks_scanned`] via la présence d'un texte vide.
    pub fn page_image_coverage(&self, page: u32) -> f32 {
        let Ok(page_id) = self.page_id(page) else {
            return 0.0;
        };
        let Ok(content) = self.inner().get_page_content(page_id) else {
            return 0.0;
        };
        let (width, height) = self.page_size(page);
        let surface = (width * height).max(1.0);
        let mut operands: Vec<f32> = Vec::new();
        let mut last_matrix: Option<[f32; 6]> = None;
        let mut best = 0.0_f32;
        for token in content.split(|byte| byte.is_ascii_whitespace()) {
            match token {
                b"cm" => {
                    if operands.len() >= 6 {
                        let start = operands.len() - 6;
                        last_matrix = Some([
                            operands[start],
                            operands[start + 1],
                            operands[start + 2],
                            operands[start + 3],
                            operands[start + 4],
                            operands[start + 5],
                        ]);
                    }
                    operands.clear();
                }
                b"Do" => {
                    if let Some(matrix) = last_matrix {
                        let area = (matrix[0] * matrix[3] - matrix[1] * matrix[2]).abs();
                        best = best.max((area / surface).min(1.0));
                    }
                    operands.clear();
                }
                other => match std::str::from_utf8(other).ok().and_then(|text| text.parse::<f32>().ok()) {
                    Some(value) => operands.push(value),
                    None => operands.clear(),
                },
            }
        }
        best
    }

    /// Vrai si la page ressemble à un scan : pas de texte extractible et une
    /// image couvrant au moins 80 % de la surface.
    pub fn looks_scanned(&self, page: u32) -> bool {
        let has_text = self
            .extract_text_fragments(page)
            .map(|fragments| fragments.iter().any(|f| !f.text.trim().is_empty()))
            .unwrap_or(true);
        !has_text && self.page_image_coverage(page) >= 0.8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, left: i32, top: i32, height: i32, line: i32) -> OcrWord {
        OcrWord {
            left,
            top,
            width: 40,
            height,
            confidence: 95,
            text: text.to_string(),
            block: 1,
            paragraph: 1,
            line,
        }
    }

    /// Deux mots d'un même paragraphe avec des hauteurs légèrement bruitées
    /// reçoivent la même taille, et les mots d'une même ligne la même base.
    #[test]
    fn paragraph_words_share_one_font_size() {
        let bitmap = PageBitmap::solid(1000, 500, [255, 255, 255, 255]);
        let words = vec![
            word("Hello", 100, 100, 20, 1),
            word("World", 200, 102, 22, 1),
            word("Again", 100, 200, 21, 2),
        ];
        let content = text_layer(
            &words,
            &bitmap,
            (500.0, 250.0),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            OcrMode::Searchable,
        );
        let content = String::from_utf8(content).expect("contenu ASCII");

        let sizes: Vec<&str> = content
            .lines()
            .filter_map(|line| {
                line.strip_prefix("/YoPdfOcr ")
                    .and_then(|rest| rest.split_whitespace().next())
            })
            .collect();
        assert_eq!(sizes.len(), 3);
        assert!(
            sizes.iter().all(|size| *size == sizes[0]),
            "tailles différentes dans un même paragraphe : {sizes:?}"
        );

        let baselines: Vec<&str> = content
            .lines()
            .filter(|line| line.contains(" Tm"))
            .filter_map(|line| {
                line.strip_prefix("1 0 0 1 ")
                    .and_then(|rest| rest.split_whitespace().nth(1))
            })
            .collect();
        assert_eq!(baselines.len(), 3);
        assert_eq!(baselines[0], baselines[1], "même ligne, même base");
        assert_ne!(baselines[0], baselines[2], "lignes distinctes");
    }
}
