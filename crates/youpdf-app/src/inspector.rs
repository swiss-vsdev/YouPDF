//! Inspecteur d'attributs flottant.
//!
//! Une sélection (texte, image, annotation, champ de formulaire) ouvre un
//! panneau posé juste au-dessus d'elle, avec une flèche qui pointe vers
//! l'élément. Le placement est calculé ici, à part de l'interface, pour rester
//! testable : l'inspecteur ne doit jamais sortir du cadre de la fenêtre.

use egui::{pos2, Color32, Painter, Rect, RichText, Rounding, Sense, Shape, Stroke, Vec2};
use youpdf_core::{PatchFont, TextAlign, TextStyle};
use youpdf_shared::pal;

/// Identificateur d'un bloc de texte dans la page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TextBlockId(pub usize);

/// Identificateur d'une image dans la page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ImageId(pub usize);

/// Identificateur d'une annotation dans la page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AnnotationId(pub usize);

/// Cible de l'inspecteur : l'élément dont on édite les attributs.
#[derive(Clone, Debug, PartialEq)]
pub enum InspectorTarget {
    /// Texte sélectionné sur la page.
    TextSelection {
        page: usize,
        /// Premier et dernier bloc couverts, dans l'ordre de lecture.
        range: (usize, usize),
        bounds: Rect,
    },
    /// Bloc de texte (mode Édition).
    TextElement {
        page: usize,
        id: TextBlockId,
        bounds: Rect,
    },
    /// Image (mode Édition).
    ///
    /// Construit par la tâche 3.4 (inspecteur d'image).
    #[allow(dead_code)]
    ImageElement {
        page: usize,
        id: ImageId,
        bounds: Rect,
    },
    /// Annotation (mode Commentaire).
    AnnotationElement {
        page: usize,
        id: AnnotationId,
        bounds: Rect,
    },
    /// Champ de formulaire (mode Formulaire).
    ///
    /// Construit par la tâche 6.3 (propriétés de champ).
    #[allow(dead_code)]
    FormFieldElement {
        page: usize,
        name: String,
        bounds: Rect,
    },
}

impl InspectorTarget {
    /// Rectangles occupés par l'élément, en coordonnées écran.
    pub fn bounds(&self) -> Rect {
        match self {
            InspectorTarget::TextSelection { bounds, .. }
            | InspectorTarget::TextElement { bounds, .. }
            | InspectorTarget::ImageElement { bounds, .. }
            | InspectorTarget::AnnotationElement { bounds, .. }
            | InspectorTarget::FormFieldElement { bounds, .. } => *bounds,
        }
    }

    /// Numéro de page (1-based) de l'élément visé.
    pub fn page(&self) -> usize {
        match self {
            InspectorTarget::TextSelection { page, .. }
            | InspectorTarget::TextElement { page, .. }
            | InspectorTarget::ImageElement { page, .. }
            | InspectorTarget::AnnotationElement { page, .. }
            | InspectorTarget::FormFieldElement { page, .. } => *page,
        }
    }

    /// Libellé court de la cible, pour l'en-tête.
    pub fn headline(&self) -> String {
        match self {
            InspectorTarget::TextSelection { .. } => "Selection".to_owned(),
            InspectorTarget::TextElement { .. } => "Text".to_owned(),
            InspectorTarget::ImageElement { .. } => "Image".to_owned(),
            InspectorTarget::AnnotationElement { .. } => "Annotation".to_owned(),
            InspectorTarget::FormFieldElement { name, .. } => format!("Field {name}"),
        }
    }
}

/// Place de l'inspecteur : le cadre et la flèche qui pointe vers l'élément.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InspectorPlacement {
    /// Cadre du panneau, en coordonnées écran.
    pub rect: Rect,
    /// Abscisse de la pointe de la flèche.
    pub arrow_x: f32,
    /// Vrai quand le panneau s'est posé sous l'élément : la flèche pointe
    /// alors vers le haut.
    pub arrow_up: bool,
}

/// Hauteur du panneau avant que son contenu ne soit mesuré.
pub const INSPECTOR_HEIGHT: f32 = 38.0;

/// Hauteur estimée de l'inspecteur de texte (en-tête + ligne de réglages),
/// avant que son contenu ne soit mesuré.
pub const TEXT_INSPECTOR_HEIGHT: f32 = 66.0;

/// Hauteur ajoutée par la palette de couleurs dépliée.
pub const PALETTE_HEIGHT: f32 = 52.0;

