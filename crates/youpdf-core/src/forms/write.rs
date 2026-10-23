use lopdf::{Dictionary, Object, ObjectId, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;

use super::{appearance_states, da_font_alias, field_rect, js_action, set_bit, FieldProperties, FormField, FormFieldKind, NewFormField};

impl Document {
    /// Remplit un champ de formulaire.
    ///
    /// Pour les cases à cocher et boutons radio, la valeur doit être un
    /// nom d'état (`/Off` par défaut, ou un nom présent dans `/AP`).
    /// On met à jour `/V` et `/AS` simultanément.
    pub fn fill_form_field(&mut self, field: &FormField, value: &str) -> Result<()> {
        if field.read_only {
            return Err(Error::InvalidInput(format!(
                "le champ « {} » est en lecture seule",
                field.name
            )));
        }

        // `set_field_value` met `/V` et, pour les cases, `/AS`.
        self.set_field_value(field, value)?;

        // Marquer le document comme ayant des changements (côté appelant).

        tracing::debug!(field = %field.name, "champ rempli");
        Ok(())
    }

    /// Crée un champ de formulaire et l'inscrit dans `/AcroForm`.
    ///
    /// Le champ reçoit une apparence (`/AP`) pour que les lecteurs qui
    /// n'ont pas de générateur d'apparences l'affichent quand même, et
    /// `/NeedAppearances` reste vrai pour ceux qui en ont un.
    pub fn create_form_field(&mut self, field: &NewFormField) -> Result<FormField> {
        if field.name.trim().is_empty() {
            return Err(Error::InvalidInput(
                "un champ de formulaire doit avoir un nom".to_string(),
            ));
        }
        let [x1, y1, x2, y2] = field.rect;
        let rect = [
            x1.min(x2),
            y1.min(y2),
            x1.max(x2),
            y1.max(y2),
        ];
        let w = (rect[2] - rect[0]).max(4.0);
        let h = (rect[3] - rect[1]).max(4.0);

        let page_id = self
            .page_id(field.page)
            .map_err(|e| Error::Pdf(format!("page {} : {e}", field.page)))?;
        let font = self.ensure_appearance_font()?;

        // Flags de champ (`/Ff`).
        let mut flags: i64 = match field.kind {
            FormFieldKind::Text => {
                if field.multiline {
                    1 << 12
                } else {
                    0
                }
            }
            FormFieldKind::Radio => 1 << 15,
            FormFieldKind::Choice => 1 << 17,
            FormFieldKind::MultiChoice => (1 << 17) | (1 << 20),
            // Bit 17 : bouton poussoir (pas d'état coché).
            FormFieldKind::Button => 1 << 16,
            _ => 0,
        };
        if field.read_only {
            // Bit 1 : lecture seule.
            flags |= 1;
        }

        let ft: &[u8] = match field.kind {
            FormFieldKind::Text => b"Tx",
            FormFieldKind::Checkbox | FormFieldKind::Radio => b"Btn",
            FormFieldKind::Choice | FormFieldKind::MultiChoice => b"Ch",
            FormFieldKind::Signature => b"Sig",
            // Un bouton poussoir reste un `/Btn` : le drapeau 17 suffit.
            FormFieldKind::Button => b"Btn",
            // Un code-barres PDF est un champ texte à police dédiée.
            FormFieldKind::Date | FormFieldKind::Barcode => b"Tx",
            FormFieldKind::Other(ref other) => {
                return Err(Error::InvalidInput(format!(
                    "type de champ non créable : {other}"
                )))
            }
        };

        // Apparence : un XObject formulaire par état. Une case a deux
        // états (`/Off` et `/Yes`), les autres un seul.
        let font_obj = Object::Reference(font);
        let (ap_obj, is_choice_state) = match field.kind {
            FormFieldKind::Checkbox | FormFieldKind::Radio => {
                let off = self.add_appearance(font_obj.clone(), w, h, None, false)?;
                let on = self.add_appearance(font_obj.clone(), w, h, None, true)?;
                let mut states = Dictionary::new();
                states.set("Off", Object::Reference(off));
                states.set("Yes", Object::Reference(on));
                (
                    Object::Dictionary(states),
                    true,
                )
            }
            _ => {
                let value = field.value.as_deref();
                let id = self.add_appearance(font_obj.clone(), w, h, value, false)?;
                (Object::Reference(id), false)
            }
        };

        let mut widget = Dictionary::new();
        widget.set("Type", Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", Object::Name(b"Widget".to_vec()));
        widget.set("FT", Object::Name(ft.to_vec()));
        widget.set(
            "T",
            Object::string_literal(crate::edit::encode_pdf_literal(&field.name)),
        );
        widget.set(
            "Rect",
            Object::Array(vec![
                Object::Real(rect[0]),
                Object::Real(rect[1]),
                Object::Real(rect[2]),
                Object::Real(rect[3]),
            ]),
        );
        widget.set("P", Object::Reference(page_id));
        widget.set("F", Object::Integer(4));
        if flags != 0 {
            widget.set("Ff", Object::Integer(flags));
        }
        widget.set("DA", Object::string_literal(b"/Helv 0 Tf 0 g".to_vec()));
        let mut mk = Dictionary::new();
        mk.set(
            "BG",
            Object::Array(vec![Object::Real(1.0), Object::Real(1.0), Object::Real(1.0)]),
        );
        mk.set(
            "BC",
            Object::Array(vec![Object::Real(0.5), Object::Real(0.5), Object::Real(0.5)]),
        );
        widget.set("MK", Object::Dictionary(mk));
        if is_choice_state {
            let state = field.value.clone().unwrap_or_else(|| "Off".to_string());
            widget.set("V", Object::Name(state.as_bytes().to_vec()));
            widget.set("AS", Object::Name(state.as_bytes().to_vec()));
        } else if let Some(value) = field.value.as_deref() {
            widget.set(
                "V",
                Object::string_literal(crate::edit::encode_pdf_literal(value)),
            );
        }
        if !field.options.is_empty() {
            let opts: Vec<Object> = field
                .options
                .iter()
                .map(|o| Object::string_literal(crate::edit::encode_pdf_literal(o)))
                .collect();
            widget.set("Opt", Object::Array(opts));
        }
        let mut ap = Dictionary::new();
        ap.set("N", ap_obj);
        widget.set("AP", Object::Dictionary(ap));

        let widget_id = self.inner_mut().add_object(Object::Dictionary(widget));

        // Page : le widget rejoint les annotations de la page.
        let mut annots = self.page_annots(page_id);
        annots.push(Object::Reference(widget_id));
        self.set_page_annots(page_id, &annots)?;

        self.register_acroform_field(widget_id, font)?;
        self.touch();

        let states = match field.kind {
            FormFieldKind::Checkbox | FormFieldKind::Radio => vec!["Yes".to_string()],
            _ => Vec::new(),
        };
        Ok(FormField {
            object_id: widget_id,
            widget_id,
            name: field.name.clone(),
            kind: field.kind.clone(),
            value: field.value.clone(),
            options: field.options.clone(),
            read_only: field.read_only,
            tooltip: String::new(),
            required: false,
            multiline: false,
            page: Some(field.page as usize),
            rect,
            states,
            state: field
                .value
                .clone()
                .filter(|_| is_choice_state),
        })
    }

    /// Ajoute un XObject d'apparence (fond blanc, cadre, croix ou texte).
    fn add_appearance(
        &mut self,
        font: Object,
        w: f32,
        h: f32,
        value: Option<&str>,
        checked: bool,
    ) -> Result<ObjectId> {
        // Fond et cadre, puis croix ou valeur selon le cas.
        let mut ops: Vec<u8> = Vec::new();
        ops.extend_from_slice(format!("1 1 1 rg 0 0 {w:.2} {h:.2} re f\n").as_bytes());
        ops.extend_from_slice(
            format!(
                "0.5 0.5 0.5 RG 0.7 w 0.35 0.35 {:.2} {:.2} re S\n",
                (w - 0.7).max(0.1),
                (h - 0.7).max(0.1)
            )
            .as_bytes(),
        );
        if checked {
            ops.extend_from_slice(b"0 0 0 RG 1.2 w\n");
            ops.extend_from_slice(
                format!("1.5 1.5 m {:.2} {:.2} l S\n", w - 1.5, h - 1.5).as_bytes(),
            );
            ops.extend_from_slice(
                format!("1.5 {:.2} m {:.2} 1.5 l S\n", h - 1.5, w - 1.5).as_bytes(),
            );
        }
        if let Some(value) = value {
            if !value.is_empty() {
                let size = (h * 0.6).clamp(6.0, 12.0);
                ops.extend_from_slice(format!("BT /Helv {size:.1} Tf 0 g 2 2 Td ").as_bytes());
                crate::annotate::push_text_literal(&mut ops, value);
                ops.extend_from_slice(b" Tj ET\n");
            }
        }

        let mut xobject = Dictionary::new();
        xobject.set("Type", Object::Name(b"XObject".to_vec()));
        xobject.set("Subtype", Object::Name(b"Form".to_vec()));
        xobject.set("FormType", Object::Integer(1));
        xobject.set(
            "BBox",
            Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(w),
                Object::Real(h),
            ]),
        );
        let mut fonts = Dictionary::new();
        fonts.set("Helv", font);
        let mut resources = Dictionary::new();
        resources.set("Font", Object::Dictionary(fonts));
        xobject.set("Resources", Object::Dictionary(resources));
        Ok(self.inner_mut().add_object(Stream::new(xobject, ops)))
    }

    /// Inscrit le widget dans `/Fields` de `/AcroForm`, en créant le
    /// dictionnaire s'il n'existe pas encore.
    fn register_acroform_field(&mut self, widget_id: ObjectId, font: ObjectId) -> Result<()> {
        let root_id = self
            .inner()
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| Error::Pdf("catalogue absent".to_string()))?;

        // Base : le dictionnaire existant, cloné pour éviter les emprunts
        // croisés pendant la réécriture.
        let base = self.acroform_dict().cloned();
        let mut acro = base.unwrap_or_default();
        // Un `/Fields` indirect se modifie sur place : le remplacer par un
        // tableau neuf ferait disparaître les champs déjà déclarés.
        match acro.get(b"Fields").cloned() {
            Ok(Object::Reference(array_id)) => {
                if let Ok(Object::Array(array)) = self.inner_mut().get_object_mut(array_id) {
                    array.push(Object::Reference(widget_id));
                }
            }
            Ok(Object::Array(mut array)) => {
                array.push(Object::Reference(widget_id));
                acro.set("Fields", Object::Array(array));
            }
            _ => {
                acro.set("Fields", Object::Array(vec![Object::Reference(widget_id)]));
            }
        }
        if acro.get(b"DR").is_err() {
            let mut fonts = Dictionary::new();
            fonts.set("Helv", Object::Reference(font));
            let mut dr = Dictionary::new();
            dr.set("Font", Object::Dictionary(fonts));
            acro.set("DR", Object::Dictionary(dr));
        }
        if acro.get(b"DA").is_err() {
            acro.set("DA", Object::string_literal(b"/Helv 0 Tf 0 g".to_vec()));
        }
        acro.set("NeedAppearances", Object::Boolean(true));

        let acro_id = self.inner_mut().add_object(Object::Dictionary(acro));
        let catalog = self
            .inner_mut()
            .get_object_mut(root_id)
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
        let dict = catalog
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
        dict.set("AcroForm", Object::Reference(acro_id));
        Ok(())
    }
}

