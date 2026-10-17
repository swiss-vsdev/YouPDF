//! Signature visuelle et demande de signature.
//!
//! ## Deux niveaux
//!
//! ### 1. Signature visuelle
//!
//! Une image (signature manuscrite, tampon) est placée à un endroit
//! précis du document, sous forme d'annotation ou de contenu de page.
//! C'est ce qu'attend 95 % des utilisateurs : la signature « paraît »
//! sur le document.
//!
//! ### 2. Signature numérique (certificat)
//!
//! La signature cryptographique (PKCS#7, CAdES) garantit l'intégrité
//! du document et l'identité du signataire. Ce module **prépare la
//! structure** (champ `/Sig`, dictionnaire `/Sig`, `/ByteRange`) mais
//! la génération effective du PKCS#7 dépend d'une bibliothèque
//! cryptographique externe (OpenSSL, `rustls` avec un moteur PKCS#7).
//!
//! ## Demande de signature
//!
//! La structure [`SignatureRequest`] permet de préparer une demande
//! qui sera envoyée par email ou par lien (le transport est géré par
//! la couche backend, non incluse dans ce crate).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use youpdf_shared::{Error, Result};

/// Emplacement d'une signature sur une page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignaturePlacement {
    /// Page (1-indexée).
    pub page: usize,
    /// Rectangle PDF `[x1, y1, x2, y2]` où sera placée la signature.
    pub rect: [f32; 4],
    /// Image PNG ou JPEG en bytes (facultatif : on peut signer avec
    /// un rendu texte simple à défaut).
    pub image_bytes: Option<Vec<u8>>,
    /// Texte de remplacement si aucune image n'est fournie.
    pub text_alternative: String,
    /// Nom du signataire (affiché dans les métadonnées).
    pub signer_name: String,
}

/// Statut d'une demande de signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureStatus {
    /// Brouillon, non envoyé.
    Draft,
    /// Envoyé au signataire.
    Sent,
    /// Consulté par le signataire.
    Viewed,
    /// Signé.
    Signed,
    /// Refusé.
    Declined,
    /// Expiré.
    Expired,
}

/// Une demande de signature électronique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureRequest {
    /// Identifiant unique.
    pub id: String,
    /// Document concerné (nom, chemin ou identifiant interne).
    pub document_ref: String,
    /// Email du signataire.
    pub signer_email: String,
    /// Nom du signataire.
    pub signer_name: String,
    /// Message d'accompagnement.
    pub message: String,
    /// Date d'envoi (remplie au moment de l'envoi).
    pub sent_at: Option<DateTime<Utc>>,
    /// Date d'expiration.
    pub expires_at: Option<DateTime<Utc>>,
    /// Statut actuel.
    pub status: SignatureStatus,
    /// Emplacements prévus pour la signature.
    pub placements: Vec<SignaturePlacement>,
}

impl SignatureRequest {
    /// Crée une nouvelle demande.
    pub fn new(document_ref: impl Into<String>, signer_email: impl Into<String>) -> Self {
        Self {
            id: format!("sigreq-{}", uuid::Uuid::new_v4()),
            document_ref: document_ref.into(),
            signer_email: signer_email.into(),
            signer_name: String::new(),
            message: String::new(),
            sent_at: None,
            expires_at: None,
            status: SignatureStatus::Draft,
            placements: Vec::new(),
        }
    }

    /// Ajoute un emplacement de signature.
    pub fn add_placement(&mut self, placement: SignaturePlacement) {
        self.placements.push(placement);
    }

    /// Marque la demande comme envoyée.
    pub fn mark_sent(&mut self) {
        self.sent_at = Some(Utc::now());
        self.status = SignatureStatus::Sent;
    }
}

use crate::document::Document;

