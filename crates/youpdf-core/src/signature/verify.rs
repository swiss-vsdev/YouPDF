//! Vérification des signatures présentes dans un PDF.

use std::time::SystemTime;

use chrono::Utc;
use der::asn1::OctetString;
use der::{Decode, Encode};
use rsa::pkcs1v15::{Signature as RsaSignature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use rsa::RsaPublicKey;
use sha2::{Digest, Sha256};
use x509_cert::time::Time as CertTime;
use x509_cert::Certificate;

use youpdf_shared::{Error, Result};

use crate::signature::identity::common_name_of;
use crate::signature::sign::{OID_MESSAGE_DIGEST, OID_SIGNING_TIME};
use crate::signature::util::{pdf_string, signature_field_name};
use crate::signature::{DerExt, SignatureInfo};

/// Vérifie toutes les signatures détachées d'un PDF.
pub fn validate_signatures(bytes: &[u8]) -> Vec<SignatureInfo> {
    let Ok(document) = lopdf::Document::load_mem(bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ((id, _), object) in &document.objects {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        let is_signature = dict
            .get(b"Type")
            .ok()
            .and_then(|object| object.as_name().ok())
            .is_some_and(|name| name == b"Sig");
        if !is_signature {
            continue;
        }
        let field = signature_field_name(&document, *id).unwrap_or_default();
        let reason = pdf_string(dict.get(b"Reason").ok());
        let location = pdf_string(dict.get(b"Location").ok());
        let signer_hint = pdf_string(dict.get(b"Name").ok());

        let mut info = SignatureInfo {
            field,
            signer: signer_hint,
            issuer: String::new(),
            signed_at: None,
            reason,
            location,
            valid: false,
            covers_whole_document: false,
            self_signed: false,
            error: None,
        };

        let result = (|| -> Result<()> {
            let byte_range = dict
                .get(b"ByteRange")
                .map_err(|_| Error::Pdf("/ByteRange absent".into()))?
                .as_array()
                .map_err(|_| Error::Pdf("/ByteRange invalide".into()))?
                .iter()
                .map(|object| object.as_i64())
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| Error::Pdf("/ByteRange invalide".into()))?;
            let contents = match dict
                .get(b"Contents")
                .map_err(|_| Error::Pdf("/Contents absent".into()))?
            {
                lopdf::Object::String(bytes, _) => bytes.clone(),
                _ => return Err(Error::Pdf("/Contents invalide".into())),
            };
            if byte_range.len() != 4 {
                return Err(Error::Pdf("/ByteRange incomplet".into()));
            }
            let [a, b, c, d] = [
                byte_range[0].max(0) as usize,
                byte_range[1].max(0) as usize,
                byte_range[2].max(0) as usize,
                byte_range[3].max(0) as usize,
            ];
            if a + b > bytes.len() || c + d > bytes.len() || c < a + b {
                return Err(Error::Pdf("/ByteRange hors du fichier".into()));
            }
            let gap = 2 + contents.len() * 2;
            info.covers_whole_document =
                a == 0 && c == a + b + gap && c + d == bytes.len();
            let mut hasher = Sha256::new();
            hasher.update(&bytes[a..a + b]);
            hasher.update(&bytes[c..c + d]);
            let covered = hasher.finalize();

            let cms_end = contents.iter().rposition(|byte| *byte != 0).map_or(0, |index| index + 1);
            let (certificate, signed_at, self_signed) = verify_cms(&contents[..cms_end], &covered)?;
            info.signer = common_name_of(&certificate).unwrap_or_else(|| certificate.tbs_certificate.subject.to_string());
            info.issuer = certificate.tbs_certificate.issuer.to_string();
            info.signed_at = signed_at;
            info.self_signed = self_signed;
            let now = SystemTime::now();
            let validity = &certificate.tbs_certificate.validity;
            if now < validity.not_before.to_system_time() {
                return Err(Error::Pdf("certificat pas encore valide".into()));
            }
            if now > validity.not_after.to_system_time() {
                return Err(Error::Pdf("certificat expiré".into()));
            }
            if !info.covers_whole_document {
                return Err(Error::Pdf(
                    "la signature ne couvre pas tout le document".into(),
                ));
            }
            info.valid = true;
            Ok(())
        })();

        if let Err(error) = result {
            info.error = Some(error.to_string());
        }
        out.push(info);
    }
    out
}

/// Vérifie le PKCS#7 et renvoie le certificat, l'horodatage et l'auto-signature.
fn verify_cms(
    cms_der: &[u8],
    covered_digest: &[u8],
) -> Result<(Certificate, Option<String>, bool)> {
    use cms::content_info::ContentInfo;
    use cms::signed_data::{CertificateSet, SignedData, SignerIdentifier};
    use x509_cert::attr::AttributeValue;

    let content_info = ContentInfo::from_der(cms_der)
        .map_err(|e| Error::Pdf(format!("PKCS#7 : {e}")))?;
    let signed_data = SignedData::from_der(&content_info.content.to_der().der()?)
        .map_err(|e| Error::Pdf(format!("SignedData : {e}")))?;
    let signer = signed_data
        .signer_infos
        .0
        .iter()
        .next()
        .ok_or_else(|| Error::Pdf("PKCS#7 sans signataire".into()))?;
    let signed_attrs = signer
        .signed_attrs
        .as_ref()
        .ok_or_else(|| Error::Pdf("attributs signés absents".into()))?;

    let certificates: &CertificateSet = signed_data
        .certificates
        .as_ref()
        .ok_or_else(|| Error::Pdf("certificat absent du PKCS#7".into()))?;
    let certificate = certificates
        .0
        .iter()
        .filter_map(|choice| match choice {
            cms::cert::CertificateChoices::Certificate(certificate) => Some(certificate),
            _ => None,
        })
        .find(|certificate| match &signer.sid {
            SignerIdentifier::IssuerAndSerialNumber(identity) => {
                certificate.tbs_certificate.issuer == identity.issuer
                    && certificate.tbs_certificate.serial_number == identity.serial_number
            }
            SignerIdentifier::SubjectKeyIdentifier(_) => true,
        })
        .ok_or_else(|| Error::Pdf("certificat du signataire introuvable".into()))?;

    let signed_der = signed_attrs
        .to_der()
        .map_err(|e| Error::Pdf(format!("attributs : {e}")))?;
    let signature = RsaSignature::try_from(signer.signature.as_bytes())
        .map_err(|e| Error::Pdf(format!("signature RSA : {e}")))?;
    let public_key = RsaPublicKey::from_public_key_der(
        &certificate
            .tbs_certificate
            .subject_public_key_info
            .to_der()
            .der()?,
    )
    .map_err(|e| Error::Pdf(format!("clé publique : {e}")))?;
    VerifyingKey::<Sha256>::new(public_key)
        .verify(&signed_der, &signature)
        .map_err(|_| Error::Pdf("signature cryptographique invalide".into()))?;

    let digest_attribute = signed_attrs
        .iter()
        .find(|attribute| attribute.oid == OID_MESSAGE_DIGEST)
        .ok_or_else(|| Error::Pdf("messageDigest absent".into()))?;
    let digest_value: &AttributeValue = digest_attribute
        .values
        .get(0)
        .ok_or_else(|| Error::Pdf("messageDigest vide".into()))?;
    let expected = OctetString::from_der(&digest_value.to_der().der()?)
        .map_err(|e| Error::Pdf(format!("messageDigest : {e}")))?;
    if expected.as_bytes() != covered_digest {
        return Err(Error::Pdf(
            "le document a été modifié après la signature".into(),
        ));
    }

    let signed_at = signed_attrs
        .iter()
        .find(|attribute| attribute.oid == OID_SIGNING_TIME)
        .and_then(|attribute| attribute.values.get(0))
        .and_then(|value| CertTime::from_der(&value.to_der().ok()?).ok())
        .map(|time| {
            chrono::DateTime::<Utc>::from_timestamp(
                time.to_unix_duration().as_secs() as i64,
                0,
            )
            .map(|date| date.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_default()
        });

    let self_signed =
        certificate.tbs_certificate.issuer == certificate.tbs_certificate.subject;
    Ok((certificate.clone(), signed_at, self_signed))
}