impl Document {
    /// Remet tous les champs à leur valeur par défaut (`/DV`), ou les vide.
    ///
    /// Renvoie le nombre de champs touchés.
    pub fn reset_form_fields(&mut self) -> Result<usize> {
        let fields = self.read_form_fields();
        let mut count = 0;
        for field in fields {
            if field.read_only {
                continue;
            }
            let default = self
                .inner()
                .get_object(field.object_id)
                .ok()
                .and_then(|obj| obj.as_dict().ok())
                .and_then(|dict| dict.get(b"DV").ok().cloned())
                .and_then(|obj| match obj {
                    Object::String(bytes, _) => Some(crate::structure::decode_pdf_text(&bytes)),
                    Object::Name(name) => Some(crate::structure::decode_pdf_text(&name)),
                    _ => None,
                });
            let value = match field.kind {
                FormFieldKind::Checkbox | FormFieldKind::Radio | FormFieldKind::Button => {
                    default.unwrap_or_else(|| "Off".to_string())
                }
                _ => default.unwrap_or_default(),
            };
            if self.fill_form_field(&field, &value).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Déplace et redimensionne un champ de formulaire.
    ///
    /// Met à jour le `/Rect` du widget et, quand le champ porte ses propres
    /// limites, celui du dictionnaire de champ. L'apparence est régénérée par
    /// les lecteurs (`/NeedAppearances` reste vrai).
    pub fn set_form_field_rect(&mut self, field: &FormField, rect: [f32; 4]) -> Result<()> {
        if rect[2] - rect[0] < 2.0 || rect[3] - rect[1] < 2.0 {
            return Err(Error::InvalidInput(
                "un champ doit mesurer au moins 2 × 2 points".to_string(),
            ));
        }
        let array = vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(rect[2]),
            Object::Real(rect[3]),
        ];
        let obj = self
            .inner_mut()
            .get_object_mut(field.widget_id)
            .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
        dict.set("Rect", Object::Array(array));
        Ok(())
    }

    /// Applique `props` au champ dont le nom actuel est `field.name`.
    pub fn update_form_field(&mut self, field: &FormField, props: &FieldProperties) -> Result<()> {
        let object_id = self
            .read_form_fields()
            .into_iter()
            .find(|f| f.name == field.name)
            .map(|f| f.object_id)
            .ok_or_else(|| Error::InvalidInput(format!("champ « {} » introuvable", field.name)))?;

        // Le nom complet peut être hiérarchique : on n'écrit que le segment
        // local, et seulement quand le préfixe du parent est bien celui du nom
        // demandé. Un champ de premier niveau garde le nom saisi tel quel —
        // le tronquer au dernier point renommerait « a.b » en « b ».
        let local = match field.name.rsplit_once('.') {
            Some((prefix, _)) => props
                .name
                .strip_prefix(&format!("{prefix}."))
                .or_else(|| props.name.rsplit('.').next())
                .unwrap_or(&props.name),
            None => props.name.as_str(),
        };
        let is_choice = matches!(
            field.kind,
            FormFieldKind::Choice | FormFieldKind::MultiChoice
        );

        {
            let obj = self
                .inner_mut()
                .get_object_mut(object_id)
                .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
            let dict = obj
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;

            if !local.is_empty() {
                dict.set(
                    "T",
                    Object::string_literal(crate::edit::encode_pdf_literal(local)),
                );
            }
            dict.set(
                "TU",
                Object::string_literal(crate::edit::encode_pdf_literal(&props.tooltip)),
            );

            // Chaque drapeau est posé ou effacé sans toucher aux autres.
            let mut flags = dict
                .get(b"Ff")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            set_bit(&mut flags, 1, props.read_only);
            set_bit(&mut flags, 2, props.required);
            set_bit(&mut flags, 4096, props.multiline);

            if is_choice && !props.options.is_empty() {
                let opts: Vec<Object> = props
                    .options
                    .iter()
                    .map(|o| Object::string_literal(crate::edit::encode_pdf_literal(o)))
                    .collect();
                dict.set("Opt", Object::Array(opts));
                // Bit 19 : l'utilisateur peut saisir une valeur absente de la
                // liste (liste déroulante éditable).
                set_bit(&mut flags, 1 << 18, props.editable);
            }
            // Bits 14 (mot de passe) et 25 (peigne).
            set_bit(&mut flags, 1 << 13, props.password);
            set_bit(&mut flags, 1 << 24, props.comb > 0);
            if props.comb > 0 {
                dict.set("MaxLen", Object::Integer(props.comb as i64));
            }
            dict.set("Ff", Object::Integer(flags));

            // Alignement du texte.
            dict.set("Q", Object::Integer(props.alignment.min(2) as i64));

            // Visibilité : bit 2 masqué, bit 3 imprimable, bit 6 masqué à
            // l'écran.
            let mut field_flags = dict
                .get(b"F")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            set_bit(&mut field_flags, 1, props.visibility == 1 || props.visibility == 3);
            set_bit(&mut field_flags, 2, props.visibility != 2);
            set_bit(&mut field_flags, 32, props.visibility == 2);
            dict.set("F", Object::Integer(field_flags));

            // Valeur par défaut.
            if !props.default_value.is_empty() {
                dict.set(
                    "DV",
                    Object::string_literal(crate::edit::encode_pdf_literal(&props.default_value)),
                );
            }

            // Apparence : bordure et fond.
            let mut mk = Dictionary::new();
            mk.set(
                "BC",
                Object::Array(
                    props
                        .border_color
                        .iter()
                        .map(|c| Object::Real(*c as f32 / 255.0))
                        .collect(),
                ),
            );
            if props.fill {
                mk.set(
                    "BG",
                    Object::Array(
                        props
                            .fill_color
                            .iter()
                            .map(|c| Object::Real(*c as f32 / 255.0))
                            .collect(),
                    ),
                );
            }
            dict.set("MK", Object::Dictionary(mk));

            let style = match props.border_style {
                1 => "D",
                2 => "B",
                3 => "I",
                4 => "U",
                _ => "S",
            };
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(props.border_width.clamp(0.0, 12.0)));
            bs.set("S", Object::Name(style.as_bytes().to_vec()));
            dict.set("BS", Object::Dictionary(bs));

            // `/DA` : police, corps et couleur du texte.
            let size = if props.font_size > 0.0 {
                props.font_size
            } else {
                12.0
            };
            let [r, g, b] = props.text_color.map(|c| c as f32 / 255.0);
            let da = format!(
                "/{} {} Tf {r:.3} {g:.3} {b:.3} rg",
                da_font_alias(&props.font),
                size
            );
            dict.set("DA", Object::string_literal(da.as_bytes().to_vec()));

            // Actions, format et calcul.
            let mut aa = Dictionary::new();
            if !props.action_js.is_empty() {
                aa.set("E", js_action(&props.action_js));
            }
            if !props.format_js.is_empty() {
                aa.set("F", js_action(&props.format_js));
            }
            if !props.calc_js.is_empty() {
                aa.set("C", js_action(&props.calc_js));
            }
            if !aa.is_empty() {
                dict.set("AA", Object::Dictionary(aa));
            }
        }

        // Les apparences ne sont plus à jour : les lecteurs les régénèrent.
        if let Some(acro) = self.acroform_dict().cloned() {
            let mut acro = acro;
            acro.set("NeedAppearances", Object::Boolean(true));
            if let Ok(root) = self
                .inner()
                .trailer
                .get(b"Root")
                .and_then(|o| o.as_reference())
            {
                if let Ok(catalog) = self.inner_mut().get_object_mut(root) {
                    if let Ok(catalog) = catalog.as_dict_mut() {
                        catalog.set("AcroForm", Object::Dictionary(acro));
                    }
                }
            }
        }

        self.touch();
        tracing::debug!(field = %field.name, "champ mis à jour");
        Ok(())
    }
    /// Écrit `/V` (et `/AS` pour les cases) sans contrôle de lecture seule.
    pub(super) fn set_field_value(&mut self, field: &FormField, value: &str) -> Result<()> {
        let button = matches!(
            field.kind,
            FormFieldKind::Checkbox | FormFieldKind::Radio | FormFieldKind::Button
        );
        if button {
            let selected = if value.is_empty() || value == "Off" {
                "Off".to_string()
            } else {
                self.resolve_button_state(field, value)
            };
            self.set_dict_name(field.object_id, "V", &selected)?;
            for widget_id in self.field_widget_ids(field) {
                let state = if selected == "Off" {
                    "Off".to_string()
                } else if widget_id == field.widget_id {
                    selected.clone()
                } else {
                    let states = self.widget_states(widget_id);
                    if states.iter().any(|state| state == &selected) {
                        selected.clone()
                    } else {
                        "Off".to_string()
                    }
                };
                self.set_dict_name(widget_id, "AS", &state)?;
            }
            return Ok(());
        }

        self.set_dict_string(field.object_id, "V", value)?;
        // Le texte doit rester lisible dans un lecteur qui ne régénère pas
        // les apparences : on écrit un nouveau flux `/AP`.
        let font = self.ensure_appearance_font()?;
        for widget_id in self.field_widget_ids(field) {
            let (w, h) = self.widget_size(widget_id, field.rect);
            let ap = self.add_appearance(Object::Reference(font), w, h, Some(value), false)?;
            self.set_widget_appearance(widget_id, ap)?;
        }
        Ok(())
    }

