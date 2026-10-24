//! Jetons de thème de l'interface.
//!
//! Les quatre thèmes de YouPDF. Une palette est un jeu de couleurs constant ;
//! le thème actif est rangé dans une variable locale au fil d'exécution, lue
//! par tout le chrome à travers [`pal`]. C'est ce qui permet de changer de
//! thème sans redémarrer.
//!
//! **Aucune couleur en dur ailleurs que dans ce fichier** : tout le chrome
//! passe par les jetons de [`Palette`]. Les tailles de texte ont la même règle :
//! l'échelle [`CAPTION`] → [`BODY`] → [`TITLE`] est posée une fois pour toutes
//! dans [`Theme::apply`], un simple `ui.label` en hérite donc, et les variantes
//! éteintes se posent avec [`section`] et [`caption`].

use std::cell::Cell;

use egui::{Color32, Context, FontId, RichText, Rounding, Stroke, TextStyle, Visuals};

/// Texte secondaire : aide, horodatages, libellés d'un seul mot.
pub const CAPTION: f32 = 10.5;
/// Libellés de champ et lignes denses, entre l'aide et le texte courant.
pub const LABEL: f32 = 11.0;
/// Texte courant, boutons, titres de section : la taille de base.
pub const BODY: f32 = 12.0;
/// Titres de panneau, d'onglet, de fenêtre et d'écran d'accueil.
pub const TITLE: f32 = 13.0;
/// Champs et tableaux monospace.
pub const MONO: f32 = 11.0;

/// Texte éteint à la taille voulue : toute la peinture sourde passe par là.
pub fn muted(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).size(size).color(pal().muted)
}

/// Titre de section : [`BODY`] éteint.
pub fn section(text: impl Into<String>) -> RichText {
    muted(text, BODY)
}

/// Texte secondaire : [`CAPTION`] éteint.
pub fn caption(text: impl Into<String>) -> RichText {
    muted(text, CAPTION)
}

/// Couleurs de l'interface, choisies par le thème.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Fond du canevas, quand l'utilisateur n'a pas choisi d'autre fond.
    pub canvas_bg: Color32,
    /// Fond des barres, panneaux et menus.
    pub chrome_bg: Color32,
    /// Fond des contrôles au repos.
    pub widget_bg: Color32,
    /// Fond des contrôles survolés ou ouverts.
    pub hover_bg: Color32,
    /// Couleur d'accent (sélection, boutons actifs, liens).
    pub accent: Color32,
    /// Texte secondaire, libellés d'aide.
    pub muted: Color32,
    /// Icônes au repos.
    pub icon: Color32,
    /// Icônes et texte mis en avant.
    pub icon_bright: Color32,
    /// Teinte des vignettes de page (le papier est toujours clair).
    pub thumb_tint: Color32,
    /// Bordures et séparateurs.
    pub separator: Color32,
    /// Rouge des marques de caviardage.
    pub redact_red: Color32,
    /// Jaune des correspondances de recherche.
    pub search_match: Color32,
}

/// Thème de l'interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Sombre, la palette d'origine.
    #[default]
    Dark,
    /// Clair : chrome et canevas clairs.
    Light,
    /// Menthe : vert d'eau, pour un écran clair teinté.
    Mint,
    /// Nuit étoilée : bleu très sombre, canevas noir.
    Starry,
}

impl Theme {
    /// Tous les thèmes, dans l'ordre d'affichage.
    pub const ALL: [Theme; 4] = [Theme::Dark, Theme::Light, Theme::Mint, Theme::Starry];

    /// Nom affiché dans le menu.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
            Theme::Mint => "Mint",
            Theme::Starry => "Starry",
        }
    }

    /// Vrai si le thème est sombre : ajuste les visuels natifs d'egui.
    pub fn is_dark(self) -> bool {
        matches!(self, Theme::Dark | Theme::Starry)
    }

    /// Palette associée.
    pub fn palette(self) -> Palette {
        match self {
            Theme::Dark => DARK,
            Theme::Light => LIGHT,
            Theme::Mint => MINT,
            Theme::Starry => STARRY,
        }
    }

    /// Applique le thème au contexte egui.
    ///
    /// Le chrome de YouPDF se peint lui-même avec [`pal`] ; cet appel règle
    /// les visuels natifs d'egui (fenêtres, menus, champs, infobulles) sur la
    /// même palette.
    pub fn apply(self, ctx: &Context) {
        let palette = self.palette();
        let mut visuals = if self.is_dark() {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.panel_fill = palette.canvas_bg;
        visuals.window_fill = palette.chrome_bg;
        visuals.extreme_bg_color = palette.widget_bg;
        visuals.faint_bg_color = palette.widget_bg;
        visuals.override_text_color = Some(palette.icon_bright);
        visuals.selection.bg_fill = palette.accent;
        visuals.selection.stroke = Stroke::new(1.5_f32, Color32::WHITE);
        visuals.hyperlink_color = palette.accent;
        visuals.window_rounding = Rounding::same(10.0);
        visuals.menu_rounding = Rounding::same(8.0);

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = palette.canvas_bg;
        widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.muted);
        widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.separator);
        widgets.inactive.bg_fill = palette.widget_bg;
        widgets.inactive.weak_bg_fill = palette.widget_bg;
        widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.icon_bright);
        widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.separator);
        widgets.inactive.rounding = Rounding::same(6.0);
        widgets.hovered.bg_fill = palette.hover_bg;
        widgets.hovered.weak_bg_fill = palette.hover_bg;
        widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.icon_bright);
        widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.accent);
        widgets.hovered.rounding = Rounding::same(6.0);
        widgets.active.bg_fill = palette.accent;
        widgets.active.weak_bg_fill = palette.accent;
        widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
        widgets.active.bg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
        widgets.active.rounding = Rounding::same(6.0);
        widgets.open.bg_fill = palette.hover_bg;
        widgets.open.fg_stroke = Stroke::new(1.0_f32, palette.icon_bright);
        widgets.open.rounding = Rounding::same(6.0);

        ctx.set_visuals(visuals);

        ctx.style_mut(|style| {
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);

            // Échelle typographique : posée ici, nulle part ailleurs. Un
            // `ui.label` ou un `ui.button` sans `RichText` lit ces entrées, il
            // n'a donc pas besoin d'être recalibré à l'appel.
            for (text_style, font) in [
                (TextStyle::Small, FontId::proportional(CAPTION)),
                (TextStyle::Body, FontId::proportional(BODY)),
                (TextStyle::Button, FontId::proportional(BODY)),
                (TextStyle::Monospace, FontId::monospace(MONO)),
                (TextStyle::Heading, FontId::proportional(TITLE)),
            ] {
                style.text_styles.insert(text_style, font);
            }
        });
    }
}

