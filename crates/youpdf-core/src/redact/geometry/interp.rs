//! Interprétation des opérateurs : géométrie peinte par instruction.

use lopdf::content::Operation;
use lopdf::Object;

use super::*;

impl Interpreter {
    /// Interprète une instruction et renvoie sa géométrie.
    ///
    /// Renvoie `Some` **uniquement** pour les instructions qui peignent
    /// quelque chose ; toutes les autres ne font que modifier l'état et
    /// renvoient `None`.
    pub fn interpret(&mut self, operation: &Operation) -> Option<OpGeometry> {
        let operator = operation.operator.as_str();
        match operator {
            // --- État graphique -------------------------------------------------
            "q" => {
                self.stack.push(self.state.clone());
                None
            }
            "Q" => {
                if let Some(state) = self.stack.pop() {
                    self.state = state;
                }
                None
            }
            "cm" => {
                if let Some(m) = matrix_of(&operation.operands) {
                    // L'opérande s'applique *avant* la CTM en vigueur : d'où
                    // le produit par la gauche.
                    self.state.ctm = m.then(self.state.ctm);
                }
                None
            }
            "w" => {
                if let Some(v) = operand_number(&operation.operands, 0) {
                    self.state.line_width = v;
                }
                None
            }
            // `gs` (état graphique étendu) peut changer l'alpha ou le
            // rendu, jamais la géométrie : il n'affecte pas la position
            // des dessins et n'impose donc pas de rasterisation.
            "gs" => None,

            // --- État de texte ---------------------------------------------------
            "BT" => {
                // `BT` remet les deux matrices de texte à l'identité.
                self.text.matrix = Matrix::IDENTITY;
                self.text.line_matrix = Matrix::IDENTITY;
                None
            }
            "ET" => None,
            "Tf" => {
                if let Some(name) = operation.operands.first().and_then(|o| o.as_name().ok()) {
                    self.text.font = Some(String::from_utf8_lossy(name).to_string());
                }
                if let Some(size) = operand_number(&operation.operands, 1) {
                    self.text.size = size;
                }
                None
            }
            "Tc" => {
                if let Some(v) = operand_number(&operation.operands, 0) {
                    self.text.char_spacing = v;
                }
                None
            }
            "Tw" => {
                if let Some(v) = operand_number(&operation.operands, 0) {
                    self.text.word_spacing = v;
                }
                None
            }
            "Tz" => {
                // Échelle horizontale en pourcentage.
                if let Some(v) = operand_number(&operation.operands, 0) {
                    self.text.hscale = v / 100.0;
                }
                None
            }
            "Ts" => {
                // Élévation du texte, exprimée ici en pourcentage.
                if let Some(v) = operand_number(&operation.operands, 0) {
                    self.text.vscale = v / 100.0;
                }
                None
            }
            "TL" => {
                if let Some(v) = operand_number(&operation.operands, 0) {
                    self.text.leading = v;
                }
                None
            }
            "Td" => {
                let tx = operand_number(&operation.operands, 0).unwrap_or(0.0);
                let ty = operand_number(&operation.operands, 1).unwrap_or(0.0);
                self.move_to_next_line(tx, ty);
                None
            }
            "TD" => {
                let tx = operand_number(&operation.operands, 0).unwrap_or(0.0);
                let ty = operand_number(&operation.operands, 1).unwrap_or(0.0);
                // `TD` vaut `TD` + interligne valant `-ty`.
                self.text.leading = -ty;
                self.move_to_next_line(tx, ty);
                None
            }
            "Tm" => {
                if let Some(m) = matrix_of(&operation.operands) {
                    self.text.matrix = m;
                    self.text.line_matrix = m;
                }
                None
            }
            "T*" => {
                self.next_line();
                None
            }
            "Tj" => {
                let text = operation
                    .operands
                    .first()
                    .map(|o| o.as_str().unwrap_or_default())
                    .unwrap_or_default();
                Some(OpGeometry {
                    kind: OpKind::Text,
                    bbox: self.show_text(text),
                    filled: true,
                    stroked: false,
                    plain_rect: false,
                })
            }
            "TJ" => {
                let array = operation
                    .operands
                    .first()
                    .and_then(|o| o.as_array().ok())
                    .cloned()
                    .unwrap_or_default();
                let mut bbox = BBox::EMPTY;
                for item in array.iter() {
                    match item {
                        Object::String(bytes, _) => {
                            let box_ = self.show_text(bytes);
                            bbox = union(&bbox, &box_);
                        }
                        Object::Name(_) => {}
                        other => {
                            // Ajustement numérique : déplacement du stylo.
                            if let Some(v) = number_of(other) {
                                let shift = -v / 1000.0 * self.text.size * self.text.hscale;
                                self.text.matrix = translation(shift, 0.0).then(self.text.matrix);
                            }
                        }
                    }
                }
                if bbox.is_empty() {
                    None
                } else {
                    Some(OpGeometry {
                        kind: OpKind::Text,
                        bbox,
                        filled: true,
                        stroked: false,
                        plain_rect: false,
                    })
                }
            }
            "'" => {
                self.next_line();
                let text = operation
                    .operands
                    .first()
                    .map(|o| o.as_str().unwrap_or_default())
                    .unwrap_or_default();
                Some(OpGeometry {
                    kind: OpKind::Text,
                    bbox: self.show_text(text),
                    filled: true,
                    stroked: false,
                    plain_rect: false,
                })
            }
            "\"" => {
                // Interligne, puis espacement des mots et des caractères.
                self.next_line();
                if let Some(v) = operand_number(&operation.operands, 0) {
                    self.text.word_spacing = v;
                }
                if let Some(v) = operand_number(&operation.operands, 1) {
                    self.text.char_spacing = v;
                }
                let text = operation
                    .operands
                    .get(2)
                    .map(|o| o.as_str().unwrap_or_default())
                    .unwrap_or_default();
                Some(OpGeometry {
                    kind: OpKind::Text,
                    bbox: self.show_text(text),
                    filled: true,
                    stroked: false,
                    plain_rect: false,
                })
            }
            // --- Construction de chemin ------------------------------------------
            "m" | "l" | "c" | "v" | "y" | "re" | "h" => {
                self.build_path(operator, &operation.operands);
                None
            }
            // --- Peinture de chemin ----------------------------------------------
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => self.paint(operator),
            // --- XObject -----------------------------------------------------------
            "Do" => self.image_geometry(),
            _ => None,
        }
    }
}
impl Interpreter {
    /// Déplace la ligne courante de `(tx, ty)` (espace de texte).
    ///
    /// `Td` et `TD` partagent ce comportement : la matrice de ligne est
    /// translatée, puis la matrice de texte est alignée sur elle.
    fn move_to_next_line(&mut self, tx: f32, ty: f32) {
        self.text.line_matrix = translation(tx, ty).then(self.text.line_matrix);
        self.text.matrix = self.text.line_matrix;
    }

