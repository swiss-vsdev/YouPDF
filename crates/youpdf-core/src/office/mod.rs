//! Export PDF → Word (`.docx`), Excel (`.xlsx`) et PowerPoint (`.pptx`).
//!
//! Les parties OOXML (Office Open XML) sont écrites à la main puis
//! empaquetées avec la crate `zip`. Aucune dépendance `docx-rs` : chaque
//! format reste autonome et la surface de dépendances est minimale.
//!
//! ## Ce que l'export transporte
//!
//! Deux mises en page sont proposées, choisies par [`OfficeLayout`] :
//!
//! - **Flow** : le texte est reconstruit en paragraphes éditables. Word
//!   regroupe les lignes en paragraphes et applique les styles de titre,
//!   Excel pose une ligne de tableau par ligne de texte, PowerPoint empile
//!   les paragraphes dans des zones de texte.
//! - **Exact** : chaque fragment garde sa position mesurée sur la page.
//!   Word écrit des cadres ancrés sur la page, Excel reconstruit un tableau
//!   à partir de l'alignement des fragments, PowerPoint pose un objet par
//!   fragment.
//!
//! Les images sont toujours embarquées (`word/media/`, `ppt/media/`) ; en
//! mode Exact, elles sont replacées à leur position d'origine.

use std::io::Write;
use std::path::Path;

use lopdf::{Dictionary, Object};
use youpdf_shared::{Error, Result};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::document::Document;
use crate::util::escape_xml;

mod common;
mod excel;
mod powerpoint;
mod word;

pub use common::hex_color;
pub(crate) use common::{group_paragraphs, office_font, sanitize_xml, scale_to_i64, write_zip_with_media};
pub(super) use common::{body_size, group_lines, root_rels};
pub(super) use excel::{excel_parts_exact, excel_parts_flow};
pub(super) use powerpoint::powerpoint_parts;
pub(super) use word::{word_parts_exact, word_parts_flow};


/// Format Office proposé à l'export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeFormat {
    /// Word (`.docx`).
    Word,
    /// Excel (`.xlsx`).
    Excel,
    /// PowerPoint (`.pptx`).
    PowerPoint,
}

impl OfficeFormat {
    /// Nom affiché dans l'interface.
    pub fn display_name(&self) -> &'static str {
        match self {
            OfficeFormat::Word => "Word (.docx)",
            OfficeFormat::Excel => "Excel (.xlsx)",
            OfficeFormat::PowerPoint => "PowerPoint (.pptx)",
        }
    }

    /// Extension de fichier suggérée (sans point).
    pub fn extension(&self) -> &'static str {
        match self {
            OfficeFormat::Word => "docx",
            OfficeFormat::Excel => "xlsx",
            OfficeFormat::PowerPoint => "pptx",
        }
    }

    /// Tous les formats, dans l'ordre d'affichage.
    pub const ALL: [OfficeFormat; 3] = [
        OfficeFormat::Word,
        OfficeFormat::Excel,
        OfficeFormat::PowerPoint,
    ];
}

/// Mode de reconstruction de la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OfficeLayout {
    /// Paragraphes fluides, reconstruits à partir des fragments.
    #[default]
    Flow,
    /// Chaque fragment est placé à ses coordonnées PDF mesurées.
    Exact,
}

/// Une partie du paquet OOXML : chemin dans l'archive, contenu XML.
pub type Part = (String, String);


/// Un fragment de texte, avec son style résolu en nom de police Office.
#[derive(Debug, Clone)]
pub(super) struct OfficeRun {
    /// Texte du fragment, nettoyé des caractères interdits en XML.
    text: String,
    /// Boîte `[x1, y1, x2, y2]` en coordonnées PDF (origine en bas à gauche).
    rect: [f32; 4],
    /// Corps en points.
    font_size: f32,
    /// Couleur RGB.
    color: [u8; 3],
    /// Famille de police Office.
    font: &'static str,
    /// Gras.
    bold: bool,
    /// Italique.
    italic: bool,
}

/// Une image embarquée, avec sa boîte sur la page quand elle est connue.
pub(super) struct OfficeImage {
    /// Octets PNG.
    png: Vec<u8>,
    /// Largeur en pixels.
    width: u32,
    /// Hauteur en pixels.
    height: u32,
    /// Boîte `[x1, y1, x2, y2]` sur la page, si la matrice est connue.
    rect: Option<[f32; 4]>,
}

/// Une page reconstruite : sa taille, son texte et ses images.
pub(super) struct OfficePage {
    /// Largeur en points.
    width: f32,
    /// Hauteur en points.
    height: f32,
    /// Fragments de texte, dans l'ordre de lecture.
    runs: Vec<OfficeRun>,
    /// Images de la page.
    images: Vec<OfficeImage>,
}

/// Une ligne de texte : fragments alignés sur une même base.
#[derive(Clone)]
pub(super) struct OfficeLine {
    /// Fragments de la ligne, de gauche à droite.
    runs: Vec<OfficeRun>,
    /// Bord supérieur de la ligne (coordonnées PDF).
    top: f32,
    /// Bord inférieur de la ligne (coordonnées PDF).
    bottom: f32,
    /// Corps le plus grand de la ligne.
    size: f32,
}

/// Un paragraphe : des lignes consécutives partageant le même style.
pub(super) struct OfficeParagraph {
    /// Lignes du paragraphe, de haut en bas.
    lines: Vec<OfficeLine>,
    /// Niveau de titre (1, 2 ou 3) si le paragraphe est un titre.
    heading: Option<u8>,
}

