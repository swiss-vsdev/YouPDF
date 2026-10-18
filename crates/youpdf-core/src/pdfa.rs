//! Conformité **déclarée** : PDF/A, PDF/X, PDF/E, PDF d'image.
//!
//! Ce module pose les marqueurs exigés par les normes (paquet XMP,
//! `/OutputIntents`, `/MarkInfo`, `/ID`) mais **ne valide pas** le fichier :
//! aucune vérification de contenu, de polices incorporées, de transparence ou
//! de structure n'est faite. Un fichier marqué ici n'est pas, de ce seul
//! fait, conforme. Un valideur externe (veraPDF, preflight) reste nécessaire.
//!
//! Lacune connue : `/OutputIntents` cite le profil **sRGB IEC61966-2.1** par
//! son nom, mais aucun flux ICC (`/DestOutputProfile`) n'est embarqué. Les
//! valideurs stricts le signalent ; embarquer les octets du profil est un
//! ajout ultérieur.

use std::path::Path;

use lopdf::{dictionary, Dictionary, Object, Stream};
use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::render::{RenderOptions, Renderer};
use crate::util::flate_compress;

/// Nom du profil colorimétrique déclaré (pas d'octets ICC embarqués).
const SRGB_PROFILE: &str = "sRGB IEC61966-2.1";

/// Niveau de conformité déclaré.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conformance {
    /// PDF/A-2b : archivage.
    PdfA2b,
    /// PDF/X-4 : échange d'impression.
    PdfX4,
    /// PDF/E-1 : ingénierie.
    PdfE1,
}

impl Conformance {
    /// Nom affiché.
    pub fn display_name(self) -> &'static str {
        match self {
            Conformance::PdfA2b => "PDF/A-2b",
            Conformance::PdfX4 => "PDF/X-4",
            Conformance::PdfE1 => "PDF/E-1",
        }
    }

    /// Identifiant de la partie de la norme, pour `/OutputIntents`.
    pub fn identifier(self) -> &'static str {
        match self {
            Conformance::PdfA2b => "PDF/A-2b",
            Conformance::PdfX4 => "PDF/X-4",
            Conformance::PdfE1 => "PDF/E-1",
        }
    }

    /// Sous-type `/S` de l'`/OutputIntent` exigé par la norme.
    fn output_intent_subtype(self) -> &'static [u8] {
        match self {
            // PDF/A-2 et PDF/E-1 déclarent un intent de sortie PDF/A.
            Conformance::PdfA2b | Conformance::PdfE1 => b"GTS_PDFA1",
            Conformance::PdfX4 => b"GTS_PDFX",
        }
    }
}

/// Identifiant de document `/ID` : 16 octets dérivés de l'horodatage courant
/// et du nombre de pages. Pas de tirage aléatoire : la valeur reste
/// reproductible entre deux exécutions à la même nanoseconde.
fn document_id(page_count: usize) -> Vec<u8> {
    use md5::{Digest, Md5};

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let mut hasher = Md5::new();
    hasher.update(nanos.to_le_bytes());
    hasher.update((page_count as u64).to_le_bytes());
    hasher.finalize().to_vec()
}