    /// Choisit l'état d'apparence réel d'un bouton.
    ///
    /// Les producteurs nomment rarement l'état `Yes` : `/On`, `/1`, `/Choice2`
    /// sont courants. Une valeur inconnue retombe sur le premier état du
    /// widget plutôt que d'écrire un `/AS` qui n'existe pas.
    fn resolve_button_state(&self, field: &FormField, value: &str) -> String {
        for widget_id in self.field_widget_ids(field) {
            let states = self.widget_states(widget_id);
            if let Some(state) = states.iter().find(|state| state.as_str() == value) {
                return state.clone();
            }
        }
        if let Some(state) = field
            .states
            .iter()
            .find(|state| state.as_str() == value)
        {
            return state.clone();
        }
        for widget_id in self.field_widget_ids(field) {
            if let Some(state) = self.widget_states(widget_id).into_iter().next() {
                return state;
            }
        }
        if let Some(state) = field.states.first() {
            return state.clone();
        }
        value.to_string()
    }

    /// Widgets d'un champ : le sien, plus ses enfants qui n'ont pas de `/T`.
    fn field_widget_ids(&self, field: &FormField) -> Vec<(u32, u16)> {
        let mut ids = vec![field.widget_id];
        if let Ok(dict) = self
            .inner()
            .get_object(field.object_id)
            .and_then(|obj| obj.as_dict())
        {
            if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
                for kid in kids {
                    let Ok(id) = kid.as_reference() else {
                        continue;
                    };
                    let Ok(kid_dict) = self.inner().get_object(id).and_then(|obj| obj.as_dict())
                    else {
                        continue;
                    };
                    if kid_dict.get(b"T").is_err() && !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        ids
    }

    /// États d'apparence d'un widget, hors `Off`.
    fn widget_states(&self, widget_id: (u32, u16)) -> Vec<String> {
        self.inner()
            .get_object(widget_id)
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .map(appearance_states)
            .unwrap_or_default()
    }

    fn widget_size(&self, widget_id: (u32, u16), fallback: [f32; 4]) -> (f32, f32) {
        let rect = self
            .inner()
            .get_object(widget_id)
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .and_then(field_rect)
            .unwrap_or(fallback);
        (
            (rect[2] - rect[0]).max(4.0),
            (rect[3] - rect[1]).max(4.0),
        )
    }

    fn set_dict_name(&mut self, object_id: (u32, u16), key: &str, value: &str) -> Result<()> {
        let obj = self
            .inner_mut()
            .get_object_mut(object_id)
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        dict.set(key, Object::Name(value.as_bytes().to_vec()));
        Ok(())
    }

    fn set_dict_string(&mut self, object_id: (u32, u16), key: &str, value: &str) -> Result<()> {
        let obj = self
            .inner_mut()
            .get_object_mut(object_id)
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        dict.set(
            key,
            Object::string_literal(crate::edit::encode_pdf_literal(value)),
        );
        Ok(())
    }

    fn set_widget_appearance(&mut self, widget_id: (u32, u16), stream: ObjectId) -> Result<()> {
        let obj = self
            .inner_mut()
            .get_object_mut(widget_id)
            .map_err(|e| Error::Pdf(format!("widget : {e}")))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("widget : {e}")))?;
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(stream));
        dict.set("AP", Object::Dictionary(ap));
        Ok(())
    }
}
