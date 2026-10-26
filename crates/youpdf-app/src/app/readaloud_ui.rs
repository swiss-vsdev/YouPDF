//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel readaloud_ui).

use super::*;

impl YouPdfApp {
    /// Fenêtre « Read Aloud » : voix, vitesse, lecture du texte de la page courante.
    ///
    /// ponytail: le moteur `say` est un processus unique ; `is_playing` est
    /// appelé à chaque trame pour rafraîchir les boutons. Un `request_repaint`
    /// de 250 ms garantit que l'état est mis à jour une fois la lecture finie
    /// (le processus `say` n'envoie aucun événement à l'application).
    pub(super) fn readaloud_window(&mut self, ctx: &egui::Context) {
        if !self.readaloud_open {
            return;
        }
        // Résolution des valeurs avant le show_modal : la fermeture emprunte
        // `self` mutably, et l'intérieur ne peut plus emprunter `self.tabs`.
        let active = self.active;
        let page_text = self
            .tabs
            .get(active)
            .and_then(|tab| tab.doc.page_text(tab.page as u32).ok())
            .unwrap_or_default();
        let page = self.tabs.get(active).map(|t| t.page).unwrap_or(1);
        let total = self.tabs.get(active).map(|t| t.total()).unwrap_or(0);
        let voices = TtsEngine::list_voices();
        let mut open = true;
        // `open` est emprunté par `show_modal` : la fermeture ne peut pas
        // l'écrire depuis l'intérieur. Un drapeau séparé, capturé par le
        // clôture, reçoit la demande de fermeture.
        let mut close = false;

        self.show_modal(
            ctx,
            "modal-readaloud",
            "Read Aloud",
            380.0,
            &mut open,
            |app, ui| {
                ui.label(
                    RichText::new(format!("{page} / {total}"))
                        .size(CAPTION)
                        .color(pal().muted),
                );
                ui.add_space(SPACE_XS);

                ui.horizontal(|ui| {
                    ui.label(section(t("Voice")));
                    let mut picked: Option<String> = None;
                    egui::ComboBox::from_id_salt("readaloud-voice")
                        .selected_text(app.tts.current_voice.clone())
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for voice in voices.iter().take(200) {
                                if ui
                                    .selectable_label(app.tts.current_voice == *voice, voice)
                                    .clicked()
                                {
                                    picked = Some(voice.clone());
                                }
                            }
                        });
                    if let Some(voice) = picked {
                        app.tts.set_voice(voice);
                    }
                });
                ui.add_space(SPACE_M);

                ui.label(section(t("Speed")));
                ui.add(
                    egui::Slider::new(&mut app.tts.rate, 100.0..=300.0)
                        .text("wpm")
                        .show_value(true),
                );
                ui.add_space(SPACE_M);

                ui.label(
                    RichText::new(if page_text.is_empty() {
                        "No text on this page."
                    } else {
                        "Ready to read the current page."
                    })
                    .size(CAPTION)
                    .color(pal().muted),
                );
                ui.add_space(SPACE_L);

                ui.horizontal(|ui| {
                    let playing = app.tts.is_playing();
                    if ui
                        .add_enabled(!playing, egui::Button::new(t("Play")))
                        .clicked()
                    {
                        let _ = app.tts.speak(&page_text);
                    }
                    if ui
                        .add_enabled(playing, egui::Button::new(t("Stop")))
                        .clicked()
                    {
                        app.tts.stop();
                    }
                });
                ui.add_space(SPACE_XS);
                if ui.button(t("Close")).clicked() {
                    close = true;
                }

                // Requête de repaint : le processus `say` ne prévient pas
                // l'application de sa fin, on sonde `is_playing` à chaque trame.
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            },
        );

        if !open || close {
            self.readaloud_open = false;
        }
        // La lecture est relancée avec la page courante : si l'utilisateur a
        // tourné pendant la lecture, le `say` en cours continue sur l'ancienne
        // page ; `stop()` le coupe. Aucune action implicite ici.
    }

}