//! Widgets maison : boutons, pastilles et lignes de menu.

use super::*;

/// Pastille de mode / d'outil : icône + libellé (+ `▾`), fond si actif.
pub(crate) fn pill_button(
    ui: &mut egui::Ui,
    label: &str,
    draw: IconFn,
    chevron: bool,
    primary: bool,
    selected: bool,
) -> egui::Response {
    // Texte blanc sur l'accent ; la teinte du thème ailleurs, pour rester
    // lisible quand le thème est clair.
    let text_color = if primary || selected {
        Color32::WHITE
    } else {
        pal().icon_bright
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(BODY),
        text_color,
    );
    let width = 12.0
        + 15.0
        + 7.0
        + galley.size().x
        + if chevron { 15.0 } else { 0.0 }
        + 12.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    if primary || selected {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), pal().accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), pal().hover_bg);
    }
    let icon_rect = Rect::from_center_size(
        pos2(rect.min.x + 12.0 + 7.5, rect.center().y),
        vec2(15.0, 15.0),
    );
    draw(ui.painter(), icon_rect, text_color);
    ui.painter().galley(
        pos2(
            rect.min.x + 12.0 + 15.0 + 7.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_color,
    );
    if chevron {
        let ch_rect = Rect::from_center_size(
            pos2(rect.max.x - 12.0 - 5.0, rect.center().y),
            vec2(9.0, 9.0),
        );
        icon_chevron(ui.painter(), ch_rect, pal().muted);
    }
    resp
}

/// Ligne de menu cochable : la place de la coche est réservée même quand
/// l'entrée ne l'est pas, sinon les libellés d'un même groupe danseraient
/// d'une ligne à l'autre.
pub(crate) fn menu_row_checked(ui: &mut egui::Ui, label: &str, checked: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(210.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), pal().hover_bg);
    }
    if checked {
        glyph(
            ui.painter(),
            Rect::from_center_size(pos2(rect.min.x + 18.0, rect.center().y), vec2(15.0, 15.0)),
            pal().accent,
            ph::CHECK,
        );
    }
    ui.painter().text(
        pos2(rect.min.x + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(BODY),
        pal().icon_bright,
    );
    resp.clicked()
}

/// Séparateur de menu, aligné sur la largeur des lignes.
pub(crate) fn menu_separator(ui: &mut egui::Ui) {
    ui.add_space(SPACE_S);
    let y = ui.cursor().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        y,
        Stroke::new(1.0_f32, pal().separator),
    );
    ui.add_space(SPACE_M);
}

/// Entrée qui ouvre un sous-menu au clic.
pub(crate) fn menu_submenu(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let text = RichText::new(format!("{label}  ▸")).size(BODY);
    ui.menu_button(text, |ui| {
        ui.set_min_width(180.0);
        add(ui);
    });
}

/// Ligne d'un sous-menu.
pub(crate) fn submenu_row(ui: &mut egui::Ui, label: &str, icon: Option<fn(&egui::Painter, egui::Rect, egui::Color32)>) -> bool {
    let desired_width = if icon.is_some() { 200.0 } else { 180.0 };
    let button = Button::new(RichText::new(label))
        .min_size(vec2(desired_width, 24.0));
    let resp = ui.add(button);
    // Draw icon if provided, positioned to the right of the label
    if let Some(draw_icon) = icon {
        let icon_size = 14.0; // Smaller icon for submenus
        let icon_x = resp.rect.min.x + resp.rect.width() - 10.0 - icon_size;
        let icon_rect = egui::Rect::from_min_max(
            egui::pos2(icon_x, resp.rect.min.y + (resp.rect.height() - icon_size) / 2.0),
            egui::pos2(icon_x + icon_size, resp.rect.max.y - (resp.rect.height() - icon_size) / 2.0),
        );
        draw_icon(ui.painter(), icon_rect, pal().icon_bright);
    }
    resp.clicked
}

