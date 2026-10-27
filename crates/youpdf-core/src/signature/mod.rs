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
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use x509_cert::Certificate;

use youpdf_shared::{Error, Result};

/// Convertit les erreurs ASN.1/DER en erreur applicative.
trait DerExt<T> {
    fn der(self) -> Result<T>;
}

impl<T> DerExt<T> for std::result::Result<T, der::Error> {
    fn der(self) -> Result<T> {
        self.map_err(|e| Error::InvalidInput(format!("DER : {e}")))
    }
}

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
/// Identité de signature : certificat feuille, chaîne et clé privée RSA.
pub struct SigningIdentity {
    certificate: Certificate,
    chain: Vec<Certificate>,
    private_key: RsaPrivateKey,
}
/// Paramètres de création d'un certificat auto-signé.
#[derive(Debug, Clone)]
pub struct CertificateRequest {
    pub common_name: String,
    pub organization: String,
    pub email: String,
    pub country: String,
    pub years: u32,
}

impl Default for CertificateRequest {
    fn default() -> Self {
        Self {
            common_name: String::new(),
            organization: String::new(),
            email: String::new(),
            country: "FR".into(),
            years: 3,
        }
    }
}

/// Emplacement et métadonnées d'une signature détachée.
#[derive(Debug, Clone, Default)]
pub struct SignOptions {
    /// Page (1-indexée).
    pub page: usize,
    /// Rectangle PDF `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
    /// Nom du champ de signature.
    pub field_name: String,
    /// Nom affiché du signataire.
    pub name: String,
    /// Motif de la signature.
    pub reason: String,
    /// Lieu de signature.
    pub location: String,
    /// Coordonnées de contact.
    pub contact: String,
    /// Image PNG de la signature (facultative). Si fournie, elle est
    /// intégrée dans l'apparence visible du champ de signature.
    pub signature_image: Option<Vec<u8>>,
}

/// Résultat de la vérification d'une signature présente dans un PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub field: String,
    pub signer: String,
    pub issuer: String,
    pub signed_at: Option<String>,
    pub reason: String,
    pub location: String,
    pub valid: bool,
    pub covers_whole_document: bool,
    pub self_signed: bool,
    pub error: Option<String>,
}

mod identity;
mod sign;
mod util;
mod verify;
mod visual;

pub use verify::validate_signatures;
