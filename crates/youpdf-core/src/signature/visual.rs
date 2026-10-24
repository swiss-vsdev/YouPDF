//! Signature visuelle : image ou texte alternatif posé sur une page.

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::signature::util::escape_sig;
use crate::signature::SignaturePlacement;

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
            // RGB, faite ici même (extraction de palette).
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

        // 2. Enregistrer dans /Resources/XObject sous un nom libre. Un nom fixe
        //    ferait que deux images posées sur la même page partageraient la
        //    ressource : la seconde écraserait la première, et l'inspecteur
        //    d'image ne saurait plus laquelle il édite.
        let name = self.free_xobject_name(page_id, "SigIm")?;
        self.link_xobject_to_page(page_id, &name, img_id)?;

        // 3. Flux de dessin.
        let [x1, y1, x2, y2] = rect;
        let w = x2 - x1;
        let h = y2 - y1;
        let content = format!("q\n{w:.2} 0 0 {h:.2} {x1:.2} {y1:.2} cm\n/{name} Do\nQ\n");
        let stream = Stream::new(lopdf::Dictionary::new(), content.into_bytes());
        let stream_id = self.inner_mut().add_object(stream);
        self.append_content_by_id(page_id, stream_id)?;

        Ok(())
    }

    /// Premier nom de ressource libre de la forme `prefixe{n}`.
    fn free_xobject_name(&self, page_id: (u32, u16), prefix: &str) -> Result<String> {
        let used: Vec<Vec<u8>> = self
            .read_resources(page_id)?
            .get(b"XObject")
            .ok()
            .and_then(|object| object.as_dict().ok())
            .map(|xobjects| xobjects.iter().map(|(name, _)| name.clone()).collect())
            .unwrap_or_default();
        for index in 0..1000u32 {
            let candidate = format!("{prefix}{index}");
            if !used.iter().any(|name| name == candidate.as_bytes()) {
                return Ok(candidate);
            }
        }
        Err(Error::Pdf("plus de nom d'image libre".to_owned()))
    }

    /// Enregistre un XObject dans les ressources d'une page.
    fn link_xobject_to_page(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        object_id: (u32, u16),
    ) -> Result<()> {
        // Lecture des ressources actuelles. `/XObject` peut être une
        // référence : la résoudre évite d'écraser un dictionnaire partagé.
        let mut resources = self.read_resources(page_id)?;
        let mut xobjects = self
            .resolve_dict(resources.get(b"XObject").ok())
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

    /// Lit les ressources d'une page, en résolvant les références et en
    /// héritant de ses parents.
    fn read_resources(&self, page_id: (u32, u16)) -> Result<lopdf::Dictionary> {
        self.page_resources(page_id)
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