    /// Passe à la ligne suivante (interligne négatif vers le bas).
    fn next_line(&mut self) {
        let leading = self.text.leading;
        self.text.line_matrix = translation(0.0, -leading).then(self.text.line_matrix);
        self.text.matrix = self.text.line_matrix;
    }

    /// Affiche une chaîne et renvoie la boîte des glyphes dessinés.
    ///
    /// La boîte est calculée en espace de texte — hauteur d'ascendante
    /// (`0.75 em`) et de descendante (`0.25 em`) autour de la ligne de
    /// base — puis transformée par la matrice de texte et la CTM.
    fn show_text(&mut self, bytes: &[u8]) -> BBox {
        let decoded = decode_text(bytes);
        let glyphs = decoded.chars().count();
        let spaces = decoded.chars().filter(|c| *c == ' ').count();

        let size = self.text.size;
        let font = self.text.font.clone().unwrap_or_default();
        // Largeur d'encre : c'est elle qui détermine la boîte visible.
        let ink = estimate_width(&decoded, size, &font);
        // Le stylo avance de la largeur d'encre, plus les espacements, le
        // tout mis à l'échelle horizontale.
        let advance =
            (ink + self.text.char_spacing * glyphs as f32 + self.text.word_spacing * spaces as f32)
                * self.text.hscale;
        let ink_scaled = ink * self.text.hscale;

        let (rise, vscale) = self.vertical_metrics();
        let descent = 0.25 * size * vscale;
        let ascent = 0.75 * size * vscale;

        // Le stylo est à l'origine de la matrice de texte : la course
        // commence donc en `x = 0` de l'espace de texte.
        let m = self.text.matrix.then(self.state.ctm);
        let mut bbox = BBox::EMPTY;
        bbox.extend(
            m.apply(0.0, -descent + rise).0,
            m.apply(0.0, -descent + rise).1,
        );
        let p1 = m.apply(ink_scaled, -descent + rise);
        let p2 = m.apply(ink_scaled, ascent + rise);
        let p3 = m.apply(0.0, ascent + rise);
        bbox.extend(p1.0, p1.1);
        bbox.extend(p2.0, p2.1);
        bbox.extend(p3.0, p3.1);

        // Le stylo avance de la largeur totale.
        self.text.matrix = translation(advance, 0.0).then(self.text.matrix);

        bbox
    }

