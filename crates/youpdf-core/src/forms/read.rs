use lopdf::{Dictionary, Object};

use crate::document::Document;

use super::{appearance_states, field_options, field_rect, field_value, number_f32, FieldProperties, FormField, FormFieldKind};

/// Attributs qu'un champ transmet à ses sous-champs.
#[derive(Debug, Clone, Default)]
struct Inherited {
    ft: Option<Vec<u8>>,
    flags: i64,
    value: Option<String>,
    options: Option<Vec<String>>,
    tooltip: Option<String>,
    page: Option<usize>,
    rect: Option<[f32; 4]>,
}

impl Document {
    /// Lit tous les champs de formulaire du document.
    ///
    /// Un champ peut hériter de `/FT`, `/Ff`, `/V`, `/Opt` et `/TU` de son
    /// parent, et ses widgets peuvent être des objets séparés sans `/T` :
    /// chaque widget devient une entrée avec son propre rectangle et sa page.
    pub fn read_form_fields(&self) -> Vec<FormField> {
        let mut out = Vec::new();

        let Some(acroform) = self.acroform_dict() else {
            return out;
        };
        // `/Fields` peut être un tableau direct ou une référence : les
        // deux formes sont valides, et beaucoup de producteurs choisissent
        // la seconde.
        let fields: Vec<Object> = match acroform.get(b"Fields") {
            Ok(Object::Array(items)) => items.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_object(*id)
                .ok()
                .and_then(|obj| obj.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => return out,
        };

        for field_ref in &fields {
            let Ok(field_id) = field_ref.as_reference() else {
                continue;
            };
            self.walk_field(field_id, None, &Inherited::default(), &mut out);
        }

        out
    }

    /// Parcourt un champ, ses sous-champs et ses widgets.
    fn walk_field(
        &self,
        field_id: (u32, u16),
        parent_name: Option<&str>,
        inherited: &Inherited,
        out: &mut Vec<FormField>,
    ) {
        let Ok(obj) = self.inner().get_object(field_id) else {
            return;
        };
        let Ok(dict) = obj.as_dict() else {
            return;
        };

        // Nom local du champ. Les noms sont encodés comme le reste : WinAnsi
        // ou UTF-16BE, pas forcément de l'UTF-8.
        let local_name = dict
            .get(b"T")
            .ok()
            .and_then(|o| o.as_str().ok())
            .map(crate::structure::decode_pdf_text);

        let full_name = match (parent_name, &local_name) {
            (Some(p), Some(n)) => format!("{p}.{n}"),
            (None, Some(n)) => n.clone(),
            (Some(p), None) => p.to_string(),
            (None, None) => String::new(),
        };

        // Attributs hérités : le dictionnaire le plus proche gagne.
        let ft = dict
            .get(b"FT")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(<[u8]>::to_vec)
            .or_else(|| inherited.ft.clone());
        let flags = dict
            .get(b"Ff")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(inherited.flags);
        let value = field_value(dict.get(b"V").ok()).or_else(|| inherited.value.clone());
        let options = field_options(dict.get(b"Opt").ok())
            .or_else(|| inherited.options.clone())
            .unwrap_or_default();
        let tooltip = dict
            .get(b"TU")
            .ok()
            .and_then(|o| o.as_str().ok())
            .map(crate::structure::decode_pdf_text)
            .or_else(|| inherited.tooltip.clone())
            .unwrap_or_default();
        let page = dict
            .get(b"P")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .and_then(|page_id| self.page_number_of(page_id).map(|num| num as usize))
            .or(inherited.page);
        let rect = field_rect(dict).or(inherited.rect);

        // Enfants : les vrais sous-champs portent un `/T` (ou ont leurs
        // propres enfants) ; les autres sont des widgets.
        let kids: Vec<(u32, u16)> = dict
            .get(b"Kids")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|items| items.iter().filter_map(|item| item.as_reference().ok()).collect())
            .unwrap_or_default();
        let mut child_fields = Vec::new();
        let mut widgets = Vec::new();
        for kid in kids {
            let Ok(kid_obj) = self.inner().get_object(kid) else {
                continue;
            };
            let Ok(kid_dict) = kid_obj.as_dict() else {
                continue;
            };
            if kid_dict.get(b"T").is_ok() || kid_dict.get(b"Kids").is_ok() {
                child_fields.push(kid);
            } else {
                widgets.push(kid);
            }
        }

        if !child_fields.is_empty() {
            let next = Inherited {
                ft,
                flags,
                value,
                options: Some(options),
                tooltip: Some(tooltip),
                page,
                rect,
            };
            for kid in child_fields {
                self.walk_field(kid, Some(&full_name), &next, out);
            }
            return;
        }

        let Some(ft) = ft else {
            return;
        };
        if full_name.is_empty() {
            return;
        }
        let mut kind = FormFieldKind::from_ft(&ft);
        if matches!(kind, FormFieldKind::Checkbox) {
            // Bit 16 : bouton radio ; bit 17 : bouton poussoir.
            if flags & 0x8000 != 0 {
                kind = FormFieldKind::Radio;
            } else if flags & 0x10000 != 0 {
                kind = FormFieldKind::Button;
            }
        }
        if matches!(kind, FormFieldKind::Choice) && flags & (1 << 20) != 0 {
            kind = FormFieldKind::MultiChoice;
        }

        let widgets = if widgets.is_empty() {
            vec![field_id]
        } else {
            widgets
        };
        for widget_id in widgets {
            let widget = self
                .inner()
                .get_object(widget_id)
                .ok()
                .and_then(|obj| obj.as_dict().ok())
                .unwrap_or(dict);
            let rect = field_rect(widget).or(rect).unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let page = widget
                .get(b"P")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|page_id| self.page_number_of(page_id).map(|num| num as usize))
                .or_else(|| self.page_of_annotation(widget_id))
                .or(page);
            out.push(FormField {
                object_id: field_id,
                widget_id,
                name: full_name.clone(),
                kind: kind.clone(),
                value: value.clone(),
                options: options.clone(),
                read_only: flags & 1 != 0,
                tooltip: tooltip.clone(),
                required: flags & 2 != 0,
                multiline: flags & 4096 != 0,
                page,
                rect,
                states: appearance_states(widget),
                state: widget
                    .get(b"AS")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .map(crate::structure::decode_pdf_text),
            });
        }
    }

    /// Page qui référence cette annotation dans son `/Annots`.
    fn page_of_annotation(&self, annot_id: (u32, u16)) -> Option<usize> {
        for (number, page_id) in self.inner().get_pages() {
            let Ok(page) = self.inner().get_object(page_id).and_then(|obj| obj.as_dict()) else {
                continue;
            };
            let Ok(annots) = page.get(b"Annots").and_then(|o| o.as_array()) else {
                continue;
            };
            if annots
                .iter()
                .any(|item| item.as_reference().ok() == Some(annot_id))
            {
                return Some(number as usize);
            }
        }
        None
    }
    /// Récupère le dictionnaire `/AcroForm` du catalogue.
    pub(super) fn acroform_dict(&self) -> Option<&Dictionary> {
        let catalog_id = self
            .inner()
            .trailer
            .get(b"Root")
            .ok()?
            .as_reference()
            .ok()?;
        let catalog = self.inner().get_object(catalog_id).ok()?.as_dict().ok()?;
        let acro_ref = catalog.get(b"AcroForm").ok()?;
        match acro_ref {
            Object::Reference(id) => self.inner().get_object(*id).ok()?.as_dict().ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        }
    }
}