/// Ligne de menu `Outils` : libellé à gauche, raccourci à droite.
pub(crate) fn menu_row(ui: &mut egui::Ui, label: &str, shortcut: &str, icon: Option<fn(&egui::Painter, egui::Rect, egui::Color32)>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(210.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), pal().hover_bg);
    }
    let label_x = rect.min.x + 10.0;
    let shortcut_x = rect.max.x - 10.0;
    let icon_size = 16.0;
    let icon_x = label_x + 20.0 + icon_size;
    // Position icon only if provided and there's space
    if let Some(draw_icon) = icon {
        let icon_rect = egui::Rect::from_min_max(
            egui::pos2(icon_x, rect.min.y + (rect.height() - icon_size) / 2.0),
            egui::pos2(icon_x + icon_size, rect.max.y - (rect.height() - icon_size) / 2.0),
        );
        draw_icon(ui.painter(), icon_rect, pal().icon_bright);
    }
    ui.painter().text(
        pos2(label_x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(BODY),
        pal().icon_bright,
    );
    ui.painter().text(
        pos2(shortcut_x, rect.center().y),
        Align2::RIGHT_CENTER,
        shortcut,
        FontId::proportional(LABEL),
        pal().muted,
    );
    resp.clicked()
}

/// Bouton-icône 38×38 avec fond sélection/hover.
pub(crate) fn rail_button(
    ui: &mut egui::Ui,
    selected: bool,
    draw: IconFn,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_L), pal().accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_L), pal().widget_bg);
    }
    let color = if selected {
        Color32::WHITE
    } else if resp.hovered() {
        pal().icon_bright
    } else {
        pal().icon
    };
    draw(ui.painter(), rect.shrink(9.0), color);
    resp
}

/// Bouton d'action d'un panneau (icône 30×28, fond si actif).
pub(crate) fn panel_action(
    ui: &mut egui::Ui,
    selected: bool,
    draw: IconFn,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), pal().widget_bg);
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, Rounding::same(RADIUS_M), Color32::from_rgb(0x33, 0x33, 0x33));
    }
    let color = if selected || resp.hovered() { pal().icon_bright } else { pal().icon };
    draw(ui.painter(), rect.shrink(8.0), color);
    resp
}

/// Petit bouton d'icône de la barre d'état.
pub(crate) fn status_button(ui: &mut egui::Ui, glyph: &str, tip: &str) -> bool {
    let resp = ui
        .add(Button::new(RichText::new(glyph).size(TITLE).color(pal().icon)).frame(false))
        .on_hover_text(tip);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// Bouton d'action de la fenêtre d'organisation : icône puis libellé.
pub(crate) fn organize_button(ui: &mut egui::Ui, icon: IconFn, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(116.0, 28.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        Rounding::same(RADIUS_M),
        if resp.hovered() { pal().hover_bg } else { pal().widget_bg },
    );
    let color = if resp.hovered() { pal().icon_bright } else { pal().icon };
    icon(
        ui.painter(),
        Rect::from_center_size(pos2(rect.min.x + 15.0, rect.center().y), vec2(14.0, 14.0)),
        color,
    );
    ui.painter().text(
        pos2(rect.min.x + 28.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(BODY),
        color,
    );
    resp.clicked()
}

/// Vrai si le réglage `mode` correspond au zoom courant de l'onglet.
pub(super) fn zoom_choice_matches(mode: ZoomMode, zoom: Option<f32>, fit_page: bool) -> bool {
    match mode {
        ZoomMode::FitWidth => zoom.is_none() && !fit_page,
        ZoomMode::FitPage => zoom.is_none() && fit_page,
        ZoomMode::Fixed(scale) => zoom.is_some_and(|z| (z - scale).abs() < 0.001),
    }
}

/// Message centré d'un panneau vide.
pub(crate) fn panel_hint(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        ui.label(section(title));
        ui.add_space(SPACE_S);
        ui.label(muted(detail, LABEL));
    });
}