/// Paquet XMP déclarant `level`.
///
/// Les marqueurs normatifs sont posés par niveau (`pdfaid` pour PDF/A,
/// `pdfx` pour PDF/X, `pdfe` + schéma d'extension pour PDF/E). Le paquet
/// contient aussi l'identifiant lisible du niveau, pour qu'un humain (et le
/// test) retrouve la déclaration sans décoder les namespaces.
fn xmp_packet(level: Conformance) -> String {
    let specific = match level {
        Conformance::PdfA2b => concat!(
            "      <pdfaid:part>2</pdfaid:part>\n",
            "      <pdfaid:conformance>B</pdfaid:conformance>\n",
        ),
        Conformance::PdfX4 => "      <pdfx:GTS_PDFXVersion>PDF/X-4</pdfx:GTS_PDFXVersion>\n",
        Conformance::PdfE1 => "      <pdfe:ISO_PDFEVersion>PDF/E-1</pdfe:ISO_PDFEVersion>\n",
    };

    // Le schéma d'extension XMP n'est exigé que par PDF/E.
    let extension = if level == Conformance::PdfE1 {
        concat!(
            "    <rdf:Description rdf:about=\"\"\n",
            "        xmlns:pdfaExtension=\"http://www.aiim.org/pdfa/ns/extension/\"\n",
            "        xmlns:pdfaSchema=\"http://www.aiim.org/pdfa/ns/schema#\"\n",
            "        xmlns:pdfaProperty=\"http://www.aiim.org/pdfa/ns/property#\">\n",
            "      <pdfaExtension:schemas>\n",
            "        <rdf:Bag>\n",
            "          <rdf:li rdf:parseType=\"Resource\">\n",
            "            <pdfaSchema:namespaceURI>http://www.aiim.org/pdfe/ns/id/</pdfaSchema:namespaceURI>\n",
            "            <pdfaSchema:prefix>pdfe</pdfaSchema:prefix>\n",
            "            <pdfaSchema:schema>PDF/E identification schema</pdfaSchema:schema>\n",
            "            <pdfaSchema:property>\n",
            "              <rdf:Seq>\n",
            "                <rdf:li rdf:parseType=\"Resource\">\n",
            "                  <pdfaProperty:name>ISO_PDFEVersion</pdfaProperty:name>\n",
            "                  <pdfaProperty:valueType>Text</pdfaProperty:valueType>\n",
            "                  <pdfaProperty:category>internal</pdfaProperty:category>\n",
            "                  <pdfaProperty:description>Version de la norme PDF/E</pdfaProperty:description>\n",
            "                </rdf:li>\n",
            "              </rdf:Seq>\n",
            "            </pdfaSchema:property>\n",
            "          </rdf:li>\n",
            "        </rdf:Bag>\n",
            "      </pdfaExtension:schemas>\n",
            "    </rdf:Description>\n",
        )
    } else {
        ""
    };

    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
    <rdf:Description rdf:about=\"\"\n\
        xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"\n\
        xmlns:pdfx=\"http://ns.adobe.com/pdfx/1.3/\"\n\
        xmlns:pdfe=\"http://www.aiim.org/pdfe/ns/id/\"\n\
        xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
      <dc:description>\n\
        <rdf:Alt>\n\
          <rdf:li xml:lang=\"x-default\">{identifier}</rdf:li>\n\
        </rdf:Alt>\n\
      </dc:description>\n\
{specific}    </rdf:Description>\n\
{extension}  </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>\n",
        identifier = level.identifier(),
        specific = specific,
        extension = extension,
    )
}

/// Dictionnaire `/OutputIntent` déclarant `level`.
///
/// `/OutputConditionIdentifier` porte l'identifiant du niveau ; le profil
/// sRGB est cité dans `/OutputCondition` et `/Info`, sans octets ICC.
fn output_intent(level: Conformance) -> Dictionary {
    dictionary! {
        "Type" => Object::Name(b"OutputIntent".to_vec()),
        "S" => Object::Name(level.output_intent_subtype().to_vec()),
        "OutputConditionIdentifier" => Object::string_literal(level.identifier()),
        "OutputCondition" => Object::string_literal(SRGB_PROFILE),
        "RegistryName" => Object::string_literal("http://www.color.org"),
        "Info" => Object::string_literal(format!(
            "{} — {SRGB_PROFILE} (profil ICC non embarqué)",
            level.identifier()
        )),
    }
}

impl Document {
    /// Écrit une copie déclarée conforme à `level`.
    ///
    /// Le document en mémoire n'est pas modifié : les marqueurs sont posés sur
    /// une copie, comme pour la compression et le chiffrement. C'est ce que
    /// veut « Enregistrer sous ▸ PDF/A » : un fichier de sortie marqué, pas un
    /// état de travail contaminé.
    pub fn save_with_conformance(
        &self,
        path: impl AsRef<Path>,
        level: Conformance,
    ) -> Result<()> {
        let mut copie = Document::from_lopdf(self.inner.clone());
        copie.apply_conformance(level)?;
        copie.save(path)
    }

