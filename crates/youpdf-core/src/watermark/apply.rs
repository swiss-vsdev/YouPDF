// ────────────────────────────────────────────────────────────────────────
// Intégration dans un Document
// ────────────────────────────────────────────────────────────────────────

use super::*;
use crate::render::{RenderOptions, Renderer};

impl Document {
    /// Ajoute un filigrane texte aux pages de `wm.pages`.
    ///
    /// Renvoie le nombre de pages décorées.
    pub fn add_text_watermark(&mut self, wm: &Watermark) -> Result<usize> {
        if wm.text.trim().is_empty() {
            return Err(Error::InvalidInput("filigrane sans texte".to_string()));
        }

        let total = self.page_count() as u32;
        let pages = wm.pages.pages(total);
        for page in &pages {
            let page_id = self
                .page_id(*page)
                .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
            let (width, height) = self.page_size(*page);

            let ops = watermark_ops(wm, width, height);
            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(marked_dict(), ops));

            self.wm_ensure_font(page_id)?;
            if wm.opacity < 1.0 {
                self.wm_ensure_extgstate(page_id, wm.opacity.clamp(0.05, 1.0))?;
            }
            if wm.behind {
                self.wm_prepend_content(page_id, stream_id)?;
            } else {
                self.wm_append_content(page_id, stream_id)?;
            }
        }

