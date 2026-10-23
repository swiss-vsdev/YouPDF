//! Géométrie, tampons et rendu des apparences.

use lopdf::{Dictionary, Object, Stream};

use super::types::*;

/// Quadrilatère `/QuadPoints` d'un rectangle : coins dans l'ordre
/// haut-gauche, haut-droit, bas-gauche, bas-droit.
pub fn quad_from_rect(x1: f32, y1: f32, x2: f32, y2: f32) -> [f32; 8] {
    [x1, y2, x2, y2, x1, y1, x2, y1]
}

/// Boîte englobante d'un quadrilatère `/QuadPoints` : `(x1, y1, x2, y2)`.
fn quad_bounds(quad: [f32; 8]) -> (f32, f32, f32, f32) {
    let mut x1 = f32::INFINITY;
    let mut y1 = f32::INFINITY;
    let mut x2 = f32::NEG_INFINITY;
    let mut y2 = f32::NEG_INFINITY;
    for point in quad.chunks_exact(2) {
        x1 = x1.min(point[0]);
        y1 = y1.min(point[1]);
        x2 = x2.max(point[0]);
        y2 = y2.max(point[1]);
    }
    (x1, y1, x2, y2)
}

/// Distance d'un point au segment `[a, b]` ; zéro s'il est dessus.
pub(crate) fn point_segment_distance(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [point[0] - a[0], point[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
    };
    let dx = point[0] - (a[0] + ab[0] * t);
    let dy = point[1] - (a[1] + ab[1] * t);
    (dx * dx + dy * dy).sqrt()
}

/// Rééchantillonne une polyligne par des splines Catmull-Rom.
///
/// Chaque segment est remplacé par `samples` points. La courbe passe par
/// tous les points d'origine : elle arrondit les angles sans déplacer le
/// tracé, ce qui supprime le tremblement de la souris.
pub fn smooth_polyline(points: &[[f32; 2]], samples: usize) -> Vec<[f32; 2]> {
    if points.len() < 3 || samples < 2 {
        return points.to_vec();
    }
    let last = points.len() - 1;
    let mut out = Vec::with_capacity(points.len() * samples);
    for i in 0..last {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(last)];
        for step in 0..samples {
            let t = step as f32 / samples as f32;
            let (t2, t3) = (t * t, t * t * t);
            let mut point = [0.0f32; 2];
            for (axis, slot) in point.iter_mut().enumerate() {
                *slot = 0.5
                    * (2.0 * p1[axis]
                        + (-p0[axis] + p2[axis]) * t
                        + (2.0 * p0[axis] - 5.0 * p1[axis] + 4.0 * p2[axis] - p3[axis]) * t2
                        + (-p0[axis] + 3.0 * p1[axis] - 3.0 * p2[axis] + p3[axis]) * t3);
            }
            out.push(point);
        }
    }
    out.push(points[last]);
    out
}

/// Modèle correspondant à un identifiant ; le premier par défaut.
pub fn stamp_template(id: &str) -> &'static StampTemplate {
    STAMP_TEMPLATES
        .iter()
        .find(|template| template.id == id)
        .unwrap_or(&STAMP_TEMPLATES[0])
}

/// Identifiant du modèle qui écrit un `/Name` donné.
///
/// Plusieurs modèles partagent un `/Name` (`Draft` sert à « Draft » et à
/// « For Comment ») : on renvoie le premier, la nuance n'existant qu'à
/// l'écran.
pub fn stamp_id_for_pdf_name(name: &str) -> Option<String> {
    STAMP_TEMPLATES
        .iter()
        .find(|template| template.pdf_name == name)
        .map(|template| template.id.to_string())
}

/// Texte d'un tampon dynamique : libellé, puis auteur et date s'ils sont
/// fournis. Une ligne vide n'est jamais ajoutée, pour qu'un tampon posé
/// sans auteur ne réserve pas de place dans sa boîte.
pub fn dynamic_stamp_text(template: &StampTemplate, author: &str, date: &str) -> String {
    let mut lines = vec![template.label.to_string()];
    if template.dynamic {
        let author = author.trim();
        if !author.is_empty() {
            lines.push(author.to_string());
        }
        let date = date.trim();
        if !date.is_empty() {
            lines.push(date.to_string());
        }
    }
    lines.join("\n")
}