/// Largeur du panneau avant que son contenu ne soit mesuré : assez pour la
/// ligne de réglages du texte.
pub const INSPECTOR_WIDTH: f32 = 330.0;

/// Marge gardée entre le panneau et les bords de la fenêtre.
const SCREEN_MARGIN: f32 = 4.0;

/// Distance entre la pointe de la flèche et l'élément visé.
const ARROW_GAP: f32 = 6.0;

/// Calcule la place de l'inspecteur : juste au-dessus de `bounds`, poussé vers
/// l'intérieur de `screen` pour ne jamais dépasser à gauche, à droite ou en
/// haut. `size` est la taille voulue du panneau ; elle est réduite si la
/// fenêtre est plus étroite. Faute de place au-dessus (élément contre le bord
/// haut), le panneau passe sous l'élément et sa flèche pointe vers le haut.
pub fn place_inspector(bounds: Rect, screen: Rect, size: Vec2) -> InspectorPlacement {
    let available_w = (screen.width() - 2.0 * SCREEN_MARGIN).max(1.0);
    let width = size.x.min(available_w);
    let height = size.y;
    // Centré sur l'élément, dos à la fenêtre.
    let x = (bounds.center().x - width / 2.0)
        .clamp(screen.min.x + SCREEN_MARGIN, screen.max.x - width - SCREEN_MARGIN);
    let above = bounds.top() - height - ARROW_GAP;
    let (y, arrow_up) = if above >= screen.min.y + SCREEN_MARGIN {
        (above, false)
    } else {
        // Pas de place au-dessus : sous l'élément.
        let below = bounds.bottom() + ARROW_GAP;
        let y = below.min(screen.max.y - height - SCREEN_MARGIN);
        (y.max(screen.min.y + SCREEN_MARGIN), true)
    };
    let rect = Rect::from_min_size(pos2(x, y), Vec2::new(width, height));
    let arrow_x = arrow_x(bounds, rect);
    InspectorPlacement {
        rect,
        arrow_x,
        arrow_up,
    }
}

/// Abscisse de la flèche : centrée sur l'élément, sans quitter le cadre.
pub fn arrow_x(bounds: Rect, rect: Rect) -> f32 {
    let low = rect.min.x + 12.0;
    let high = (rect.max.x - 12.0).max(low);
    bounds.center().x.clamp(low, high)
}

/// Dessine la flèche du panneau vers l'élément visé.
///
/// Le trait du cadre est recouvert là où la flèche le touche, sinon la
/// séparation se voit.
pub fn paint_arrow(
    painter: &Painter,
    rect: Rect,
    arrow_x: f32,
    arrow_up: bool,
    fill: Color32,
    border: Color32,
) {
    let half = 7.0;
    let (tip, a, b) = if arrow_up {
        (
            pos2(arrow_x, rect.min.y - ARROW_GAP + 1.0),
            pos2(arrow_x - half, rect.min.y + 0.5),
            pos2(arrow_x + half, rect.min.y + 0.5),
        )
    } else {
        (
            pos2(arrow_x, rect.max.y + ARROW_GAP - 1.0),
            pos2(arrow_x - half, rect.max.y - 0.5),
            pos2(arrow_x + half, rect.max.y - 0.5),
        )
    };
    painter.add(Shape::convex_polygon(
        vec![tip, a, b],
        fill,
        Stroke::new(1.0_f32, border),
    ));
    painter.line_segment([a, b], Stroke::new(1.5_f32, fill));
}

/// Palette rapide : vingt-quatre teintes, du noir au blanc.
pub const TEXT_COLORS: [[u8; 3]; 24] = [
    [0, 0, 0],
    [64, 64, 64],
    [128, 128, 128],
    [192, 192, 192],
    [255, 255, 255],
    [127, 0, 0],
    [255, 0, 0],
    [255, 128, 128],
    [255, 153, 0],
    [255, 192, 0],
    [255, 255, 0],
    [0, 127, 0],
    [0, 176, 80],
    [146, 208, 80],
    [0, 112, 192],
    [0, 176, 240],
    [0, 255, 255],
    [0, 32, 96],
    [112, 48, 160],
    [255, 0, 255],
    [255, 153, 204],
    [128, 64, 0],
    [255, 204, 153],
    [240, 240, 240],
];

/// État d'interface des contrôles de texte : palette ouverte, saisie hexa.
#[derive(Clone, Debug, Default)]
pub struct TextStyleUi {
    /// La palette de vingt-quatre teintes est dépliée.
    pub palette_open: bool,
    /// Saisie hexadécimale en cours (`#RRGGBB` ou `RRGGBB`).
    pub hex: String,
}