    /// Interprétation de `Ts`, dont la valeur est ambiguë dans les flux.
    ///
    /// La spécification PDF définit `Ts` comme une *élévation* en points ;
    /// certains générateurs l'utilisent comme une échelle verticale. On
    /// distingue les deux cas : une valeur proche de 100 % est une échelle,
    /// une valeur faible est une élévation (surscript, indice). Renvoie
    /// l'élévation et l'échelle verticale effectives.
    fn vertical_metrics(&self) -> (f32, f32) {
        let v = self.text.vscale;
        if !v.is_finite() {
            return (0.0, 1.0);
        }
        if v >= 0.5 {
            (0.0, v)
        } else {
            (v * 100.0, 1.0)
        }
    }
}
impl Interpreter {
    /// Accumule la géométrie d'un chemin en cours de construction.
    ///
    /// Les points sont transformés par la CTM **au moment de leur saisie** :
    /// c'est l'espace utilisateur en vigueur qui définit le chemin, et une
    /// instruction `cm` ultérieure ne doit pas le déformer.
    fn build_path(&mut self, operator: &str, operands: &[Object]) {
        let ctm = self.state.ctm;
        match operator {
            "m" | "l" => {
                let x = operand_number(operands, 0).unwrap_or(self.current.0);
                let y = operand_number(operands, 1).unwrap_or(self.current.1);
                if operator == "m" {
                    self.path_open = true;
                }
                self.current = (x, y);
                let p = ctm.apply(x, y);
                self.path.push(p);
            }
            "c" => {
                // Les deux points de contrôle comptent aussi : une Bézier
                // peut déborder hors de ses extrémités.
                for i in 0..3 {
                    let x = operand_number(operands, i * 2).unwrap_or(self.current.0);
                    let y = operand_number(operands, i * 2 + 1).unwrap_or(self.current.1);
                    let p = ctm.apply(x, y);
                    self.path.push(p);
                }
                if let (Some(x), Some(y)) =
                    (operand_number(operands, 4), operand_number(operands, 5))
                {
                    self.current = (x, y);
                }
            }
            "v" => {
                // Premier point de contrôle = point courant.
                let p = ctm.apply(self.current.0, self.current.1);
                self.path.push(p);
                for i in 0..2 {
                    let x = operand_number(operands, i * 2).unwrap_or(self.current.0);
                    let y = operand_number(operands, i * 2 + 1).unwrap_or(self.current.1);
                    let p = ctm.apply(x, y);
                    self.path.push(p);
                }
                if let (Some(x), Some(y)) =
                    (operand_number(operands, 2), operand_number(operands, 3))
                {
                    self.current = (x, y);
                }
            }
            "y" => {
                // Second point de contrôle = point courant.
                let p = ctm.apply(self.current.0, self.current.1);
                self.path.push(p);
                let x = operand_number(operands, 0).unwrap_or(self.current.0);
                let y = operand_number(operands, 1).unwrap_or(self.current.1);
                let p = ctm.apply(x, y);
                self.path.push(p);
                self.current = (x, y);
            }
            "re" => {
                let x = operand_number(operands, 0).unwrap_or(0.0);
                let y = operand_number(operands, 1).unwrap_or(0.0);
                let w = operand_number(operands, 2).unwrap_or(0.0);
                let h = operand_number(operands, 3).unwrap_or(0.0);
                // Le rectangle d'origine est conservé en repère utilisateur :
                // c'est le seul tracé que l'appelant peut découper en
                // rectangles plus petits.
                self.last_rect = Some([x, y, x + w, y + h]);
                for corner in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
                    let p = ctm.apply(corner.0, corner.1);
                    self.path.push(p);
                }
                self.current = (x, y);
                self.path_open = true;
            }
            "h" => {
                // Fermeture : le sous-chemin rejoint le point de départ.
                self.path_open = false;
            }
            _ => {}
        }
    }

    /// Peint (ou abandonne) le chemin courant et renvoie sa géométrie.
    fn paint(&mut self, operator: &str) -> Option<OpGeometry> {
        let mut bbox = BBox::EMPTY;
        for (x, y) in self.path.iter() {
            bbox.extend(*x, *y);
        }
        // Après une instruction de peinture, le chemin est vidé.
        self.path.clear();
        self.path_open = false;

        let filled = matches!(operator, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
        let stroked = matches!(operator, "S" | "s" | "B" | "B*" | "b" | "b*");

        if stroked {
            // Le trait déborde d'une demi-épaisseur de chaque côté.
            let ctm = self.state.ctm;
            let scale = ctm.scale_x().max(ctm.scale_y());
            let half = 0.5 * self.state.line_width * scale;
            bbox.inflate(half);
        }

        if bbox.is_empty() {
            return None;
        }

        Some(OpGeometry {
            kind: OpKind::Path,
            bbox,
            filled,
            stroked,
            // `plain_rect` n'est vrai que pour un `re` *en construction* :
            // ici le chemin est peint, l'appelant décidera du découpage.
            plain_rect: false,
        })
    }

    /// Géométrie d'un XObject invoqué par `Do`.
    ///
    /// Un XObject image peint le carré unité `[0, 1]²` transformé par la
    /// CTM. Un XObject de formulaire peint son propre contenu : ce
    /// contenu n'est pas découpable, l'appelant décide de le rasteriser.
    /// On renvoie dans les deux cas la boîte de placement.
    fn image_geometry(&mut self) -> Option<OpGeometry> {
        let ctm = self.state.ctm;
        let mut bbox = BBox::EMPTY;
        for corner in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            let p = ctm.apply(corner.0, corner.1);
            bbox.extend(p.0, p.1);
        }
        if bbox.is_empty() {
            return None;
        }
        Some(OpGeometry {
            kind: OpKind::Image,
            bbox,
            filled: true,
            stroked: false,
            plain_rect: false,
        })
    }
}
/// Lit l'opérande numérique à l'indice donné, s'il existe.
fn operand_number(operands: &[Object], index: usize) -> Option<f32> {
    operands.get(index).and_then(number_of)
}

