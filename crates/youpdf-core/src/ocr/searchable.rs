//! Réinjection : génération d'un PDF consultable.
//!
//! Chaque page devient une image XObject + un calque texte invisible,
//! ou, en mode modifiable, un fond blanc avec le texte reconnu.

use lopdf::{dictionary, Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::encode_pdf_literal;

use super::{OcrEngine, OcrMode, validate_language};

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
/// - `Searchable` : image d'origine + texte invisible.
/// - `TextAndImage` : image d'origine + texte visible par-dessus.
/// - `Editable` : fond blanc + texte visible, sans image de scan ; le texte
///   reste modifiable et cherchable, mais la texture du papier disparaît.
pub fn make_ocr(
    source: &Document,
    renderer: &dyn crate::render::Renderer,
    engine: &dyn OcrEngine,
    language: &str,
    mode: OcrMode,
) -> Result<Document> {
    use lopdf::Document as LopdfDoc;

    // On valide le langage **avant** de rendre la première page : sur un
    // document de 500 pages, l'erreur sinon n'arriverait qu'après des
    // dizaines de secondes de travail.
    validate_language(language)?;

    // `make_searchable` **reconstruit** le document à partir du rendu : avec
    // un renderer qui ne dessine rien, chaque page devient une image vide et
    // le texte d'origine est perdu — pour un résultat renvoyé en `Ok`.
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

        // 2. OCR.
        let ocr = engine.recognize(&bitmap, page_num, language, OcrMode::Searchable)?;

        // 3. Image XObject (RGB), sauf en mode modifiable : le scan est
        //    remplacé par un fond blanc pour que le texte reconnu devienne
        //    la seule couche visible.
        let keep_image = mode != OcrMode::Editable;
        let img_id = if keep_image {
            let rgb = rgba_to_rgb(&bitmap.pixels);
            let img_dict = dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Image".to_vec()),
                "Width" => Object::Integer(bitmap.width as i64),
                "Height" => Object::Integer(bitmap.height as i64),
                "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                "BitsPerComponent" => Object::Integer(8),
            };
            let img_stream = Stream::new(img_dict, rgb);
            Some(out.add_object(img_stream))
        } else {
            None
        };

        // 4. Dimensions PDF de la page d'origine.
        let (w_pt, h_pt) = source.page_size(page_num as u32);

        // 5. Flux de contenu : image + calque texte invisible.
        // Le flux est construit en octets : la couche texte utilise une
        // police WinAnsi, et un accent écrit en UTF-8 y produirait deux
        // octets, donc deux glyphes faux.
        let mut content: Vec<u8> = Vec::new();
        if keep_image {
            content.extend_from_slice(b"q\n");
            content.extend_from_slice(format!("{w_pt:.2} 0 0 {h_pt:.2} 0 0 cm\n").as_bytes());
            content.extend_from_slice(b"/Im0 Do\n");
            content.extend_from_slice(b"Q\n");
        }

        // Calque texte.
        content.extend_from_slice(b"BT\n");
        // Mode 3 : ni rempli, ni contourné → invisible. Les deux autres
        // modes écrivent le texte visible.
        content.extend_from_slice(if mode == OcrMode::Searchable {
            b"3 Tr\n".as_slice()
        } else {
            b"0 Tr\n".as_slice()
        });
        content.extend_from_slice(b"0 g\n");

        // Le facteur de conversion entre pixels bitmap et points PDF.
        // Un bitmap vide diviserait par zéro et poserait un calque texte en
        // `inf` : mieux vaut ne pas écrire de couche texte du tout.
        if bitmap.width == 0 || bitmap.height == 0 {
            return Err(Error::InvalidInput(
                "rendu de page vide : impossible de poser la couche texte".into(),
            ));
        }
        let sx = w_pt / bitmap.width as f32;
        let sy = h_pt / bitmap.height as f32;

        for word in &ocr.words {
            // Position dans le bitmap (origine haut-gauche).
            let x_pt = word.left as f32 * sx;
            // Le texte PDF a une origine bas-gauche.
            let y_pt = h_pt - (word.top as f32 + word.height as f32) * sy;

            // Taille de police : on prend la hauteur du mot moins une marge,
            // pour que la boîte de sélection corresponde visuellement.
            let size = (word.height as f32 * sy * 1.1).max(4.0);

            content.extend_from_slice(format!("/F1 {size:.2} Tf\n").as_bytes());
            content.extend_from_slice(format!("1 0 0 1 {x_pt:.2} {y_pt:.2} Tm\n").as_bytes());
            content.push(b'(');
            content.extend_from_slice(&encode_pdf_literal(&word.text));
            content.extend_from_slice(b") Tj\n");
        }

        content.extend_from_slice(b"ET\n");

        let content_stream = Stream::new(dictionary! {}, content);
        let content_id = out.add_object(content_stream);

        // 6. Dictionnaire de page.
        let mut resources = dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(helv_id),
            }),
        };
        if let Some(img_id) = img_id {
            resources.set(
                "XObject",
                Object::Dictionary(dictionary! { "Im0" => Object::Reference(img_id) }),
            );
        }
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

/// Convertit RGBA en RGB, sans le canal alpha.
fn rgba_to_rgb(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for chunk in rgba.chunks_exact(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }
    rgb
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