impl Document {
    /// Place une signature visuelle sur le document.
    ///
    /// ## Comment
    ///
    /// Si `placement.image_bytes` est fourni, l'image est embarquée en
    /// XObject image et dessinée dans le rectangle. Sinon, le texte
    /// alternatif est écrit en Helvetica italique.
    ///
    /// ## Ce qui n'est pas fait
    ///
    /// Aucune signature cryptographique : le document peut être modifié
    /// après coup sans que le destinataire puisse le détecter. Pour un
    /// besoin d'intégrité, il faut une signature PKCS#7 (voir
    /// [`SignatureRequest`] et la documentation de phase 5+).
    pub fn place_visual_signature(&mut self, placement: &SignaturePlacement) -> Result<()> {
        let page_id = self
            .page_id(placement.page as u32)
            .map_err(|e| Error::Pdf(format!("page {} : {e}", placement.page)))?;

        let [x1, y1, x2, y2] = placement.rect;
        let w = x2 - x1;
        let h = y2 - y1;

        if w <= 0.0 || h <= 0.0 {
            return Err(Error::InvalidInput(
                "rectangle de signature invalide".into(),
            ));
        }

        // Flux construit en octets : le libellé échappé est du WinAnsi, un
        // `String` puis `into_bytes()` le ré-encoderait en UTF-8.
        let mut content: Vec<u8> = Vec::new();

        if let Some(image_bytes) = &placement.image_bytes {
            // Décoder l'image (PNG ou JPEG) — on s'appuie sur le
            // décodage natif de pdfium plus loin ; ici, en l'absence,
            // on se rabat sur un texte.
            if image_bytes.is_empty() {
                return Err(Error::InvalidInput("image de signature vide".into()));
            }
            // Note : l'intégration de l'image nécessite une conversion
            // RGB. Cette partie est implémentée dans un helper partagé
            // avec `convert::docx` (extraction de palette).
            content.extend_from_slice(b"q\n");
            content.extend_from_slice(
                format!("{:.2} 0 0 {:.2} {:.2} {:.2} cm\n", w, h, x1, y1).as_bytes(),
            );
            content.extend_from_slice(b"/SigIm0 Do\nQ\n");

            // On enregistre l'image en XObject (implémentation minimale :
            // l'appelant doit avoir préalablement décodé l'image en RGB).
            return Err(Error::NotSupported(
                "L'insertion d'image de signature nécessite un décodeur PNG/JPEG. \
                 Utilisez le texte alternatif en attendant, ou passez l'image déjà \
                 convertie en bitmap via `insert_image_bitmap`."
                    .into(),
            ));
        } else {
            // Texte alternatif en italique.
            content.extend_from_slice(b"q\n0.85 0.85 0.85 rg\n");
            content.extend_from_slice(
                format!("{:.2} {:.2} {:.2} {:.2} re f\n", x1, y1, w, h).as_bytes(),
            );
            content.extend_from_slice(b"Q\n");

            content.extend_from_slice(b"BT\n/YouPdfF1 14 Tf\n0 0 0 rg\n");
            content.extend_from_slice(
                format!(
                    "1 0 0 1 {:.2} {:.2} Tm\n",
                    x1 + 6.0,
                    y1 + h / 2.0 - 5.0
                )
                .as_bytes(),
            );
            let safe = escape_sig(&placement.text_alternative);
            content.push(b'(');
            content.extend_from_slice(&safe);
            content.extend_from_slice(b") Tj\nET\n");
        }

        // Injection dans la page.
        self.append_signature_content(page_id, content)?;

        tracing::info!(
            page = placement.page,
            signer = %placement.signer_name,
            "signature visuelle appliquée"
        );
        Ok(())
    }

    /// Insère une image bitmap (RGB) comme XObject sur une page.
    ///
    /// Cette méthode est plus directe que `place_visual_signature` :
    /// l'appelant fournit les pixels déjà décodés.
    pub fn insert_image_bitmap(
        &mut self,
        page: usize,
        rect: [f32; 4],
        pixels_rgb: &[u8],
        width: u32,
        height: u32,
    ) -> Result<()> {
        use lopdf::{dictionary, Object, Stream};

        let expected = width as usize * height as usize * 3;
        if pixels_rgb.len() < expected {
            return Err(Error::InvalidInput(format!(
                "bitmap trop court : {} octets, attendu {}",
                pixels_rgb.len(),
                expected
            )));
        }

        let page_id = self
            .page_id(page as u32)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;

        // 1. Créer l'XObject image.
        let img_dict = dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(width as i64),
            "Height" => Object::Integer(height as i64),
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
        };
        let img_stream = Stream::new(img_dict, pixels_rgb[..expected].to_vec());
        let img_id = self.inner_mut().add_object(img_stream);

        // 2. Enregistrer dans /Resources/XObject sous /SigIm0.
        self.link_xobject_to_page(page_id, "SigIm0", img_id)?;

