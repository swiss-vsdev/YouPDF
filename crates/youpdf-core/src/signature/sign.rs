//! Apposition d'une signature PKCS#7 détachée (RSA + SHA-256) par incrément.

use std::time::SystemTime;

use const_oid::ObjectIdentifier;
use der::asn1::SetOfVec;
use der::{Any, Encode};
use rsa::pkcs1v15::{Signature as RsaSignature, SigningKey};
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::signature::util::{
    find_subslice, hex_encode, last_startxref, literal_string, object_bytes, pdf_date_now,
    pdf_date_short, resolve_array, resolve_dict, signature_appearance_content,
};
use crate::signature::{SignOptions, SigningIdentity};

// ---------------------------------------------------------------------------
// Signature numérique (PKCS#7 détachée, RSA + SHA-256)
// ---------------------------------------------------------------------------

/// Capacité réservée au PKCS#7 dans `/Contents`, en octets DER.
const SIGNATURE_CAPACITY: usize = 8192;

/// Emplacement fixe de `/ByteRange`, remplacé après calcul du hachage.
const BYTE_RANGE_PLACEHOLDER: &[u8] = b"[0 0000000000 0000000000 0000000000]";

pub(super) const OID_CERT_BAG: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.3");
pub(super) const OID_KEY_BAG: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.1");
pub(super) const OID_SHROUDED_KEY_BAG: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.2");
pub(super) const OID_X509_CERT: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.22.1");
pub(super) const OID_SIGNING_TIME: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
pub(super) const OID_MESSAGE_DIGEST: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const OID_SHA_256: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
impl Document {
    /// Appose une signature PKCS#7 détachée (RSA SHA-256) sur le document.
    ///
    /// Le document courant est d'abord sérialisé, puis un incrément PDF est
    /// ajouté : dictionnaire `/Sig`, widget, apparence, `/AcroForm`, page et
    /// catalogue. Les octets hors `/Contents` sont hachés, signés, puis le
    /// PKCS#7 est incrusté dans l'emplacement réservé.
    pub fn sign(&self, identity: &SigningIdentity, options: &SignOptions) -> Result<Vec<u8>> {
        if options.page == 0 {
            return Err(Error::InvalidInput("page de signature invalide".into()));
        }
        let base = self.to_bytes()?;
        let page_id = self.page_id(options.page as u32)?;
        let page_dict = self
            .inner()
            .get_object(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?
            .as_dict()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?
            .clone();
        let root_id = self
            .inner()
            .trailer
            .get(b"Root")
            .and_then(lopdf::Object::as_reference)
            .map_err(|_| Error::Pdf("catalogue absent".into()))?;
        let catalog = self
            .inner()
            .get_object(root_id)
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?
            .as_dict()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?
            .clone();

        let mut next_id = self.inner().max_id + 1;
        let sig_id = next_id;
        next_id += 1;
        let widget_id = next_id;
        next_id += 1;
        let appearance_id = next_id;
        next_id += 1;
        let font_id = next_id;
        next_id += 1;
        let acroform_id = next_id;
        next_id += 1;
        let reference = |id: u32| lopdf::Object::Reference((id, 0));

        let [x1, y1, x2, y2] = options.rect;
        let (x1, x2) = (x1.min(x2), x1.max(x2));
        let (y1, y2) = (y1.min(y2), y1.max(y2));
        let width = (x2 - x1).max(24.0);
        let height = (y2 - y1).max(20.0);
        let rect = vec![
            lopdf::Object::Real(x1),
            lopdf::Object::Real(y1),
            lopdf::Object::Real(x1 + width),
            lopdf::Object::Real(y1 + height),
        ];

        // Dictionnaire /Sig, écrit à la main pour garder /ByteRange de largeur fixe.
        let mut sig_body = Vec::new();
        sig_body.extend_from_slice(
            b"<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange ",
        );
        sig_body.extend_from_slice(BYTE_RANGE_PLACEHOLDER);
        sig_body.extend_from_slice(b" /Contents <");
        sig_body.extend(std::iter::repeat(b'0').take(SIGNATURE_CAPACITY * 2));
        sig_body.extend_from_slice(b">");
        for (key, value) in [
            ("Name", options.name.as_str()),
            ("Reason", options.reason.as_str()),
            ("Location", options.location.as_str()),
            ("ContactInfo", options.contact.as_str()),
        ] {
            if !value.trim().is_empty() {
                sig_body.extend_from_slice(b" /");
                sig_body.extend_from_slice(key.as_bytes());
                sig_body.push(b' ');
                sig_body.extend(literal_string(value));
            }
        }
        sig_body.extend_from_slice(b" /M ");
        sig_body.extend(literal_string(&pdf_date_now()));
        sig_body.extend_from_slice(b" >>");

        // Apparence visible (nom, motif, date).
        let font_dict = lopdf::Dictionary::from_iter(vec![
            ("Type", lopdf::Object::Name(b"Font".to_vec())),
            ("Subtype", lopdf::Object::Name(b"Type1".to_vec())),
            ("BaseFont", lopdf::Object::Name(b"Helvetica".to_vec())),
            ("Encoding", lopdf::Object::Name(b"WinAnsiEncoding".to_vec())),
        ]);
        let mut font_res = lopdf::Dictionary::new();
        font_res.set("Helv", reference(font_id));
        let mut resources = lopdf::Dictionary::new();
        resources.set("Font", lopdf::Object::Dictionary(font_res));
        let mut ap_dict = lopdf::Dictionary::new();
        ap_dict.set("Type", lopdf::Object::Name(b"XObject".to_vec()));
        ap_dict.set("Subtype", lopdf::Object::Name(b"Form".to_vec()));
        ap_dict.set(
            "BBox",
            lopdf::Object::Array(vec![
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(width),
                lopdf::Object::Real(height),
            ]),
        );
        ap_dict.set("Resources", lopdf::Object::Dictionary(resources));
        let ap_content = signature_appearance_content(
            width as f64,
            height as f64,
            &options.name,
            &options.reason,
            &pdf_date_short(),
        );
        let appearance = lopdf::Stream::new(ap_dict, ap_content);

        let mut widget = lopdf::Dictionary::new();
        widget.set("Type", lopdf::Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", lopdf::Object::Name(b"Widget".to_vec()));
        widget.set("FT", lopdf::Object::Name(b"Sig".to_vec()));
        widget.set(
            "T",
            lopdf::Object::string_literal(options.field_name.as_bytes().to_vec()),
        );
        widget.set("Rect", lopdf::Object::Array(rect));
        widget.set("P", reference(page_id.0));
        widget.set("V", reference(sig_id));
        widget.set("F", lopdf::Object::Integer(132));
        let mut ap = lopdf::Dictionary::new();
        ap.set("N", reference(appearance_id));
        widget.set("AP", lopdf::Object::Dictionary(ap));

        // /AcroForm : conserve les champs existants, ajoute le widget.
        let mut acroform = catalog
            .get(b"AcroForm")
            .ok()
            .and_then(|object| resolve_dict(self.inner(), object))
            .unwrap_or_default();
        let mut fields = acroform
            .get(b"Fields")
            .ok()
            .and_then(|object| resolve_array(self.inner(), object))
            .unwrap_or_default();
        fields.push(reference(widget_id));
        acroform.set("Fields", lopdf::Object::Array(fields));
        acroform.set("SigFlags", lopdf::Object::Integer(3));

        let mut page_dict = page_dict;
        let mut annots = page_dict
            .get(b"Annots")
            .ok()
            .and_then(|object| resolve_array(self.inner(), object))
            .unwrap_or_default();
        annots.push(reference(widget_id));
        page_dict.set("Annots", lopdf::Object::Array(annots));

        let mut catalog = catalog;
        catalog.set("AcroForm", reference(acroform_id));

        let previous_xref = last_startxref(&base)
            .ok_or_else(|| Error::Pdf("startxref introuvable dans la base".into()))?;

        let mut file = base;
        if !file.ends_with(b"\n") {
            file.push(b'\n');
        }
        let base_len = file.len();

        let mut update = Vec::new();
        let mut offsets = Vec::new();
        let mut push = |id: u32, body: &[u8], update: &mut Vec<u8>| {
            let offset = update.len();
            update.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
            update.extend_from_slice(body);
            if !body.ends_with(b"\n") {
                update.push(b'\n');
            }
            update.extend_from_slice(b"endobj\n");
            offsets.push((id, offset));
        };

        push(sig_id, &sig_body, &mut update);
        let widget_body = object_bytes(&lopdf::Object::Dictionary(widget))?;
        push(widget_id, &widget_body, &mut update);
        let appearance_body = object_bytes(&lopdf::Object::Stream(appearance))?;
        push(appearance_id, &appearance_body, &mut update);
        let font_body = object_bytes(&lopdf::Object::Dictionary(font_dict))?;
        push(font_id, &font_body, &mut update);
        let acroform_body = object_bytes(&lopdf::Object::Dictionary(acroform))?;
        push(acroform_id, &acroform_body, &mut update);
        let page_body = object_bytes(&lopdf::Object::Dictionary(page_dict))?;
        push(page_id.0, &page_body, &mut update);
        let catalog_body = object_bytes(&lopdf::Object::Dictionary(catalog))?;
        push(root_id.0, &catalog_body, &mut update);

        let xref_start = base_len + update.len();
        update.extend_from_slice(b"xref\n");
        offsets.sort_unstable_by_key(|(id, _)| *id);
        for (id, offset) in &offsets {
            update.extend_from_slice(format!("{id} 1\n{:010} 00000 n \n", base_len + offset).as_bytes());
        }
        let mut trailer = lopdf::Dictionary::new();
        trailer.set("Size", lopdf::Object::Integer(next_id as i64));
        trailer.set("Root", reference(root_id.0));
        trailer.set("Prev", lopdf::Object::Integer(previous_xref as i64));
        if let Ok(id) = self.inner().trailer.get(b"ID") {
            trailer.set("ID", id.clone());
        }
        let trailer_body = object_bytes(&lopdf::Object::Dictionary(trailer))?;
        update.extend_from_slice(b"trailer\n");
        update.extend_from_slice(&trailer_body);
        update.extend_from_slice(
            format!("\nstartxref\n{xref_start}\n%%EOF\n").as_bytes(),
        );

        file.extend_from_slice(&update);
        let total_len = file.len();

        let marker = format!("<{}>", "0".repeat(SIGNATURE_CAPACITY * 2));
        let contents_start = find_subslice(&file[base_len..], marker.as_bytes())
            .map(|pos| base_len + pos)
            .ok_or_else(|| Error::Internal("emplacement /Contents introuvable".into()))?;
        let contents_end = contents_start + marker.len();
        let byte_range_pos = find_subslice(&file[base_len..], BYTE_RANGE_PLACEHOLDER)
            .map(|pos| base_len + pos)
            .ok_or_else(|| Error::Internal("emplacement /ByteRange introuvable".into()))?;
        let replacement = format!(
            "[0 {:010} {:010} {:010}]",
            contents_start,
            contents_end,
            total_len - contents_end
        );
        if replacement.len() != BYTE_RANGE_PLACEHOLDER.len() {
            return Err(Error::Internal("largeur de /ByteRange inattendue".into()));
        }
        file[byte_range_pos..byte_range_pos + replacement.len()]
            .copy_from_slice(replacement.as_bytes());

        let mut hasher = Sha256::new();
        hasher.update(&file[..contents_start]);
        hasher.update(&file[contents_end..]);
        let digest = hasher.finalize();

        let cms = build_cms(identity, &digest)?;
        if cms.len() > SIGNATURE_CAPACITY {
            return Err(Error::NotSupported(format!(
                "signature trop volumineuse ({} octets, limite {})",
                cms.len(),
                SIGNATURE_CAPACITY
            )));
        }
        let mut hex = hex_encode(&cms);
        hex.extend(std::iter::repeat('0').take(SIGNATURE_CAPACITY * 2 - hex.len()));
        file[contents_start + 1..contents_start + 1 + hex.len()]
            .copy_from_slice(hex.as_bytes());

        Ok(file)
    }
}

/// Construit le `ContentInfo` PKCS#7/SignedData détaché.
fn build_cms(identity: &SigningIdentity, digest: &[u8]) -> Result<Vec<u8>> {
    use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
    use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
    use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};

    let signer = SigningKey::<Sha256>::new(identity.private_key.clone());
    let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
        issuer: identity.certificate.tbs_certificate.issuer.clone(),
        serial_number: identity.certificate.tbs_certificate.serial_number.clone(),
    });
    let econtent = EncapsulatedContentInfo {
        econtent_type: const_oid::db::rfc5911::ID_DATA,
        econtent: None,
    };
    let digest_algorithm = AlgorithmIdentifierOwned {
        oid: OID_SHA_256,
        parameters: Some(Any::null()),
    };
    let mut signed = SignerInfoBuilder::new(
        &signer,
        sid,
        digest_algorithm.clone(),
        &econtent,
        Some(digest),
    )
    .map_err(|e| Error::Internal(format!("CMS signer : {e}")))?;

    let signing_time = der::asn1::UtcTime::from_system_time(SystemTime::now())
        .map_err(|e| Error::Internal(format!("horodatage : {e}")))?;
    let mut values = SetOfVec::new();
    values
        .insert(Any::encode_from(&signing_time).map_err(|e| Error::Internal(format!("attribut : {e}")))?)
        .map_err(|e| Error::Internal(format!("attribut : {e}")))?;
    signed
        .add_signed_attribute(x509_cert::attr::Attribute {
            oid: OID_SIGNING_TIME,
            values,
        })
        .map_err(|e| Error::Internal(format!("attribut : {e}")))?;

    let mut builder = SignedDataBuilder::new(&econtent);
    builder
        .add_digest_algorithm(digest_algorithm)
        .map_err(|e| Error::Internal(format!("CMS digest : {e}")))?;
    builder
        .add_certificate(CertificateChoices::Certificate(identity.certificate.clone()))
        .map_err(|e| Error::Internal(format!("CMS certificat : {e}")))?;
    for certificate in &identity.chain {
        builder
            .add_certificate(CertificateChoices::Certificate(certificate.clone()))
            .map_err(|e| Error::Internal(format!("CMS chaîne : {e}")))?;
    }
    builder
        .add_signer_info::<SigningKey<Sha256>, RsaSignature>(signed)
        .map_err(|e| Error::Internal(format!("CMS signature : {e}")))?;
    let content_info = builder
        .build()
        .map_err(|e| Error::Internal(format!("CMS : {e}")))?;
    content_info
        .to_der()
        .map_err(|e| Error::Internal(format!("CMS DER : {e}")))
}