        tracing::debug!(pages = pages.len(), "filigrane ajouté");
        Ok(pages.len())
    }

    /// Ajoute en-tête, pied de page et numérotation aux pages choisies.
    ///
    /// Renvoie le nombre de pages effectivement décorées (une page sans
    /// rien à écrire n'est pas touchée).
    pub fn add_header_footer(&mut self, hf: &HeaderFooter) -> Result<usize> {
        let total = self.page_count() as u32;
        let pages = hf.pages.pages(total);
        let mut decorated = 0usize;

        for page in &pages {
            let page_id = self
                .page_id(*page)
                .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
            let (width, height) = self.page_size(*page);

            let Some(ops) = header_footer_ops(hf, *page, total, width, height) else {
                continue;
            };
            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(marked_dict(), ops));

            self.wm_ensure_font(page_id)?;
            self.wm_append_content(page_id, stream_id)?;
            decorated += 1;
        }

        tracing::debug!(pages = decorated, "en-tête et pied de page ajoutés");
        Ok(decorated)
    }

    /// Retire tous les filigranes, en-têtes et numéros posés par ce module.
    ///
    /// Les flux retirés sont exactement ceux marqués `/YouPdfDecoration` :
    /// le contenu d'origine de la page n'est jamais touché. Renvoie le
    /// nombre de flux retirés.
    pub fn clear_watermarks(&mut self) -> Result<usize> {
        self.wm_clear_marked(WM_MARKER)
    }

    /// Ajoute un arrière-plan aux pages de `bg.pages`.
    ///
    /// L'arrière-plan est inséré **avant** le contenu existant : il se
    /// dessine donc derrière le texte et les images de la page. Renvoie le
    /// nombre de pages décorées.
    pub fn add_background(&mut self, bg: &Background) -> Result<usize> {
        let total = self.page_count() as u32;
        let pages = bg.pages.pages(total);

        // L'image n'est décodée qu'une fois, puis partagée entre les pages.
        let image = match &bg.image {
            Some(path) => Some(background_image_stream(path)?),
            None => None,
        };
        let image_res = match image {
            Some((stream, width, height)) => {
                let id = self.inner_mut().add_object(stream);
                Some((id, width as f32, height as f32))
            }
            None => None,
        };

        for page in &pages {
            let page_id = self
                .page_id(*page)
                .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
            let view = self.page_view(*page);

            let ops = match &image_res {
                Some((_, width, height)) => background_image_ops(view, *width, *height, bg.scale),
                None => background_color_ops(bg, view),
            };
            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(marked_dict_with(BG_MARKER), ops));

            if let Some((xobject_id, _, _)) = image_res {
                self.wm_link_xobject(page_id, BG_XOBJECT_NAME, xobject_id)?;
            }
            if bg.opacity < 1.0 {
                self.wm_ensure_extgstate(page_id, bg.opacity.clamp(0.05, 1.0))?;
            }
            self.wm_prepend_content(page_id, stream_id)?;
        }

        tracing::debug!(pages = pages.len(), "arrière-plan ajouté");
        Ok(pages.len())
    }

    /// Retire tous les arrière-plans posés par ce module.
    ///
    /// Seuls les flux marqués `/YouPdfBackground` sont retirés : les
    /// filigranes, en-têtes et numéros restent en place. Renvoie le nombre
    /// de flux retirés.
    pub fn clear_backgrounds(&mut self) -> Result<usize> {
        self.wm_clear_marked(BG_MARKER)
    }

    /// Ajoute une numérotation Bates aux pages de `b.pages`.
    ///
    /// Le premier numéro de la sélection est `b.start` ; chaque page suivante
    /// s'incrémente de `b.step`. Renvoie le nombre de pages numérotées.
    pub fn add_bates(&mut self, b: &Bates) -> Result<usize> {
        let total = self.page_count() as u32;
        let pages = b.pages.pages(total);
        let digits = b.digits.clamp(1, 20);
        let step = b.step.max(1);

        for (index, page) in pages.iter().enumerate() {
            let page_id = self
                .page_id(*page)
                .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
            let n = b.start.saturating_add(index as u32 * step);
            let label = format!("{}{n:0width$}{}", b.prefix, b.suffix, width = digits);

            let view = self.page_view(*page);
            let ops = bates_ops(b, &label, view);
            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(marked_dict_with(BATES_MARKER), ops));

            self.wm_ensure_font(page_id)?;
            self.wm_append_content(page_id, stream_id)?;
        }

        tracing::debug!(pages = pages.len(), "numérotation Bates ajoutée");
        Ok(pages.len())
    }

    /// Retire tous les numéros Bates posés par ce module.
    ///
    /// Seuls les flux marqués `/YouPdfBates` sont retirés. Renvoie le nombre
    /// de flux retirés.
    pub fn clear_bates(&mut self) -> Result<usize> {
        self.wm_clear_marked(BATES_MARKER)
    }

    /// Ajoute un filigrane depuis un PDF source : chaque page décorée
    /// reçoit le flux de la page source, échelle et positionnée selon
    /// `wm`. C'est le filigrane « PDF-file » : l'autre PDF devient un
    /// filigrane, imprimé derrière ou devant le texte.
    ///
    /// `source` est un [`Document`] ouvert en mémoire, `from_page` est le
    /// numéro (1-indexé) de la page source à utiliser comme filigrane.
    /// Renvoie le nombre de pages décorées.
    ///
    /// ponytail: le flux source est rasterisé par le renderer fourni ;
    /// sans renderer, l'opération échoue (comme le caviardage).
    pub fn add_pdf_watermark(
        &mut self,
        wm: &Watermark,
        source: &Document,
        from_page: u32,
        renderer: &dyn Renderer,
    ) -> Result<usize> {
        let src_total = source.page_count() as u32;
        if src_total == 0 || from_page < 1 || from_page > src_total {
            return Err(Error::InvalidInput("source page out of range".into()));
        }
        if !renderer.renders_real_content() {
            return Err(Error::NotSupported(
                "PDF watermark needs pdfium (renderer unavailable)".into(),
            ));
        }

        let total = self.page_count() as u32;
        let pages = wm.pages.pages(total);
        let (sw, sh) = source.page_size(from_page);
        if sw <= 0.0 || sh <= 0.0 {
            return Err(Error::InvalidInput("source page has no size".into()));
        }

        let mut decorated = 0usize;
        for page in &pages {
            let page_id = self
                .page_id(*page)
                .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
            let (width, height) = self.page_size(*page);
            let ops = pdf_watermark_ops(wm, width, height, sw, sh);
            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(marked_dict(), ops));
            let image_id = self.rasterize_source_page(source, from_page, renderer)?;
            self.wm_link_xobject(page_id, WM_PDF_XOBJECT_NAME, image_id)?;
            if wm.behind {
                self.wm_prepend_content(page_id, stream_id)?;
            } else {
                self.wm_append_content(page_id, stream_id)?;
            }
            decorated += 1;
        }
        tracing::debug!(pages = decorated, "filigrane PDF ajouté");
        Ok(decorated)
    }

    /// Rasterise une page d'un document source en XObject image.
    fn rasterize_source_page(
        &mut self,
        source: &Document,
        page: u32,
        renderer: &dyn Renderer,
    ) -> Result<ObjectId> {
        let opts = RenderOptions {
            scale: 1.0,
            rotation: 0,
            ..Default::default()
        };
        let rgba = renderer
            .render_page(source, page, &opts)
            .map_err(|e| Error::Pdf(format!("render source page {page} : {e}")))?;
        let (w, h) = (rgba.width, rgba.height);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for chunk in rgba.pixels.chunks_exact(4) {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }
        let data = flate_compress(&rgb)?;
        let stream = Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Image".to_vec()),
                "Width" => Object::Integer(w as i64),
                "Height" => Object::Integer(h as i64),
                "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                "BitsPerComponent" => Object::Integer(8),
                "Filter" => Object::Name(b"FlateDecode".to_vec()),
            },
            data,
        );
        Ok(self.inner_mut().add_object(stream))
    }

    /// Retire les flux de contenu marqués par `marker`, sur toutes les pages.
    ///
    /// Le contenu d'origine n'est jamais touché : seuls les flux portant la
    /// clé sont retirés. Renvoie le nombre de flux retirés.
    pub(crate) fn wm_clear_marked(&mut self, marker: &str) -> Result<usize> {
        let pages = self.inner().get_pages();
        let mut removed = 0usize;

        for (_num, page_id) in pages {
            let Ok(page) = self.inner().get_dictionary(page_id) else {
                continue;
            };
            let Ok(contents) = page.get(b"Contents") else {
                continue;
            };
            let listed: Vec<Object> = match contents {
                Object::Array(arr) => arr.clone(),
                single => vec![single.clone()],
            };

            let mut kept: Vec<Object> = Vec::with_capacity(listed.len());
            for item in &listed {
                let ours = item
                    .as_reference()
                    .ok()
                    .and_then(|id| self.inner().get_object(id).ok())
                    .and_then(|o| o.as_stream().ok())
                    .and_then(|s| s.dict.get(marker.as_bytes()).ok())
                    .is_some();
                if ours {
                    removed += 1;
                } else {
                    kept.push(item.clone());
                }
            }
            if kept.len() == listed.len() {
                continue;
            }

            let page = self
                .inner_mut()
                .get_object_mut(page_id)
                .map_err(|e| Error::Pdf(format!("page : {e}")))?;
            let dict = page
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("page : {e}")))?;
            match kept.len() {
                0 => {
                    dict.remove(b"Contents");
                }
                1 => {
                    dict.set("Contents", kept.remove(0));
                }
                _ => dict.set("Contents", Object::Array(kept)),
            }
        }

        Ok(removed)
    }

    /// Pose la police Helvetica /WinAnsiEncoding du module sur une page.
    ///
    /// On n'écrit jamais dans une police existante : un `/F1` peut être en
    /// StandardEncoding ou en Identity-H, où des octets WinAnsi donneraient
    /// un texte illisible.
    pub(crate) fn wm_ensure_font(&mut self, page_id: ObjectId) -> Result<()> {
        if let Ok(fonts) = self
            .inner()
            .get_dictionary(page_id)
            .and_then(|p| p.get(b"Resources"))
            .and_then(|r| r.as_dict())
            .and_then(|r| r.get(b"Font"))
            .and_then(|f| f.as_dict())
        {
            if fonts.get(WM_FONT_NAME.as_bytes()).is_ok() {
                return Ok(());
            }
        }

        let font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        };
        let font_id = self.inner_mut().add_object(Object::Dictionary(font));

        let mut resources = self.wm_read_resources(page_id)?;
        let mut fonts = self.wm_sub_dictionary(&resources, b"Font")?;
        fonts.set(WM_FONT_NAME, Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(fonts));
        self.wm_write_resources(page_id, resources)
    }

    /// Pose l'état graphique de transparence du filigrane sur une page.
    fn wm_ensure_extgstate(&mut self, page_id: ObjectId, opacity: f32) -> Result<()> {
        let mut gs = Dictionary::new();
        gs.set("Type", Object::Name(b"ExtGState".to_vec()));
        gs.set("ca", Object::Real(opacity));
        gs.set("CA", Object::Real(opacity));

        let mut resources = self.wm_read_resources(page_id)?;
        let mut gstates = self.wm_sub_dictionary(&resources, b"ExtGState")?;
        gstates.set(WM_GS_NAME, Object::Dictionary(gs));
        resources.set("ExtGState", Object::Dictionary(gstates));
        self.wm_write_resources(page_id, resources)
    }

    /// Sous-dictionnaire d'une clé de `/Resources`, résolu s'il est
    /// indirect. Vide s'il est absent ou illisible.
    fn wm_sub_dictionary(&self, resources: &Dictionary, key: &[u8]) -> Result<Dictionary> {
        match resources.get(key) {
            Ok(Object::Dictionary(d)) => Ok(d.clone()),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .cloned()
                .map_err(|e| Error::Pdf(format!("{key:?} : {e}"))),
            _ => Ok(Dictionary::new()),
        }
    }

    /// Ressources d'une page, résolues en dictionnaire direct.
    fn wm_read_resources(&self, page_id: ObjectId) -> Result<Dictionary> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        match page.get(b"Resources") {
            Ok(Object::Dictionary(d)) => Ok(d.clone()),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .cloned()
                .map_err(|e| Error::Pdf(format!("ressources : {e}"))),
            _ => Ok(Dictionary::new()),
        }
    }

    /// Écrit les ressources d'une page.
    fn wm_write_resources(&mut self, page_id: ObjectId, resources: Dictionary) -> Result<()> {
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
    pub(crate) fn wm_append_content(&mut self, page_id: ObjectId, stream_id: ObjectId) -> Result<()> {
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

    /// Insère un flux au début du contenu d'une page.
    ///
    /// Miroir de [`Document::wm_append_content`] : le flux passe avant le
    /// contenu existant, qui reste intact et ordonné après lui.
    fn wm_prepend_content(&mut self, page_id: ObjectId, stream_id: ObjectId) -> Result<()> {
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
                        Object::Reference(stream_id),
                        Object::Reference(existing),
                    ]),
                );
            }
            Ok(Object::Array(arr)) => arr.insert(0, Object::Reference(stream_id)),
            _ => dict.set("Contents", Object::Reference(stream_id)),
        }
        Ok(())
    }

    /// Pose un XObject dans `/Resources` d'une page, sous un nom donné.
    fn wm_link_xobject(&mut self, page_id: ObjectId, name: &str, xobject_id: ObjectId) -> Result<()> {
        let mut resources = self.wm_read_resources(page_id)?;
        let mut xobjects = self.wm_sub_dictionary(&resources, b"XObject")?;
        xobjects.set(name, Object::Reference(xobject_id));
        resources.set("XObject", Object::Dictionary(xobjects));
        self.wm_write_resources(page_id, resources)
    }
}