const DARK: Palette = Palette {
    canvas_bg: Color32::from_rgb(0x30, 0x30, 0x30),
    chrome_bg: Color32::from_rgb(0x28, 0x28, 0x28),
    widget_bg: Color32::from_rgb(0x38, 0x38, 0x38),
    hover_bg: Color32::from_rgb(0x48, 0x48, 0x48),
    accent: Color32::from_rgb(0x7A, 0x7E, 0xF6),
    muted: Color32::from_rgb(0xA0, 0xA0, 0xA0),
    icon: Color32::from_rgb(0xC8, 0xC8, 0xC8),
    icon_bright: Color32::from_rgb(0xE8, 0xE8, 0xE8),
    thumb_tint: Color32::from_rgb(0xE8, 0xE8, 0xF8),
    separator: Color32::from_rgb(0x3A, 0x3A, 0x3A),
    redact_red: Color32::from_rgb(0xE0, 0x4F, 0x5F),
    search_match: Color32::from_rgba_premultiplied(0x50, 0x40, 0x05, 0x6E),
};

const LIGHT: Palette = Palette {
    canvas_bg: Color32::from_rgb(0xEC, 0xEC, 0xEE),
    chrome_bg: Color32::from_rgb(0xF6, 0xF6, 0xF8),
    widget_bg: Color32::from_rgb(0xE4, 0xE4, 0xE8),
    hover_bg: Color32::from_rgb(0xD4, 0xD4, 0xDA),
    accent: Color32::from_rgb(0x5B, 0x5F, 0xE0),
    muted: Color32::from_rgb(0x66, 0x66, 0x6E),
    icon: Color32::from_rgb(0x44, 0x44, 0x4C),
    icon_bright: Color32::from_rgb(0x1E, 0x1E, 0x24),
    thumb_tint: Color32::from_rgb(0x30, 0x30, 0x38),
    separator: Color32::from_rgb(0xD0, 0xD0, 0xD6),
    redact_red: Color32::from_rgb(0xC8, 0x38, 0x48),
    search_match: Color32::from_rgba_premultiplied(0xF0, 0xD0, 0x40, 0x9C),
};

const MINT: Palette = Palette {
    canvas_bg: Color32::from_rgb(0xDF, 0xEE, 0xE7),
    chrome_bg: Color32::from_rgb(0xF0, 0xFA, 0xF4),
    widget_bg: Color32::from_rgb(0xDC, 0xEC, 0xE3),
    hover_bg: Color32::from_rgb(0xC8, 0xE0, 0xD3),
    accent: Color32::from_rgb(0x18, 0x9E, 0x77),
    muted: Color32::from_rgb(0x4E, 0x6B, 0x5D),
    icon: Color32::from_rgb(0x2E, 0x4A, 0x3D),
    icon_bright: Color32::from_rgb(0x14, 0x2C, 0x21),
    thumb_tint: Color32::from_rgb(0x1E, 0x3A, 0x2C),
    separator: Color32::from_rgb(0xC2, 0xDA, 0xCD),
    redact_red: Color32::from_rgb(0xC8, 0x38, 0x48),
    search_match: Color32::from_rgba_premultiplied(0xF5, 0xD8, 0x50, 0x9C),
};

const STARRY: Palette = Palette {
    canvas_bg: Color32::from_rgb(0x08, 0x0A, 0x18),
    chrome_bg: Color32::from_rgb(0x12, 0x16, 0x2A),
    widget_bg: Color32::from_rgb(0x1C, 0x22, 0x3C),
    hover_bg: Color32::from_rgb(0x2A, 0x32, 0x52),
    accent: Color32::from_rgb(0x8E, 0x9B, 0xFF),
    muted: Color32::from_rgb(0x9A, 0xA4, 0xC8),
    icon: Color32::from_rgb(0xC2, 0xCA, 0xEA),
    icon_bright: Color32::from_rgb(0xEE, 0xF2, 0xFF),
    thumb_tint: Color32::from_rgb(0xE0, 0xE8, 0xFF),
    separator: Color32::from_rgb(0x26, 0x2E, 0x4C),
    redact_red: Color32::from_rgb(0xFF, 0x6B, 0x7A),
    search_match: Color32::from_rgba_premultiplied(0x60, 0x50, 0x10, 0x7E),
};

thread_local! {
    /// Palette courante, lue par tout le chrome.
    static PALETTE: Cell<Palette> = const { Cell::new(DARK) };
}

/// Palette courante.
pub fn pal() -> Palette {
    PALETTE.with(Cell::get)
}

/// Installe une palette comme palette courante.
///
/// À appeler au début d'une trame, avant toute peinture.
pub fn set_palette(palette: Palette) {
    PALETTE.with(|cell| cell.set(palette));
}