/// Lit une couleur hexadécimale (`#RRGGBB` ou `RRGGBB`, casse libre).
pub fn parse_hex_color(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |range: std::ops::Range<usize>| u8::from_str_radix(&digits[range], 16).ok();
    Some([byte(0..2)?, byte(2..4)?, byte(4..6)?])
}

/// Contrôles de style du texte patché, sur une seule ligne.
///
/// Rend vrai quand un réglage a changé : l'appelant réécrit alors le bloc.
pub fn text_style_row(
    ui: &mut egui::Ui,
    style: &mut TextStyle,
    state: &mut TextStyleUi,
) -> bool {
    let mut changed = false;
    // La ligne des réglages est large : elle passe à la ligne plutôt que de
    // déborder du fond du panneau.
    ui.horizontal_wrapped(|ui| {
        // Famille de police.
        egui::ComboBox::from_id_salt("inspector-font")
            .selected_text(style.font.display_name())
            .width(74.0)
            .show_ui(ui, |ui| {
                for font in PatchFont::ALL {
                    if ui
                        .selectable_label(style.font == font, font.display_name())
                        .clicked()
                    {
                        style.font = font;
                        changed = true;
                    }
                }
            });
        // Corps.
        egui::ComboBox::from_id_salt("inspector-size")
            .selected_text(format!("{}", style.size.round() as i64))
            .width(46.0)
            .show_ui(ui, |ui| {
                for size in TextStyle::SIZES {
                    if ui
                        .selectable_label((style.size - size).abs() < 0.01, size.to_string())
                        .clicked()
                    {
                        style.size = size;
                        changed = true;
                    }
                }
            });
        // Couleur : pastille ouvrant la palette.
        let (swatch, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::click());
        let color = Color32::from_rgb(style.color[0], style.color[1], style.color[2]);
        ui.painter().circle_filled(swatch.center(), 8.0, color);
        ui.painter()
            .circle_stroke(swatch.center(), 8.0, Stroke::new(1.0_f32, pal().separator));
        if resp.clicked() {
            state.palette_open = !state.palette_open;
            state.hex = format!(
                "{:02X}{:02X}{:02X}",
                style.color[0], style.color[1], style.color[2]
            );
        }
        // Styles.
        for (label, active, flag) in [
            ("B", style.bold, 0u8),
            ("I", style.italic, 1),
            ("U", style.underline, 2),
            ("S", style.strikeout, 3),
        ] {
            if ui
                .selectable_label(
                    active,
                    RichText::new(label).size(11.0).strong(),
                )
                .clicked()
            {
                match flag {
                    0 => style.bold = !style.bold,
                    1 => style.italic = !style.italic,
                    2 => style.underline = !style.underline,
                    _ => style.strikeout = !style.strikeout,
                }
                changed = true;
            }
        }
        // Alignement.
        for align in TextAlign::ALL {
            let label = match align {
                TextAlign::Left => "L",
                TextAlign::Center => "C",
                TextAlign::Right => "R",
            };
            if ui
                .selectable_label(style.align == align, RichText::new(label).size(11.0))
                .clicked()
            {
                style.align = align;
                changed = true;
            }
        }
        // Interligne.
        egui::ComboBox::from_id_salt("inspector-leading")
            .selected_text(format!("{:.2}", style.line_spacing))
            .width(52.0)
            .show_ui(ui, |ui| {
                for spacing in TextStyle::LINE_SPACINGS {
                    if ui
                        .selectable_label(
                            (style.line_spacing - spacing).abs() < 0.01,
                            format!("{spacing:.2}"),
                        )
                        .clicked()
                    {
                        style.line_spacing = spacing;
                        changed = true;
                    }
                }
            });
        // Approche.
        let spacing = ui.add(
            egui::DragValue::new(&mut style.char_spacing)
                .range(-2.0..=20.0)
                .speed(0.1)
                .prefix("T "),
        );
        if spacing.changed() {
            changed = true;
        }
        spacing.on_hover_text("Character spacing");
        changed
    });
    if state.palette_open {
        ui.horizontal(|ui| {
            for row in TEXT_COLORS.chunks(8) {
                for color in row {
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::click());
                    let fill = Color32::from_rgb(color[0], color[1], color[2]);
                    ui.painter().rect_filled(rect, Rounding::same(2.0), fill);
                    if style.color == *color {
                        ui.painter().rect_stroke(
                            rect,
                            2.0,
                            Stroke::new(1.5_f32, pal().accent),
                        );
                    }
                    if resp.clicked() {
                        style.color = *color;
                        state.hex = format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
                        changed = true;
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("#").size(11.0).color(pal().muted));
            let hex = ui.add(
                egui::TextEdit::singleline(&mut state.hex)
                    .desired_width(64.0)
                    .font(egui::FontId::monospace(11.0)),
            );
            if hex.changed() {
                if let Some(color) = parse_hex_color(&state.hex) {
                    style.color = color;
                    changed = true;
                }
            }
            if ui.small_button("Close").clicked() {
                state.palette_open = false;
            }
        });
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;
    fn screen() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))
    }

    fn size() -> Vec2 {
        vec2(INSPECTOR_WIDTH, INSPECTOR_HEIGHT)
    }

    /// Le panneau se pose au-dessus de l'élément, flèche centrée.
    #[test]
    fn panel_sits_above_the_element() {
        let bounds = Rect::from_min_size(pos2(500.0, 400.0), vec2(100.0, 20.0));
        let placement = place_inspector(bounds, screen(), size());
        assert!(placement.rect.max.y < bounds.top(), "le panneau recouvre l'élément");
        assert!(!placement.arrow_up);
        assert_eq!(placement.arrow_x, 550.0);
    }

    /// Près d'un bord, le panneau glisse vers l'intérieur sans dépasser.
    #[test]
    fn panel_stays_inside_the_window() {
        let left = Rect::from_min_size(pos2(0.0, 400.0), vec2(40.0, 20.0));
        let placement = place_inspector(left, screen(), size());
        assert!(placement.rect.min.x >= 0.0);
        assert!(placement.arrow_x >= placement.rect.min.x + 12.0);

        let right = Rect::from_min_size(pos2(1170.0, 400.0), vec2(30.0, 20.0));
        let placement = place_inspector(right, screen(), size());
        assert!(placement.rect.max.x <= 1200.0);
        assert!(placement.arrow_x <= placement.rect.max.x - 12.0);
    }

    /// Une fenêtre plus étroite que le panneau le rétrécit au lieu de le
    /// laisser dépasser.
    #[test]
    fn narrow_window_shrinks_the_panel() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 800.0));
        let bounds = Rect::from_min_size(pos2(20.0, 400.0), vec2(60.0, 20.0));
        let placement = place_inspector(bounds, screen, size());
        assert!(placement.rect.max.x <= screen.max.x);
    }

    /// Un élément contre le bord haut fait passer le panneau dessous, flèche
    /// vers le haut : il ne recouvre jamais l'élément.
    #[test]
    fn panel_flips_below_when_there_is_no_room_above() {
        let bounds = Rect::from_min_size(pos2(300.0, 2.0), vec2(80.0, 10.0));
        let placement = place_inspector(bounds, screen(), size());
        assert!(placement.arrow_up);
        assert!(
            placement.rect.min.y >= bounds.bottom(),
            "le panneau recouvre l'élément"
        );
        assert!(placement.rect.min.y >= 0.0);
    }

    /// Une cible minuscule ne fait pas paniquer le calcul de la flèche.
    #[test]
    fn arrow_survives_a_tiny_panel() {
        let bounds = Rect::from_min_size(pos2(10.0, 10.0), vec2(2.0, 2.0));
        let rect = Rect::from_min_size(pos2(0.0, 20.0), vec2(4.0, 4.0));
        assert_eq!(arrow_x(bounds, rect), 12.0);
    }

    /// L'en-tête nomme la cible, et les accesseurs restent cohérents.
    #[test]
    fn target_reports_page_and_headline() {
        let bounds = Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 20.0));
        let target = InspectorTarget::FormFieldElement {
            page: 3,
            name: "total".to_owned(),
            bounds,
        };
        assert_eq!(target.page(), 3);
        assert_eq!(target.headline(), "Field total");
        assert_eq!(target.bounds(), bounds);
    }

    /// La saisie hexadécimale accepte les deux écritures et refuse le reste.
    #[test]
    fn hex_colour_is_parsed_strictly() {
        assert_eq!(parse_hex_color("#1a2B3c"), Some([0x1A, 0x2B, 0x3C]));
        assert_eq!(parse_hex_color("FF0000"), Some([255, 0, 0]));
        assert_eq!(parse_hex_color("  #00ff00 "), Some([0, 255, 0]));
        assert_eq!(parse_hex_color("12345"), None);
        assert_eq!(parse_hex_color("GGGGGG"), None);
    }
}