    /// Pose les marqueurs de conformité déclarés par `level`.
    ///
    /// Écrit un paquet XMP (`/Metadata`), un `/OutputIntents` (sRGB
    /// IEC61966-2.1, **sans** octets ICC), `/MarkInfo << /Marked true >>` et
    /// un `/ID` de trailer (deux valeurs identiques de 16 octets).
    ///
    /// **Déclaration seulement** : la fonction ne valide pas le document et
    /// ne garantit aucune conformité effective. Voir la note de module.
    pub fn apply_conformance(&mut self, level: Conformance) -> Result<()> {
        let page_count = self.page_count();
        let xmp = xmp_packet(level);

        let metadata_id = self.inner_mut().add_object(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"Metadata".to_vec()),
                "Subtype" => Object::Name(b"XML".to_vec()),
                "Filter" => Object::Name(b"FlateDecode".to_vec()),
            },
            flate_compress(xmp.as_bytes())?,
        ));

        let intent_id = self
            .inner_mut()
            .add_object(Object::Dictionary(output_intent(level)));

        let inner = self.inner_mut();
        {
            let catalog = inner
                .catalog_mut()
                .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
            catalog.set("Metadata", Object::Reference(metadata_id));
            catalog.set(
                "OutputIntents",
                Object::Array(vec![Object::Reference(intent_id)]),
            );
            catalog.set(
                "MarkInfo",
                Object::Dictionary(dictionary! { "Marked" => Object::Boolean(true) }),
            );
        }

        let id = document_id(page_count);
        inner.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
                Object::String(id, lopdf::StringFormat::Hexadecimal),
            ]),
        );

        Ok(())
    }

    /// Écrit un PDF dont chaque page est l'image rendue de la page d'origine.
    ///
    /// Chaque page est rendue à `dpi/72.0` puis embarquée en RGB 8 bits
    /// (`/FlateDecode`) ; la taille de page reprend la boîte visible
    /// d'origine ([`Document::page_view`]) pour que la copie coïncide.
    ///
    /// Renvoie une erreur si le renderer ne produit pas de contenu réel
    /// (`Renderer::renders_real_content`), pour ne pas écrire un PDF vide en
    /// annonçant un succès.
    pub fn save_image_pdf(&self, path: &Path, renderer: &dyn Renderer, dpi: f32) -> Result<()> {
        if !renderer.renders_real_content() {
            return Err(Error::InvalidInput(
                "rendu sans contenu réel : le PDF d'image serait vide".into(),
            ));
        }

        let total = self.page_count();
        if total == 0 {
            return Err(Error::InvalidInput("document sans page".into()));
        }

        let scale = dpi.max(1.0) / 72.0;
        let options = RenderOptions {
            scale,
            ..Default::default()
        };

        let mut out = lopdf::Document::with_version("1.4");
        let pages_id = out.new_object_id();
        let mut kids: Vec<Object> = Vec::with_capacity(total);

        for page in 1..=total as u32 {
            let view = self.page_view(page);
            let bitmap = renderer.render_page(self, page, &options)?;

            let expected = (bitmap.width as usize) * (bitmap.height as usize) * 4;
            if bitmap.width == 0 || bitmap.height == 0 || bitmap.pixels.len() < expected {
                return Err(Error::Pdf(format!("page {page} : rendu vide")));
            }

            // pdfium fournit du RGBA ; l'image PDF est déclarée RGB 8 bits.
            let rgb: Vec<u8> = bitmap
                .pixels
                .chunks_exact(4)
                .flat_map(|px| [px[0], px[1], px[2]])
                .collect();

            let image_id = out.add_object(Stream::new(
                dictionary! {
                    "Type" => Object::Name(b"XObject".to_vec()),
                    "Subtype" => Object::Name(b"Image".to_vec()),
                    "Width" => Object::Integer(bitmap.width as i64),
                    "Height" => Object::Integer(bitmap.height as i64),
                    "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                    "BitsPerComponent" => Object::Integer(8),
                    "Filter" => Object::Name(b"FlateDecode".to_vec()),
                },
                flate_compress(&rgb)?,
            ));

            // La boîte visible, rotation appliquée, est la surface rendue.
            let (width, height) = view.display_size();
            let (width, height) = (width.max(1.0), height.max(1.0));

            let content = format!("q {width:.2} 0 0 {height:.2} 0 0 cm /Im0 Do Q\n");
            let content_id =
                out.add_object(Stream::new(Dictionary::new(), content.into_bytes()));

            let page_id = out.add_object(dictionary! {
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

        out.set_object(
            pages_id,
            dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(kids),
                "Count" => Object::Integer(total as i64),
            },
        );

        let catalog_id = out.add_object(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        });
        out.trailer.set("Root", Object::Reference(catalog_id));

        let mut doc = Document::from_lopdf(out);
        doc.save(path)
    }
}