/// Fusionne deux boîtes en englobant les deux.
fn union(a: &BBox, b: &BBox) -> BBox {
    if a.is_empty() {
        return *b;
    }
    if b.is_empty() {
        return *a;
    }
    BBox {
        x1: a.x1.min(b.x1),
        y1: a.y1.min(b.y1),
        x2: a.x2.max(b.x2),
        y2: a.y2.max(b.y2),
    }
}

/// Décode une chaîne PDF en texte lisible.
fn decode_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        // Chaîne UTF-16 big-endian avec BOM.
        let mut out = String::new();
        let mut index = 2;
        while index + 1 < bytes.len() {
            let unit = u16::from_be_bytes([bytes[index], bytes[index + 1]]);
            out.push(char::from_u32(unit as u32).unwrap_or('\u{FFFD}'));
            index += 2;
        }
        return out;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        // Chaîne UTF-16 little-endian avec BOM.
        let mut out = String::new();
        let mut index = 2;
        while index + 1 < bytes.len() {
            let unit = u16::from_le_bytes([bytes[index], bytes[index + 1]]);
            out.push(char::from_u32(unit as u32).unwrap_or('\u{FFFD}'));
            index += 2;
        }
        return out;
    }
    // PDFDocEncoding : les octets sont pour l'essentiel de l'ASCII.
    bytes.iter().map(|b| *b as char).collect()
}

/// Opérateurs que l'interpréteur sait traiter.
///
/// Tout opérateur **en dehors** de cette liste peut peindre quelque chose
/// que l'interpréteur est incapable de localiser : images en ligne
/// (`BI`/`ID`/`EI`), dégradés (`sh`), métriques de glyphes (`d0`/`d1`), etc.
/// L'appelant doit alors rasteriser toute la page plutôt que de risquer de
/// laisser un secret visible.
pub const KNOWN_OPERATORS: &[&str] = &[
    "q", "Q", "cm", "w", "gs", "BT", "ET", "Tf", "Tc", "Tw", "Tz", "Ts", "TL", "Td", "TD", "Tm",
    "T*", "Tj", "TJ", "'", "\"", "m", "l", "c", "v", "y", "re", "h", "S", "s", "f", "F", "f*", "B",
    "B*", "b", "b*", "n", "Do",
];