impl Document {
    /// Exporte la couche texte du document vers un fichier Office.
    ///
    /// Les pages sans texte extractible (scan non océrisé) produisent une
    /// section vide plutôt qu'une erreur : un document mixte s'exporte
    /// quand même.
    pub fn export_office(&self, path: impl AsRef<Path>, format: OfficeFormat) -> Result<()> {
        self.export_office_layout(path, format, OfficeLayout::Flow)
    }

    /// Exporte vers Office en choisissant la mise en page.
    ///
    /// `Exact` repositionne chaque fragment à ses coordonnées mesurées ;
    /// `Flow` reconstruit un document éditable. Les deux modes transportent
    /// les images de la page.
    pub fn export_office_layout(
        &self,
        path: impl AsRef<Path>,
        format: OfficeFormat,
        layout: OfficeLayout,
    ) -> Result<()> {
        let path = path.as_ref();
        let (parts, media) = match format {
            OfficeFormat::Word => match layout {
                OfficeLayout::Flow => word_parts_flow(&self.office_model(), &self.office_text_lines()),
                OfficeLayout::Exact => word_parts_exact(&self.office_model()),
            },
            OfficeFormat::Excel => match layout {
                OfficeLayout::Flow => excel_parts_flow(&self.office_text_lines()),
                OfficeLayout::Exact => excel_parts_exact(&self.office_model()),
            },
            OfficeFormat::PowerPoint => powerpoint_parts(&self.office_model(), layout),
        };
        write_zip_with_media(path, &parts, &media)
    }

    /// Reconstitue chaque page : fragments stylés, images et taille.
    ///
    /// Le décodeur interne ne connaît pas les tables de largeur : la boîte
    /// d'un fragment reste indicative. Elle suffit à un placement « au plus
    /// proche », pas à une reproduction point près.
    fn office_model(&self) -> Vec<OfficePage> {
        let total = self.page_count() as u32;
        let mut pages = Vec::with_capacity(total as usize);
        for number in 1..=total {
            let (width, height) = self.page_size(number);
            let mut runs = Vec::new();
            if let Ok(fragments) = self.extract_text_fragments(number) {
                for fragment in fragments {
                    let text = sanitize_xml(fragment.text.trim());
                    if text.is_empty() {
                        continue;
                    }
                    let (font, bold, italic) =
                        self.fragment_font(number, fragment.font.as_deref());
                    runs.push(OfficeRun {
                        text,
                        rect: fragment.rect,
                        font_size: fragment.font_size.max(1.0),
                        color: fragment.color,
                        font,
                        bold,
                        italic,
                    });
                }
            }
            // Ordre de lecture : du haut vers le bas, puis de gauche à droite.
            runs.sort_by(|a, b| {
                b.rect[3]
                    .partial_cmp(&a.rect[3])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(
                        a.rect[0]
                            .partial_cmp(&b.rect[0])
                            .unwrap_or(std::cmp::Ordering::Equal),
                    )
            });

            let mut images = Vec::new();
            for info in self.extract_images(number).unwrap_or_default() {
                let Ok(png) = self.extract_image_png(&info) else {
                    continue;
                };
                images.push(OfficeImage {
                    png,
                    width: info.width,
                    height: info.height,
                    rect: self.image_rect(&info).ok().flatten(),
                });
            }

            pages.push(OfficePage {
                width,
                height,
                runs,
                images,
            });
        }
        pages
    }

    /// Lignes de texte brutes par page, pour les mises en page fluides.
    fn office_text_lines(&self) -> Vec<Vec<String>> {
        let total = self.page_count() as u32;
        let mut pages = Vec::with_capacity(total as usize);
        for number in 1..=total {
            let text = self.page_text(number).unwrap_or_default();
            let text = text.replace('\u{c}', "\n");
            pages.push(text.split('\n').map(sanitize_xml).collect());
        }
        pages
    }

    /// Dictionnaire `/Font` des ressources d'une page.
    fn page_font_dict(&self, page: u32) -> Option<Dictionary> {
        let page_id = self.page_id(page).ok()?;
        let resources = self.page_resources(page_id).ok()?;
        match resources.get(b"Font") {
            Ok(Object::Dictionary(dict)) => Some(dict.clone()),
            Ok(Object::Reference(id)) => self.inner().get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }

    /// Nom de base (`/BaseFont`) de la police de ressource donnée.
    fn base_font_name(&self, page: u32, resource: &str) -> Option<String> {
        let fonts = self.page_font_dict(page)?;
        let object = fonts.get(resource.as_bytes()).ok()?;
        let id = object.as_reference().ok()?;
        let dict = self.inner().get_dictionary(id).ok()?;
        let base = dict.get(b"BaseFont").ok()?.as_name().ok()?;
        Some(String::from_utf8_lossy(base).into_owned())
    }

    /// Police Office correspondant à une police PDF, avec son gras et son
    /// italique.
    fn fragment_font(&self, page: u32, resource: Option<&str>) -> (&'static str, bool, bool) {
        let base = resource
            .and_then(|name| self.base_font_name(page, name))
            .unwrap_or_default();
        office_font(&base)
    }
}





// ---------------------------------------------------------------------------
// Reconstruction du texte : lignes, paragraphes, titres
// ---------------------------------------------------------------------------




// ---------------------------------------------------------------------------
// Word (.docx)
// ---------------------------------------------------------------------------















// ---------------------------------------------------------------------------
// Excel (.xlsx)
// ---------------------------------------------------------------------------





















// ---------------------------------------------------------------------------
// PowerPoint (.pptx)
// ---------------------------------------------------------------------------


















// ---------------------------------------------------------------------------
// Communs
// ---------------------------------------------------------------------------