/// Associe un libellé de tampon au nom standard PDF (`/Name`).
pub fn stamp_name(label: &str) -> &'static [u8] {
    match label.trim().to_ascii_lowercase().as_str() {
        "approuvé" | "approved" => b"Approved",
        "non approuvé" | "not approved" => b"NotApproved",
        "confidentiel" | "confidential" => b"Confidential",
        "final" => b"Final",
        "expiré" | "expired" => b"Expired",
        "pour commentaire" | "draft" | "brouillon" => b"Draft",
        "signer ici" | "sign here" => b"SignHere",
        "initial ici" | "initial here" => b"InitialHere",
        "témoin" | "witness" => b"Witness",
        _ => b"Draft",
    }
}

/// Date PDF (`D:AAAAMMJJHHmmSSZ`) pour l'entrée `/M`.
pub(crate) fn pdf_date(now: chrono::DateTime<chrono::Utc>) -> String {
    now.format("D:%Y%m%d%H%M%SZ").to_string()
}

/// Ajoute un littéral PDF `(…)` : échappé pour la syntaxe, encodé WinAnsi
/// pour la police standard.
pub(crate) fn push_text_literal(out: &mut Vec<u8>, text: &str) {
    out.push(b'(');
    out.extend_from_slice(&crate::edit::encode_pdf_literal(text));
    out.push(b')');
}

/// Boîte d'un flux de formulaire (`/BBox`). Sans boîte, la taille de
/// l'apparence est indéterminée.
pub(crate) fn form_bbox(stream: &Stream) -> Option<[f32; 4]> {
    let bbox = stream.dict.get(b"BBox").ok()?.as_array().ok()?;
    if bbox.len() != 4 {
        return None;
    }
    let mut out = [0.0f32; 4];
    for (i, obj) in bbox.iter().enumerate() {
        out[i] = obj.as_float().ok()?;
    }
    Some(out)
}

/// Apparence d'une annotation prête à peindre dans une page.
pub(crate) struct Paintable {
    /// Nom de ressource unique dans `/Resources /XObject`.
    pub(crate) name: String,
    /// Flux d'apparence (référence, ou copie si le flux était en ligne).
    pub(crate) stream: Object,
    /// Matrice `cm` qui amène la boîte du flux sur le `/Rect`.
    pub(crate) matrix: [f32; 6],
}

impl Annotation {
    /// Coin bas-gauche et dimensions du rectangle. Un `/Rect` PDF peut
    /// arriver avec ses coins dans n'importe quel ordre.
    pub fn frame(&self) -> (f32, f32, f32, f32) {
        (
            self.rect[0].min(self.rect[2]),
            self.rect[1].min(self.rect[3]),
            (self.rect[2] - self.rect[0]).abs(),
            (self.rect[3] - self.rect[1]).abs(),
        )
    }

    /// Quadrilatères à écrire et à peindre : ceux de la sélection, ou le
    /// rectangle comme quadrilatère unique.
    pub fn quads_or_rect(&self) -> Vec<[f32; 8]> {
        if self.quads.is_empty() {
            let [x1, y1, x2, y2] = self.rect;
            vec![quad_from_rect(x1, y1, x2, y2)]
        } else {
            self.quads.clone()
        }
    }

    /// Pose les quadrilatères d'une mise en évidence et ajuste `rect` à leur
    /// union, avec la même marge que le crayon.
    pub fn set_quads(&mut self, quads: Vec<[f32; 8]>) {
        let mut x1 = f32::INFINITY;
        let mut y1 = f32::INFINITY;
        let mut x2 = f32::NEG_INFINITY;
        let mut y2 = f32::NEG_INFINITY;
        for quad in &quads {
            let (bx1, by1, bx2, by2) = quad_bounds(*quad);
            x1 = x1.min(bx1);
            y1 = y1.min(by1);
            x2 = x2.max(bx2);
            y2 = y2.max(by2);
        }
        if x1.is_finite() {
            self.rect = [x1 - 2.0, y1 - 2.0, x2 + 2.0, y2 + 2.0];
        }
        self.quads = quads;
    }

