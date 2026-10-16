//! Jetons de thème pour l'interface.
//!
//! Les quatre thèmes officiels de YouPDF. Chaque thème est une valeur
//! constante. On n'utilise **jamais** de couleur en dur ailleurs dans le
//! code : tout passe par ces jetons.

use egui::{Color32, Context, Margin, Rounding, Stroke, Visuals};

/// Palette complète d'un thème.
///
/// Tous les champs sont `const` : un thème est créé à la compilation.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Nom lisible (affiché dans les préférences).
    pub name: &'static str,
    /// Vrai si le thème est sombre — ajuste les visuels natifs d'egui.
    pub is_dark: bool,

    /// Fond général de la fenêtre.
    pub bg: Color32,
    /// Fond des panneaux (barres, sidebar, inspecteur).
    pub panel: Color32,
    /// Variante légèrement plus foncée/claire pour les zones secondaires.
    pub panel_alt: Color32,
    /// Fond de la zone de rendu du PDF (canvas central).
    pub canvas: Color32,

    /// Texte principal.
    pub text: Color32,
    /// Texte secondaire (labels, aides).
    pub text_weak: Color32,

    /// Couleur d'accent (boutons actifs, sélection).
    pub accent: Color32,
    /// Accent atténué (survol).
    pub accent_weak: Color32,

    /// Bordures et séparateurs.
    pub border: Color32,

    /// États sémantiques.
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
}

impl Theme {
    // ---- Thèmes officiels ------------------------------------------------

    /// Vert doux — teinte reposante pour de longues sessions de lecture.
    pub const SOFT_GREEN: Self = Self {
        name: "Vert doux",
        is_dark: false,
        bg: Color32::from_rgb(0xEE, 0xF3, 0xEC),
        panel: Color32::from_rgb(0xF7, 0xFA, 0xF5),
        panel_alt: Color32::from_rgb(0xE4, 0xEC, 0xE1),
        canvas: Color32::from_rgb(0xDA, 0xE3, 0xD6),
        text: Color32::from_rgb(0x1F, 0x2B, 0x20),
        text_weak: Color32::from_rgb(0x5C, 0x6B, 0x5E),
        accent: Color32::from_rgb(0x3E, 0x8E, 0x5A),
        accent_weak: Color32::from_rgb(0xC5, 0xDE, 0xCB),
        border: Color32::from_rgb(0xC8, 0xD4, 0xC3),
        success: Color32::from_rgb(0x3E, 0x8E, 0x5A),
        warning: Color32::from_rgb(0xC9, 0x8A, 0x2B),
        error: Color32::from_rgb(0xB0, 0x3A, 0x3A),
    };

    /// Aspect papier — pour retrouver une sensation de lecture imprimée.
    pub const PAPER: Self = Self {
        name: "Papier",
        is_dark: false,
        bg: Color32::from_rgb(0xF5, 0xF1, 0xE8),
        panel: Color32::from_rgb(0xFA, 0xF7, 0xF0),
        panel_alt: Color32::from_rgb(0xEC, 0xE6, 0xD8),
        canvas: Color32::from_rgb(0xE0, 0xD8, 0xC5),
        text: Color32::from_rgb(0x2E, 0x29, 0x22),
        text_weak: Color32::from_rgb(0x6F, 0x66, 0x58),
        accent: Color32::from_rgb(0x8B, 0x5E, 0x2A),
        accent_weak: Color32::from_rgb(0xDD, 0xCC, 0xAC),
        border: Color32::from_rgb(0xD3, 0xC9, 0xB2),
        success: Color32::from_rgb(0x5E, 0x7A, 0x3F),
        warning: Color32::from_rgb(0xB5, 0x82, 0x28),
        error: Color32::from_rgb(0xA8, 0x40, 0x35),
    };

    /// Mode clair neutre.
    pub const LIGHT: Self = Self {
        name: "Clair",
        is_dark: false,
        bg: Color32::from_rgb(0xFA, 0xFA, 0xFA),
        panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        panel_alt: Color32::from_rgb(0xF2, 0xF2, 0xF3),
        canvas: Color32::from_rgb(0xE6, 0xE6, 0xE8),
        text: Color32::from_rgb(0x14, 0x14, 0x16),
        text_weak: Color32::from_rgb(0x70, 0x70, 0x76),
        accent: Color32::from_rgb(0x3D, 0x63, 0xE0),
        accent_weak: Color32::from_rgb(0xD3, 0xDE, 0xFB),
        border: Color32::from_rgb(0xDD, 0xDD, 0xE0),
        success: Color32::from_rgb(0x1E, 0x8A, 0x4C),
        warning: Color32::from_rgb(0xC7, 0x7A, 0x00),
        error: Color32::from_rgb(0xC0, 0x39, 0x39),
    };