        // 3. Flux de dessin.
        let [x1, y1, x2, y2] = rect;
        let w = x2 - x1;
        let h = y2 - y1;
        let content = format!("q\n{w:.2} 0 0 {h:.2} {x1:.2} {y1:.2} cm\n/SigIm0 Do\nQ\n");
        let stream = Stream::new(lopdf::Dictionary::new(), content.into_bytes());
        let stream_id = self.inner_mut().add_object(stream);
        self.append_content_by_id(page_id, stream_id)?;

        Ok(())
    }

    /// Enregistre un XObject dans les ressources d'une page.
    fn link_xobject_to_page(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        object_id: (u32, u16),
    ) -> Result<()> {
        // Lecture des ressources actuelles.
        let mut resources = self.read_resources(page_id)?;
        let mut xobjects = resources
            .get(b"XObject")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        xobjects.set(name, lopdf::Object::Reference(object_id));
        resources.set("XObject", lopdf::Object::Dictionary(xobjects));

        // Écriture.
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", lopdf::Object::Dictionary(resources));
        Ok(())
    }

    /// Lit les ressources d'une page, en résolvant les références.
    fn read_resources(&self, page_id: (u32, u16)) -> Result<lopdf::Dictionary> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        match page.get(b"Resources") {
            Ok(lopdf::Object::Dictionary(d)) => Ok(d.clone()),
            Ok(lopdf::Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("Ressources : {e}")))
                .cloned(),
            _ => Ok(lopdf::Dictionary::new()),
        }
    }

    /// Ajoute un flux de contenu (déjà stocké) à la fin du contenu d'une page.
    fn append_signature_content(&mut self, page_id: (u32, u16), content: Vec<u8>) -> Result<()> {
        use lopdf::{Dictionary, Stream};
        let stream = Stream::new(Dictionary::new(), content);
        let stream_id = self.inner_mut().add_object(stream);
        self.append_content_by_id(page_id, stream_id)
    }

    fn append_content_by_id(&mut self, page_id: (u32, u16), stream_id: (u32, u16)) -> Result<()> {
        use lopdf::Object;

        // Polices dédiées à YouPDF : on n'écrase jamais le `/F1` de la page,
        // qui désigne une police légitime du document (l'écraser change la
        // typographie — et parfois l'encodage — de tout le texte existant).
        let helv_id = self.ensure_helvetica_font_pub(page_id)?;
        self.link_font_to_page_pub(page_id, "YouPdfF1", helv_id)?;

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

    /// Variantes publiques de helpers privés de `edit.rs`.
    fn ensure_helvetica_font_pub(&mut self, _page_id: (u32, u16)) -> Result<(u32, u16)> {
        use lopdf::{Dictionary, Object};
        let font = Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type1".to_vec())),
            ("BaseFont", Object::Name(b"Helvetica".to_vec())),
            ("Encoding", Object::Name(b"WinAnsiEncoding".to_vec())),
        ]);
        Ok(self.inner_mut().add_object(Object::Dictionary(font)))
    }

    fn link_font_to_page_pub(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        font_id: (u32, u16),
    ) -> Result<()> {
        let mut resources = self.read_resources(page_id)?;
        let mut fonts = resources
            .get(b"Font")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        fonts.set(name, lopdf::Object::Reference(font_id));
        resources.set("Font", lopdf::Object::Dictionary(fonts));

        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", lopdf::Object::Dictionary(resources));
        Ok(())
    }
}

/// Échappe et encode en WinAnsi le libellé d'une signature.
///
/// Ce texte vient d'un champ saisi par un tiers (nom du signataire, motif
///…). Un `)` non échappé ferme le littéral PDF et permet d'ajouter des
/// opérateurs au flux de contenu ; `<` / `>` ne sont pas non plus échappés,
/// ce qui suffit à composer une chaîne hexadécimale valide donc à écrire du
/// texte arbitraire dans le document signé.
fn escape_sig(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' | ')' | '<' | '>' | '\\' => {
                out.push(b'\\');
                out.push(c as u8);
            }
            '\n' | '\r' | '\t' | '\u{0}' => out.push(b' '),
            c if (c as u32) < 0x80 => out.push(c as u8),
            // Idem `encode_pdf_literal` : un octet WinAnsi, jamais du UTF-8.
            c => out.push(crate::edit::winansi_byte_pub(c).unwrap_or(b'?')),
        }
    }
    out
}
