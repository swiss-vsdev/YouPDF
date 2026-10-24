//! Remplacement de texte par « patch » : masque + texte réécrit par-dessus.

use lopdf::{Dictionary, Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::MIN_TEXT_SIZE;
use crate::edit::estimate_text_width;
use crate::edit::util::encode_pdf_literal;
use crate::edit::{MAX_TEXT_SIZE, TextFragment, TextAlign, TextStyle, patch_font_name};

impl Document {
    /// Remplace un fragment de texte par un nouveau contenu.
    ///
    /// `background` est la couleur du rectangle de masquage (composantes
    /// 0.0–1.0) ; `None` donne du blanc. Un appelant qui connaît la couleur
    /// réelle de la page doit la fournir, sinon le patch se voit sur une
    /// page teintée.
    ///
    /// ## Méthode
    ///
    /// 1. On dessine un rectangle de masquage sur l'ancien fragment.
    /// 2. On écrit le nouveau texte au même emplacement, avec Helvetica,
    ///    dans la couleur du fragment (`TextFragment::color`).
    ///
    /// ## Limites
    ///
    /// - La police d'origine n'est pas reproduite : on utilise
    ///   Helvetica, l'une des 14 polices standard PDF.
    /// - Le style (gras, italique) n'est pas préservé.
    /// - Le texte n'est pas ré-encodable dans le flux d'origine : il est
    ///   écrit par-dessus. Le contenu original reste présent dans le PDF
    ///   (récupérable par extraction), ce qui **doit** être su si le but
    ///   est de supprimer une information sensible (utiliser le
    ///   caviardage pour ça).
    pub fn replace_text_fragment(&mut self, fragment: &TextFragment, new_text: &str, background: Option<[f32; 3]>) -> Result<()> {
        let style = TextStyle::from_fragment(fragment);
        self.replace_text_fragment_styled(fragment, new_text, background, &style)
    }

    /// Remplace un fragment de texte avec une mise en forme choisie.
    ///
    /// Mêmes masquage et emplacement que [`Document::replace_text_fragment`] ;
    /// la police, le corps, la couleur, les styles (gras, italique, souligné,
    /// barré), l'alignement, l'interligne et l'approche viennent de `style`.
    /// Les lignes sont séparées par `\n`. La police reste l'une des quatorze
    /// standard : le patch ne peut pas reprendre la police d'origine.
    pub fn replace_text_fragment_styled(
        &mut self,
        fragment: &TextFragment,
        new_text: &str,
        background: Option<[f32; 3]>,
        style: &TextStyle,
    ) -> Result<()> {
        self.replace_text_fragment_at(fragment, new_text, fragment.rect, background, style)
    }

    /// Réécrit un fragment de texte à un autre emplacement.
    ///
    /// `target` est la boîte où poser le nouveau texte, en points ; le fragment
    /// d'origine est masqué comme pour une réécriture sur place. C'est ce qui
    /// déplace ou redimensionne un bloc : le texte est réécrit, il n'est pas
    /// translaté dans le flux, donc la police d'origine est perdue (voir les
    /// limites de [`Document::replace_text_fragment`]).
    pub fn replace_text_fragment_at(
        &mut self,
        fragment: &TextFragment,
        new_text: &str,
        target: [f32; 4],
        background: Option<[f32; 3]>,
        style: &TextStyle,
    ) -> Result<()> {
        let page_id = self
            .page_id(fragment.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        // 1. Préparer le flux de patch.
        let [x1, y1, x2, y2] = target;
        let w = x2 - x1;

        // Le flux est construit en **octets** : un `String` intermédiaire puis
        // `into_bytes()` ré-encoderait chaque octet WinAnsi
        // >= 0x80 en deux octets UTF-8, ce qui reproduirait exactement le
        // défaut d'affichage des accents que ce correctif supprime.
        let mut content: Vec<u8> = Vec::new();

        // 2. Géométrie des lignes. La première garde la ligne de base du
        //    fragment d'origine ; les suivantes descendent d'un interligne.
        //    Le masque couvre donc tout ce qui sera écrit, sinon la seconde
        //    ligne se poserait sur le contenu de la page.
        let size = style.size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
        let ink = [
            style.color[0] as f32 / 255.0,
            style.color[1] as f32 / 255.0,
            style.color[2] as f32 / 255.0,
        ];
        let base = style.font.base_font(style.bold, style.italic);
        let font_name = patch_font_name(base);
        let line_step = size * style.line_spacing.max(0.5);
        let first_baseline = y1 + size * 0.2;
        // Un `\r` de fin de ligne (texte venu d'un champ CRLF) ne fait pas
        // partie du texte : il serait écrit comme une espace.
        let lines: Vec<&str> = new_text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect();
        let line_count = lines.len().max(1) as f32;
        let last_baseline = first_baseline - (line_count - 1.0) * line_step;
        let mask_bottom = (last_baseline - size * 0.3).min(y1) - 1.0;

        // Rectangle de masquage : couleur contrôlée, par défaut blanc. Il
        // couvre l'emplacement d'origine **et** le nouveau : sans cela, un bloc
        // déplacé laisserait son ancien texte derrière lui. Quand les deux
        // coïncident, la réunion redonne exactement l'ancien rectangle.
        let bg = background.unwrap_or([1.0, 1.0, 1.0]);
        let [ox1, oy1, ox2, oy2] = fragment.rect;
        let mask_x1 = (ox1 - 1.0).min(x1 - 1.0);
        let mask_y1 = (oy1 - 1.0).min(mask_bottom);
        let mask_x2 = (ox2 + 1.0).max(x2 + 1.0);
        let mask_y2 = (oy2 + 1.0).max(y2 + 1.0);
        content.extend_from_slice(
            format!("q\n{:.2} {:.2} {:.2} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\nQ\n",
                bg[0],
                bg[1],
                bg[2],
                mask_x1,
                mask_y1,
                mask_x2 - mask_x1,
                (mask_y2 - mask_y1).max(1.0))
            .as_bytes(),
        );

        // 3. Texte de remplacement. La couleur est reposée explicitement :
        //    après le `Q` du masque, la couleur de remplissage courante est
        //    celle qu'avait laissée le contenu de la page, qui n'a rien à voir.
        for (line_index, line) in lines.iter().enumerate() {
            let width = estimate_text_width(line, style);
            let dx = match style.align {
                TextAlign::Left | TextAlign::Justify => 0.0,
                TextAlign::Center => ((w - width) / 2.0).max(0.0),
                TextAlign::Right => (w - width).max(0.0),
            };
            let baseline = first_baseline - line_index as f32 * line_step;
            let x = x1 + dx;
            // L'approche n'est écrite que si elle est demandée : un patch sans
            // réglage garde le flux qu'il produisait avant l'inspecteur.
            let spacing = if style.char_spacing == 0.0 {
                String::new()
            } else {
                format!("{:.2} Tc\n", style.char_spacing)
            };
            // Justification : les mots sont répartis sur la largeur de la
            // boîte. Une ligne d'un seul mot, ou plus large que la boîte,
            // reste à gauche.
            let words: Vec<&str> = line.split(' ').filter(|word| !word.is_empty()).collect();
            let justify =
                style.align == TextAlign::Justify && words.len() > 1 && width < w;
            content.extend_from_slice(
                format!(
                    "BT\n{:.3} {:.3} {:.3} rg\n/{} {:.2} Tf\n{spacing}",
                    ink[0], ink[1], ink[2], font_name, size
                )
                .as_bytes(),
            );
            // Largeur réellement couverte : toute la boîte quand la ligne est
            // justifiée, la largeur estimée sinon.
            let rule_width = if justify {
                let natural: f32 = words
                    .iter()
                    .map(|word| estimate_text_width(word, style))
                    .sum();
                let gap = ((w - natural) / (words.len() - 1) as f32).max(0.0);
                let mut cursor = x;
                for (index, word) in words.iter().enumerate() {
                    if index > 0 {
                        cursor += gap;
                    }
                    content.extend_from_slice(
                        format!("1 0 0 1 {cursor:.2} {baseline:.2} Tm\n").as_bytes(),
                    );
                    content.push(b'(');
                    content.extend_from_slice(&encode_pdf_literal(word));
                    content.extend_from_slice(b") Tj\n");
                    cursor += estimate_text_width(word, style);
                }
                w
            } else {
                content.extend_from_slice(
                    format!("{x:.2} {baseline:.2} Td\n").as_bytes(),
                );
                content.push(b'(');
                content.extend_from_slice(&encode_pdf_literal(line));
                content.extend_from_slice(b") Tj\n");
                width
            };
            content.extend_from_slice(b"ET\n");

            // Souligné et barré se dessinent : le format texte de PDF n'a pas
            // de trait sous le glyphe. Une ligne vide n'a rien à souligner :
            // un trait d'un point trahirait le patch.
            let rule = (size * 0.07).max(0.5);
            let ruled = width > 0.1;
            if style.underline && ruled {
                let y = baseline - size * 0.14;
                content.extend_from_slice(
                    format!(
                        "q\n{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\nQ\n",
                        ink[0], ink[1], ink[2], x, y, rule_width.max(1.0), rule
                    )
                    .as_bytes(),
                );
            }
            if style.strikeout && ruled {
                let y = baseline + size * 0.28;
                content.extend_from_slice(
                    format!(
                        "q\n{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\nQ\n",
                        ink[0], ink[1], ink[2], x, y, rule_width.max(1.0), rule
                    )
                    .as_bytes(),
                );
            }
        }

        let patch = Stream::new(Dictionary::new(), content);
        let patch_id = self.inner_mut().add_object(patch);

        // 3. Police dédiée aux patchs (police standard + WinAnsi), indépendante
        //    des polices déjà présentes sur la page.
        self.ensure_patch_font(page_id, base)?;

        // 4. Concaténer le patch au flux de contenu.
        self.append_to_page_content(page_id, patch_id)?;

        Ok(())
    }

    /// Garantit qu'une police standard est disponible dans le document.
    ///
    /// Le nom de ressource dépend du fichier de base : chaque variante
    /// (gras, italique, famille) garde la sienne, sinon la seconde écraserait
    /// la première. Réutilise la police si elle est déjà présente.
    /// Nom de ressource de la police posée par les patchs YouPDF.
    ///
    /// On n'utilise **jamais** le `/F1` de la page : cette police peut être
    /// une Typ1 sans `/Encoding` (StandardEncoding), une police sous-ensemblée
    /// en `Identity-H` (UTF-16) ou toute autre encodage. Y écrire des octets
    /// WinAnsi produit du texte illisible. Une police dédiée garantit
    /// l'encodage et n'écrase rien.
    fn ensure_patch_font(&mut self, page_id: (u32, u16), base_font: &str) -> Result<(u32, u16)> {
        self.ensure_patch_font_named(page_id, base_font).map(|(id, _)| id)
    }

    /// Même chose que [`Document::ensure_patch_font`], mais renvoie aussi le
    /// **nom de ressource** utilisable dans un flux de contenu.
    ///
    /// Le caviardage a besoin du nom (`/YouPdfF1`) pour écrire son texte
    /// superposé, pas seulement de l'identifiant d'objet.
    pub(crate) fn ensure_patch_font_named(
        &mut self,
        page_id: (u32, u16),
        base_font: &str,
    ) -> Result<((u32, u16), String)> {
        // Nom libre, ou nom déjà pris par **la même** variante. Le nom seul ne
        // suffit pas : un document peut porter `/YouPdfF1` sur un autre fichier
        // de base (patch d'une version antérieure, outil tiers). Dans ce cas on
        // prend un nom suffixé plutôt que d'écraser une police dont d'autres
        // textes se servent.
        let base_name = patch_font_name(base_font);
        let mut name = base_name.clone();
        let mut suffix = 1u32;
        loop {
            let existing = self
                .inner()
                .get_dictionary(page_id)
                .ok()
                .and_then(|page| {
                    page.get(b"Resources")
                        .ok()
                        .and_then(|resources| resources.as_dict().ok())
                })
                .and_then(|resources| {
                    resources
                        .get(b"Font")
                        .ok()
                        .and_then(|fonts| fonts.as_dict().ok())
                })
                .and_then(|fonts| fonts.get(name.as_bytes()).ok())
                .and_then(|object| object.as_reference().ok());
            let Some(id) = existing else {
                break;
            };
            let matching = self
                .inner()
                .get_dictionary(id)
                .ok()
                .and_then(|font| {
                    font.get(b"BaseFont")
                        .ok()
                        .and_then(|base| base.as_name().ok())
                        .map(|base| base == base_font.as_bytes())
                })
                .unwrap_or(false);
            if matching {
                return Ok((id, name));
            }
            suffix += 1;
            name = format!("{base_name}{suffix}");
        }

        let font = Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type1".to_vec())),
            ("BaseFont", Object::Name(base_font.as_bytes().to_vec())),
            ("Encoding", Object::Name(b"WinAnsiEncoding".to_vec())),
        ]);
        let font_id = self.inner_mut().add_object(Object::Dictionary(font));
        self.link_font_to_page(page_id, &name, font_id)?;
        Ok((font_id, name))
    }

    /// Enregistre une police dans les ressources `/Font` d'une page.
    fn link_font_to_page(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        font_id: (u32, u16),
    ) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        // Récupère ou crée /Resources.
        // (On clone d'abord l'objet pour libérer l'emprunt mutable sur
        // la page, ce qui permet ensuite d'accéder à `self.inner()`.)
        let resources_obj = dict.get(b"Resources").ok().cloned();
        let mut resources = match resources_obj {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(id)) => self
                .inner()
                .get_dictionary(id)
                .map_err(|e| Error::Pdf(format!("Ressources : {e}")))?
                .clone(),
            _ => Dictionary::new(),
        };

        // Récupère ou crée /Font.
        let mut fonts = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("Font : {e}")))?
                .clone(),
            _ => Dictionary::new(),
        };

        // Associe le nom à la police.
        fonts.set(name, Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(fonts));

        // Écrit dans la page.
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
    fn append_to_page_content(&mut self, page_id: (u32, u16), patch_id: (u32, u16)) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        match dict.get_mut(b"Contents") {
            Ok(Object::Reference(id)) => {
                // Contenu unique : on transforme en tableau.
                let existing = *id;
                dict.set(
                    "Contents",
                    Object::Array(vec![
                        Object::Reference(existing),
                        Object::Reference(patch_id),
                    ]),
                );
            }
            Ok(Object::Array(arr)) => {
                arr.push(Object::Reference(patch_id));
            }
            _ => {
                dict.set("Contents", Object::Reference(patch_id));
            }
        }

        Ok(())
    }
}