    /// Mode sombre — contraste doux, pas de noir pur.
    pub const DARK: Self = Self {
        name: "Sombre",
        is_dark: true,
        bg: Color32::from_rgb(0x1A, 0x1B, 0x1E),
        panel: Color32::from_rgb(0x22, 0x23, 0x27),
        panel_alt: Color32::from_rgb(0x2B, 0x2C, 0x31),
        canvas: Color32::from_rgb(0x12, 0x13, 0x15),
        text: Color32::from_rgb(0xE8, 0xE8, 0xEC),
        text_weak: Color32::from_rgb(0x9A, 0x9B, 0xA2),
        accent: Color32::from_rgb(0x6E, 0x8F, 0xF2),
        accent_weak: Color32::from_rgb(0x2C, 0x38, 0x5C),
        border: Color32::from_rgb(0x35, 0x36, 0x3C),
        success: Color32::from_rgb(0x4C, 0xAF, 0x6E),
        warning: Color32::from_rgb(0xE0, 0xA9, 0x3C),
        error: Color32::from_rgb(0xE0, 0x6C, 0x6C),
    };

    /// Thème clair, neutre, à l'aspect d'un éditeur de bureau moderne
    /// (blanc, gris clair, accent bleu). Les couleurs sont originales.
    pub const NEUTRAL: Self = Self {
        name: "Neutre",
        is_dark: false,
        bg: Color32::from_rgb(0xF4, 0xF5, 0xF7),
        panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        panel_alt: Color32::from_rgb(0xEE, 0xEF, 0xF2),
        canvas: Color32::from_rgb(0xE2, 0xE4, 0xE9),
        text: Color32::from_rgb(0x1C, 0x1D, 0x22),
        text_weak: Color32::from_rgb(0x6B, 0x6D, 0x76),
        accent: Color32::from_rgb(0x3B, 0x6B, 0xD6),
        accent_weak: Color32::from_rgb(0xD6, 0xDE, 0xF6),
        border: Color32::from_rgb(0xD5, 0xD7, 0xDF),
        success: Color32::from_rgb(0x2E, 0xA0, 0x5C),
        warning: Color32::from_rgb(0xD0, 0x8A, 0x24),
        error: Color32::from_rgb(0xC8, 0x3F, 0x3F),
    };

    /// Tous les thèmes disponibles, dans l'ordre d'affichage.
    pub const ALL: &'static [Theme] =
        &[Self::SOFT_GREEN, Self::PAPER, Self::LIGHT, Self::DARK, Self::NEUTRAL];

    // ---- Application -----------------------------------------------------

    /// Applique le thème à tout le contexte egui.
    ///
    /// Effet secondaire volontairement unique : après cet appel, aucune
    /// autre partie du code n'a besoin de connaître les couleurs.
    pub fn apply(&self, ctx: &Context) {
        let mut visuals = if self.is_dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.panel_fill = self.panel;
        visuals.window_fill = self.bg;
        visuals.extreme_bg_color = self.canvas;
        visuals.faint_bg_color = self.panel_alt;
        visuals.override_text_color = Some(self.text);

        let widgets = &mut visuals.widgets;

        widgets.noninteractive.bg_fill = self.panel;
        widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text);
        widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border);
        widgets.noninteractive.rounding = Rounding::same(6.0);

        widgets.inactive.bg_fill = self.panel_alt;
        widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.text);
        widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border);
        widgets.inactive.rounding = Rounding::same(6.0);

        widgets.hovered.bg_fill = self.accent_weak;
        widgets.hovered.fg_stroke = Stroke::new(1.5_f32, self.accent);
        widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.accent);
        widgets.hovered.rounding = Rounding::same(6.0);

        widgets.active.bg_fill = self.accent;
        widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
        widgets.active.bg_stroke = Stroke::new(1.0_f32, self.accent);
        widgets.active.rounding = Rounding::same(6.0);

        widgets.open.bg_fill = self.panel_alt;
        widgets.open.fg_stroke = Stroke::new(1.0_f32, self.text);
        widgets.open.rounding = Rounding::same(6.0);

        visuals.selection.bg_fill = self.accent;
        visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
        visuals.window_rounding = Rounding::same(8.0);
        visuals.menu_rounding = Rounding::same(6.0);

        ctx.set_visuals(visuals);

        // Espacements : légèrement plus généreux que le défaut egui.
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.menu_margin = Margin::same(6.0);
        style.spacing.indent = 18.0;
        ctx.set_style(style);
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::SOFT_GREEN
    }
}