    /// Contenu de l'apparence (`/AP /N`) dans le repère local de
    /// l'annotation : origine au coin bas-gauche du `Rect`, axe Y vers le
    /// haut, unités PDF. Le lecteur pose cette apparence sur le `Rect`.
    ///
    /// Renvoie `None` quand il n'y a rien à dessiner : un lien est
    /// invisible, et lecteurs dessinent eux-mêmes l'icône des notes et des
    /// pièces jointes. Sans apparence, les autres lecteurs n'afficheraient
    /// rien du tout pour les autres types.
    pub fn appearance_content(&self) -> Option<Vec<u8>> {
        let (x1, y1, w, h) = self.frame();
        if w <= 0.5 || h <= 0.5 {
            return None;
        }
        let [r, g, b] = self.style.color;
        let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        let stroke = format!("{rf:.3} {gf:.3} {bf:.3} RG");
        let fill = format!("{rf:.3} {gf:.3} {bf:.3} rg");
        let lw = self.style.line_width.clamp(0.5, 12.0);
        let gs = format!("q /{APPEARANCE_GS_NAME} gs ");
        let mut c: Vec<u8> = Vec::new();

        match self.kind {
            AnnotationKind::Link | AnnotationKind::Note | AnnotationKind::FileAttachment => {
                return None
            }

            AnnotationKind::Polygon | AnnotationKind::PolyLine => {
                // Fond transparent pour le polygone/polyligne (on se contente
                // du trait via BS). Un polygone rempli n'aurait pas de sens
                // en PDF, le même que rect fait sans bloc de remplissage.
                let lw = self.style.line_width.clamp(0.5, 12.0);
                let gs = format!("q /{APPEARANCE_GS_NAME} gs ");
                let (x1, y1, w, h) = self.frame();
                c.extend_from_slice(
                    format!("{gs}{stroke} {lw:.2} w 1 J 1 j {} {} {} {} re S Q\n", x1, y1, w - lw, h - lw).as_bytes(),
                );
                return Some(c);
            }

            AnnotationKind::Callout => {
                // Callout : flèche + texte, dessinée avec le trait (BS) et
                // un rectangle simple pour le fond du texte.
                let (x1, y1, w, h) = self.frame();
                let lw = self.style.line_width.clamp(0.5, 12.0);
                let gs = format!("q /{APPEARANCE_GS_NAME} gs ");
                let stroke = format!("{:.3} {:.3} {:.3} RG", self.style.color[0] as f32 / 255.0, self.style.color[1] as f32 / 255.0, self.style.color[2] as f32 / 255.0);
                let fill = format!("{:.3} {:.3} {:.3} rg", self.style.color[0] as f32 / 255.0, self.style.color[1] as f32 / 255.0, self.style.color[2] as f32 / 255.0);
                // Flèche : simple segment du point 1 au point 2 (on utilise
                // les trois points d'encre stockés).
                let amp = 2.0_f32.min(h / 3.0);
                c.extend_from_slice(
                    format!("{gs}{stroke} {lw:.2} w 1 J 1 j {} {} m {} {} l S Q\n", x1, y1, x1, y1 + amp).as_bytes(),
                );
                // Fond du texte : rectangle en bas.
                let text_y = h - lw - 12.0;
                c.extend_from_slice(
                    format!("q {fill} {} {} {} {} re S Q\n", x1, y1 + text_y, w - lw, text_y).as_bytes(),
                );
                // Étiquette de texte (simplifiée).
                let txt_w = 50.0_f32.max(w * 0.3);
                let txt_x = (w - txt_w) / 2.0;
                c.extend_from_slice(
                    format!("BT /Helv {:.2} Tf {} {} Td ({:.1} Tj) ET Q\n", self.style.font_size, txt_x, text_y + 3.0, "Callout").as_bytes(),
                );
            }

            // Une mise en évidence suit les lignes sélectionnées : un bloc par
            // quadrilatère, sinon un rectangle unique avalerait l'interligne.
            AnnotationKind::Highlight
            | AnnotationKind::Underline
            | AnnotationKind::StrikeOut
            | AnnotationKind::Squiggly => {
                for quad in self.quads_or_rect() {
                    let (qx1, qy1, qx2, qy2) = quad_bounds(quad);
                    let bx = qx1 - x1;
                    let by = qy1 - y1;
                    let bw = (qx2 - qx1).max(0.1);
                    let bh = (qy2 - qy1).max(0.1);
                    match self.kind {
                        AnnotationKind::Highlight => c.extend_from_slice(
                            format!("{gs}{fill} {bx:.2} {by:.2} {bw:.2} {bh:.2} re f\n")
                                .as_bytes(),
                        ),
                        AnnotationKind::Underline => {
                            let y = by + (lw * 0.6).min(bh / 2.0);
                            c.extend_from_slice(
                                format!(
                                    "q {stroke} {lw:.2} w {bx:.2} {y:.2} m {:.2} {y:.2} l S Q\n",
                                    bx + bw
                                )
                                .as_bytes(),
                            );
                        }
                        AnnotationKind::StrikeOut => {
                            let y = by + bh / 2.0;
                            c.extend_from_slice(
                                format!(
                                    "q {stroke} {lw:.2} w {bx:.2} {y:.2} m {:.2} {y:.2} l S Q\n",
                                    bx + bw
                                )
                                .as_bytes(),
                            );
                        }
                        // Squiggly : le zigzag se dessine sur le bas du bloc.
                        _ => {
                            let amp = 2.0_f32.min(bh / 3.0);
                            c.extend_from_slice(
                                format!("q {stroke} {lw:.2} w 1 J 1 j {bx:.2} {:.2} m", by + amp)
                                    .as_bytes(),
                            );
                            let mut x = 0.0_f32;
                            let mut up = true;
                            while x < bw {
                                x = (x + 2.0).min(bw);
                                let y = by + if up { amp * 2.0 } else { 0.0 };
                                c.extend_from_slice(format!(" {:.2} {y:.2} l", bx + x).as_bytes());
                                up = !up;
                            }
                            c.extend_from_slice(b" S Q\n");
                        }
                    }
                }
            }

            AnnotationKind::FreeText => {
                let size = self.style.font_size.clamp(4.0, 72.0);
                let leading = size * 1.2;
                c.extend_from_slice(
                    format!(
                        "{gs}BT /{APPEARANCE_FONT_NAME} {size:.2} Tf {fill} {leading:.2} TL \
                         2 {:.2} Td\n",
                        (h - size - 2.0).max(1.0)
                    )
                    .as_bytes(),
                );
                for line in self.content.lines() {
                    push_text_literal(&mut c, line);
                    c.extend_from_slice(b" Tj T*\n");
                }
                c.extend_from_slice(b"ET Q\n");
            }

            AnnotationKind::Square => {
                let inset = lw / 2.0;
                c.extend_from_slice(
                    format!(
                        "q {stroke} {lw:.2} w {inset:.2} {inset:.2} {:.2} {:.2} re S Q\n",
                        (w - lw).max(0.1),
                        (h - lw).max(0.1)
                    )
                    .as_bytes(),
                );
            }
            AnnotationKind::Circle => {
                // Ellipse : quatre courbes de Bézier, constante 0.5523.
                let (rx, ry) = ((w - lw) / 2.0, (h - lw) / 2.0);
                let (cx, cy) = (w / 2.0, h / 2.0);
                let (kx, ky) = (rx * 0.5523, ry * 0.5523);
                let quad = |x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32| {
                    format!("{x1:.2} {y1:.2} {x2:.2} {y2:.2} {x3:.2} {y3:.2} c ")
                };
                let mut path = format!("q {stroke} {lw:.2} w {:.2} {:.2} m ", cx + rx, cy);
                path.push_str(&quad(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry));
                path.push_str(&quad(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy));
                path.push_str(&quad(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry));
                path.push_str(&quad(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy));
                path.push_str(" h S Q\n");
                c.extend_from_slice(path.as_bytes());
            }

            AnnotationKind::Line | AnnotationKind::Arrow => {
                let (x1, y1, _w, _h) = self.frame();
                let fallback = vec![[self.rect[0], self.rect[1]], [self.rect[2], self.rect[3]]];
                let pts = if self.ink_points.first().map_or(0, |p| p.len()) >= 2 {
                    self.ink_points.first().cloned().unwrap_or(fallback)
                } else {
                    fallback
                };
                let a = pts[0];
                let b = pts[1];
                let (ax, ay) = (a[0] - x1, a[1] - y1);
                let (bx, by) = (b[0] - x1, b[1] - y1);
                c.extend_from_slice(
                    format!(
                        "q {stroke} {lw:.2} w 0 J {ax:.2} {ay:.2} m {bx:.2} {by:.2} l S"
                    )
                    .as_bytes(),
                );
                if self.kind == AnnotationKind::Arrow {
                    let (dx, dy) = (bx - ax, by - ay);
                    let len = (dx * dx + dy * dy).sqrt().max(0.01);
                    let head = (len * 0.2).clamp(5.0, 14.0);
                    let (ux, uy) = (dx / len, dy / len);
                    let (px, py) = (-uy, ux);
                    let (hx, hy) = (bx - ux * head, by - uy * head);
                    let half = head * 0.45;
                    c.extend_from_slice(
                        format!(
                            " {fill} {bx:.2} {by:.2} m {:.2} {:.2} l {:.2} {:.2} l f",
                            hx + px * half,
                            hy + py * half,
                            hx - px * half,
                            hy - py * half
                        )
                        .as_bytes(),
                    );
                }
                c.extend_from_slice(b" Q\n");
            }

            AnnotationKind::Ink => {
                let (x1, y1, _w, _h) = self.frame();
                c.extend_from_slice(
                    format!("{gs}{stroke} {lw:.2} w 1 J 1 j ").as_bytes(),
                );
                for stroke in &self.ink_points {
                    if stroke.len() < 2 {
                        continue;
                    }
                    let mut first = true;
                    for p in stroke {
                        let (x, y) = (p[0] - x1, p[1] - y1);
                        if first {
                            c.extend_from_slice(format!("{x:.2} {y:.2} m ").as_bytes());
                            first = false;
                        } else {
                            c.extend_from_slice(format!("{x:.2} {y:.2} l ").as_bytes());
                        }
                    }
                }
                c.extend_from_slice(b"S Q\n");
            }

            AnnotationKind::Stamp => {
                let size = self.style.font_size.clamp(6.0, 48.0);
                // Le libellé est centré, la date et l'auteur dessous en plus
                // petit : un tampon dynamique tient donc sur trois lignes
                // dans la même boîte.
                let lines: Vec<&str> = self.content.lines().filter(|l| !l.is_empty()).collect();
                let lines: Vec<&str> = if lines.is_empty() {
                    vec![""]
                } else {
                    lines
                };
                let leading = size * 1.15;
                let block = leading * lines.len() as f32;
                let top = (h + block) / 2.0 - leading + size * 0.28;
                c.extend_from_slice(
                    format!("q {stroke} {lw:.2} w 0 0 {w:.2} {h:.2} re S\nBT\n").as_bytes(),
                );
                for (index, line) in lines.iter().enumerate() {
                    let line_size = if index == 0 { size } else { size * 0.72 };
                    // Largeur approchée de l'Helvetica (0.5 cadratin par
                    // caractère) : le centrage ne peut pas être mesuré sans
                    // analyser la police.
                    let text_w = line.chars().count() as f32 * line_size * 0.5;
                    let x = ((w - text_w) / 2.0).max(2.0);
                    let y = top - index as f32 * leading;
                    c.extend_from_slice(
                        format!(
                            "/{APPEARANCE_FONT_NAME} {line_size:.2} Tf {fill} 1 0 0 1 {x:.2} {y:.2} Tm "
                        )
                        .as_bytes(),
                    );
                    push_text_literal(&mut c, line);
                    c.extend_from_slice(b" Tj\n");
                }
                c.extend_from_slice(b"ET Q\n");
            }
        }

        Some(c)
    }

    /// Ressources référencées par [`Self::appearance_content`] : Helvetica
    /// pour le texte, état graphique pour la transparence.
    pub fn appearance_resources(&self, font: Object) -> Dictionary {
        let mut gs = Dictionary::new();
        gs.set("Type", Object::Name(b"ExtGState".to_vec()));
        gs.set("ca", Object::Real(self.style.opacity.clamp(0.05, 1.0)));
        gs.set("CA", Object::Real(1.0));

        let mut gstates = Dictionary::new();
        gstates.set(APPEARANCE_GS_NAME, Object::Dictionary(gs));

        let mut fonts = Dictionary::new();
        fonts.set(APPEARANCE_FONT_NAME, font);

        let mut res = Dictionary::new();
        res.set("Font", Object::Dictionary(fonts));
        res.set("ExtGState", Object::Dictionary(gstates));
        res
    }
}