/// Lit les réglages d'apparence, de visibilité et d'action d'un champ.
impl Document {
    /// Relit les propriétés éditables d'un champ depuis son dictionnaire.
    pub fn read_field_properties(&self, field: &FormField) -> FieldProperties {
        let mut props = FieldProperties::from_field(field);
        let Ok(obj) = self.inner().get_object(field.object_id) else {
            return props;
        };
        let Ok(dict) = obj.as_dict() else {
            return props;
        };

        let flags = dict
            .get(b"Ff")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        props.password = flags & (1 << 13) != 0;
        props.comb = if flags & (1 << 24) != 0 {
            dict.get(b"MaxLen")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0)
                .clamp(0, 20) as u8
        } else {
            0
        };
        props.editable = flags & (1 << 18) != 0;
        props.alignment = dict
            .get(b"Q")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0)
            .clamp(0, 2) as u8;

        let field_flags = dict
            .get(b"F")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        props.visibility = if field_flags & 1 != 0 {
            if field_flags & 2 != 0 {
                3
            } else {
                1
            }
        } else if field_flags & 32 != 0 {
            2
        } else {
            0
        };

        props.default_value = dict
            .get(b"DV")
            .ok()
            .and_then(|o| match o {
                Object::String(bytes, _) => Some(crate::structure::decode_pdf_text(bytes)),
                Object::Name(name) => Some(crate::structure::decode_pdf_text(name)),
                _ => None,
            })
            .unwrap_or_default();

        if let Ok(mk) = dict.get(b"MK").and_then(|o| o.as_dict()) {
            if let Ok(bc) = mk.get(b"BC").and_then(|o| o.as_array()) {
                let channels: Vec<u8> = bc
                    .iter()
                    .filter_map(number_f32)
                    .map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8)
                    .collect();
                if channels.len() >= 3 {
                    props.border_color = [channels[0], channels[1], channels[2]];
                }
            }
            if let Ok(bg) = mk.get(b"BG").and_then(|o| o.as_array()) {
                let channels: Vec<u8> = bg
                    .iter()
                    .filter_map(number_f32)
                    .map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8)
                    .collect();
                if channels.len() >= 3 {
                    props.fill_color = [channels[0], channels[1], channels[2]];
                    props.fill = true;
                }
            }
        }

        if let Ok(bs) = dict.get(b"BS").and_then(|o| o.as_dict()) {
            props.border_width = bs
                .get(b"W")
                .ok()
                .and_then(number_f32)
                .unwrap_or(1.0);
            props.border_style = match bs.get(b"S").ok().and_then(|o| o.as_name().ok()) {
                Some(b"D") => 1,
                Some(b"B") => 2,
                Some(b"I") => 3,
                Some(b"U") => 4,
                _ => 0,
            };
        }

        if let Ok(da) = dict.get(b"DA").and_then(|o| o.as_str()) {
            let text = String::from_utf8_lossy(da);
            let parts: Vec<&str> = text.split_whitespace().collect();
            if let Some(size) = parts
                .iter()
                .position(|part| *part == "Tf")
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| parts.get(index))
                .and_then(|value| value.parse::<f32>().ok())
            {
                props.font_size = size;
            }
            if let Some(index) = parts.iter().position(|part| *part == "rg") {
                let channels: Vec<f32> = parts[index.saturating_sub(3)..index]
                    .iter()
                    .filter_map(|value| value.parse::<f32>().ok())
                    .collect();
                if channels.len() == 3 {
                    props.text_color = [
                        (channels[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                        (channels[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                        (channels[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                    ];
                }
            }
        }

        if let Ok(aa) = dict.get(b"AA").and_then(|o| o.as_dict()) {
            let script = |key: &[u8]| -> String {
                aa.get(key)
                    .ok()
                    .and_then(|action| action.as_dict().ok())
                    .and_then(|action| action.get(b"JS").ok())
                    .and_then(|js| js.as_str().ok())
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                    .unwrap_or_default()
            };
            props.action_js = script(b"E");
            props.format_js = script(b"F");
            props.calc_js = script(b"C");
        }

        props
    }
}
