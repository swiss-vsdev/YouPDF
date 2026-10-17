//! Panneau IA : « apportez votre propre fournisseur ».
//!
//! YouPDF n'héberge aucune IA et n'en impose aucune. Le panneau ne parle
//! qu'au fournisseur que l'utilisateur configure, et seulement quand
//! l'interrupteur général est ouvert. Interrupteur fermé : aucun
//! fournisseur n'est construit, le trousseau système n'est pas lu, aucune
//! connexion n'est ouverte.
//!
//! L'appel réseau tourne sur un fil de travail : l'interface ne se fige
//! jamais, même quand le fournisseur ne répond pas.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use egui::{RichText, ScrollArea, TextEdit};
use youpdf_ai::{
    build_provider, AiTask, AppSettings, ChatContext, ChatOptions, Message, ModelInfo, ProviderConfig,
    ProviderKind, SecretStore,
};
use youpdf_shared::pal;

/// Demande envoyée au fil de travail.
///
/// La configuration part telle quelle : c'est le fil de travail qui construit
/// le fournisseur, donc qui lit le trousseau et ouvre le client HTTP. Le fil
/// d'interface ne touche jamais au trousseau.
enum AiRequest {
    /// Lister les modèles du fournisseur.
    ListModels(ProviderConfig),
    /// Envoyer une conversation.
    Chat {
        config: ProviderConfig,
        model: String,
        messages: Vec<Message>,
    },
}

/// Réponse renvoyée par le fil de travail.
enum AiReply {
    Models(Vec<ModelInfo>),
    Chat(String),
    Error(String),
}

/// Fil de travail : un runtime tokio, une demande à la fois.
///
/// Le fil n'existe que si une demande a été envoyée : tant que l'IA est
/// éteinte, aucun fil, aucun runtime, aucune lecture du trousseau.
///
/// `cancel` est une génération partagée avec l'interface : couper l'IA
/// l'incrémente, et le fil abandonne alors toute demande encore en file
/// **avant** d'ouvrir la moindre connexion.
fn spawn_worker(
    requests: Receiver<AiRequest>,
    replies: Sender<AiReply>,
    cancel: Arc<AtomicU64>,
) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(e) => {
                let _ = replies.send(AiReply::Error(format!("runtime IA : {e}")));
                return;
            }
        };

        while let Ok(request) = requests.recv() {
            let generation = cancel.load(Ordering::Relaxed);
            // La lecture du trousseau peut prendre un instant : on revérifie
            // juste avant l'appel réseau que l'IA n'a pas été coupée entre
            // temps. Sinon, on abandonne sans rien envoyer.
            let alive = || cancel.load(Ordering::Relaxed) == generation;

            let reply = match request {
                AiRequest::ListModels(config) => match build_provider(&config) {
                    Ok(provider) if alive() => runtime.block_on(async {
                        provider
                            .list_models()
                            .await
                            .map(AiReply::Models)
                            .unwrap_or_else(|e| AiReply::Error(e.to_string()))
                    }),
                    Ok(_) => continue,
                    Err(e) => AiReply::Error(e.to_string()),
                },
                AiRequest::Chat {
                    config,
                    model,
                    messages,
                } => match build_provider(&config) {
                    Ok(provider) if alive() => runtime.block_on(async {
                        provider
                            .chat(&model, &messages, &ChatOptions::default())
                            .await
                            .map(|response| AiReply::Chat(response.content))
                            .unwrap_or_else(|e| AiReply::Error(e.to_string()))
                    }),
                    Ok(_) => continue,
                    Err(e) => AiReply::Error(e.to_string()),
                },
            };

            if replies.send(reply).is_err() {
                break;
            }
        }
    });
}

/// Un tour de conversation, affiché dans le journal.
struct Turn {
    role: &'static str,
    text: String,
}

/// Panneau IA.
pub struct AiPanel {
    /// Fenêtre ouverte.
    pub open: bool,
    /// Configuration persistée.
    pub settings: AppSettings,
    /// Saisie de clé pour le fournisseur sélectionné.
    key_input: String,
    /// Modèles listés pour le fournisseur actif.
    models: Vec<ModelInfo>,
    /// Journal de conversation.
    turns: Vec<Turn>,
    /// Question en cours de saisie.
    input: String,
    /// Tâche sélectionnée.
    task: AiTask,
    /// Langue cible de la traduction.
    target_language: String,
    /// Dernier message d'état.
    status: Option<String>,
    /// Une demande est en vol.
    busy: bool,
    /// Canal vers le fil de travail ; `None` tant qu'aucune demande n'a été
    /// envoyée, donc tant que l'IA n'a pas servi.
    requests: Option<Sender<AiRequest>>,
    /// Canal depuis le fil de travail.
    replies: Option<Receiver<AiReply>>,
    /// Génération de travail : incrémentée à chaque coupure de l'IA, pour
    /// qu'une demande encore en file soit abandonnée avant tout envoi.
    cancel: Arc<AtomicU64>,
}

impl AiPanel {
    /// Construit le panneau. Aucun fil n'est démarré ici : le fil de travail
    /// naît à la première demande, jamais quand l'IA est éteinte.
    pub fn new() -> Self {
        Self::with_settings(AppSettings::load().unwrap_or_default())
    }

    /// Construit le panneau à partir de réglages déjà chargés.
    ///
    /// L'application charge la configuration une seule fois et la partage :
    /// le thème et le panneau IA lisent le même fichier.
    pub fn with_settings(settings: AppSettings) -> Self {
        Self {
            open: false,
            settings,
            key_input: String::new(),
            models: Vec::new(),
            turns: Vec::new(),
            input: String::new(),
            task: AiTask::Chat,
            target_language: "English".to_string(),
            status: None,
            busy: false,
            requests: None,
            replies: None,
            cancel: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Interrupteur général de l'IA.
    pub fn enabled(&self) -> bool {
        self.settings.ai_enabled
    }

    /// Envoie une demande au fil de travail, en le démarrant au besoin.
    fn send(&mut self, request: AiRequest) {
        if self.requests.is_none() {
            let (request_tx, request_rx) = channel();
            let (reply_tx, reply_rx) = channel();
            spawn_worker(request_rx, reply_tx, Arc::clone(&self.cancel));
            self.requests = Some(request_tx);
            self.replies = Some(reply_rx);
        }
        let sent = self
            .requests
            .as_ref()
            .is_some_and(|requests| requests.send(request).is_ok());
        if sent {
            self.busy = true;
        } else {
            // Le fil de travail est mort (échec de construction du runtime) :
            // le dire au lieu de laisser « en attente » à l'écran pour toujours.
            self.requests = None;
            self.replies = None;
            self.busy = false;
            self.status = Some("The AI worker stopped. Reopen the panel to retry.".to_string());
        }
    }

    /// Coupe l'IA : jette les canaux du fil de travail.
    ///
    /// Le fil se termine de lui-même (son `recv` échoue). Une requête déjà
    /// partie sur le réseau ne peut pas être rappelée, mais toute demande
    /// encore en file est abandonnée avant l'envoi grâce à la génération, et
    /// la réponse en vol ne peut plus être livrée.
    fn disable(&mut self) {
        self.cancel.fetch_add(1, Ordering::Relaxed);
        self.requests = None;
        self.replies = None;
        self.busy = false;
        self.status = None;
        self.models.clear();
    }

    /// Recueille les réponses du fil de travail.
    fn poll(&mut self) {
        let mut inbox = Vec::new();
        if let Some(replies) = &self.replies {
            while let Ok(reply) = replies.try_recv() {
                inbox.push(reply);
            }
        }
        for reply in inbox {
            self.busy = false;
            match reply {
                AiReply::Models(models) => {
                    self.status = Some(format!("{} model(s) available.", models.len()));
                    self.models = models;
                }
                AiReply::Chat(text) => {
                    self.status = Some("Done.".to_string());
                    self.turns.push(Turn { role: "AI", text });
                }
                AiReply::Error(message) => self.status = Some(message),
            }
        }
    }

    /// Configuration du fournisseur actif.
    fn active_config(&self) -> Option<youpdf_ai::ProviderConfig> {
        let id = self.settings.active_provider.as_deref()?;
        self.settings.providers.iter().find(|p| p.id == id).cloned()
    }

    /// Demande la liste des modèles du fournisseur actif.
    fn list_models(&mut self) {
        if self.busy {
            self.status = Some("A request is already running.".to_string());
            return;
        }
        let Some(config) = self.active_config() else {
            self.status = Some("Select a provider first.".to_string());
            return;
        };
        self.send(AiRequest::ListModels(config));
        self.status = Some("Listing models…".to_string());
    }

    /// Envoie la tâche sélectionnée au fournisseur actif.
    fn submit(&mut self, doc_text: &str, doc_name: Option<&str>, doc_pages: Option<usize>) {
        // Une seule demande à la fois : sinon une seconde question resterait
        // en file et partirait après une coupure de l'IA.
        if self.busy {
            self.status = Some("A request is already running.".to_string());
            return;
        }
        if matches!(self.task, AiTask::Chat) && self.input.trim().is_empty() {
            self.status = Some("Type a question first.".to_string());
            return;
        }
        let Some(config) = self.active_config() else {
            self.status = Some("Select a provider first.".to_string());
            return;
        };
        let model = self
            .settings
            .active_model
            .clone()
            .or_else(|| config.default_model.clone());
        let Some(model) = model else {
            self.status = Some("Choose a model first.".to_string());
            return;
        };

        let context = ChatContext {
            document_name: doc_name.map(|name| name.to_string()),
            document_text: Some(doc_text.to_string()),
            page_count: doc_pages,
            target_language: Some(self.target_language.clone()),
        };
        let messages = self.task.build_messages(&context, &self.input);

        let label = match self.task {
            AiTask::Chat => self.input.clone(),
            other => task_label(other).to_string(),
        };

        self.turns.push(Turn {
            role: "You",
            text: label,
        });
        self.input.clear();
        self.send(AiRequest::Chat {
            config,
            model,
            messages,
        });
        self.status = Some("Waiting for the model…".to_string());
    }

    /// Peint la fenêtre. `doc_text` est le texte déjà extrait du document actif.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        doc_text: &str,
        doc_name: Option<&str>,
        doc_pages: Option<usize>,
    ) {
        if !self.open {
            return;
        }
        self.poll();

        let mut open = self.open;
        egui::Window::new("AI assistant")
            .open(&mut open)
            .default_size([560.0, 640.0])
            .min_width(420.0)
            .show(ctx, |ui| {
                self.body(ui, doc_text, doc_name, doc_pages);
            });
        self.open = open;
    }

    /// Contenu de la fenêtre.
    fn body(
        &mut self,
        ui: &mut egui::Ui,
        doc_text: &str,
        doc_name: Option<&str>,
        doc_pages: Option<usize>,
    ) {
        let mut enabled = self.settings.ai_enabled;
        if ui.checkbox(&mut enabled, "Enable AI features").changed() {
            self.settings.ai_enabled = enabled;
            if !enabled {
                self.disable();
            }
            let _ = self.settings.save();
        }

        if !self.settings.ai_enabled {
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "AI is off. YouPDF sends nothing anywhere while this is off: no provider is \
                     built, the system keyring is not read, and no network connection is opened.",
                )
                .size(12.0)
                .color(pal().muted),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "YouPDF hosts no AI of its own. When you turn this on, you pick a provider \
                     and supply your own key (or point at a local server such as Ollama). Your \
                     document text goes only to that provider, and only when you press a button.",
                )
                .size(12.0)
                .color(pal().muted),
            );
            return;
        }

        ui.label(
            RichText::new("Bring your own provider. No telemetry. Nothing leaves this machine unless you press a button.")
                .size(11.5)
                .color(pal().muted),
        );
        ui.separator();

        // --- Choix du fournisseur -----------------------------------------
        ui.label(RichText::new("Provider").strong());
        let mut chosen: Option<String> = None;
        for config in &self.settings.providers {
            let active = self.settings.active_provider.as_deref() == Some(config.id.as_str());
            ui.horizontal(|ui| {
                if ui.radio(active, &config.display_name).clicked() {
                    chosen = Some(config.id.clone());
                }
                ui.label(RichText::new(kind_label(config.kind)).size(10.5).color(pal().muted));
            });
        }
        if let Some(id) = chosen {
            if self.settings.active_provider.as_deref() != Some(id.as_str()) {
                // Un identifiant de modèle n'a de sens que chez le fournisseur
                // qui l'a listé : le garder enverrait un modèle inconnu.
                self.settings.active_model = None;
            }
            self.settings.active_provider = Some(id);
            self.models.clear();
            self.status = None;
            let _ = self.settings.save();
        }

        // --- Détails du fournisseur actif ---------------------------------
        let index = self
            .settings
            .providers
            .iter()
            .position(|p| Some(p.id.as_str()) == self.settings.active_provider.as_deref());

        let mut do_list = false;
        let mut do_save_key = false;
        let mut do_forget_key = false;
        let mut do_submit = false;

        if let Some(index) = index {
            let mut config = self.settings.providers[index].clone();
            let config_id = config.id.clone();
            let mut changed = false;

            ui.add_space(4.0);

            // URL de base : utile pour les fournisseurs locaux ou compatibles.
            if matches!(
                config.kind,
                ProviderKind::Ollama | ProviderKind::OpenAiCompat | ProviderKind::Gemini
            ) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Base URL").size(11.5).color(pal().muted));
                    let mut url = config.base_url.clone().unwrap_or_default();
                    if ui
                        .add(TextEdit::singleline(&mut url).desired_width(330.0))
                        .changed()
                    {
                        config.base_url = if url.trim().is_empty() {
                            None
                        } else {
                            Some(url)
                        };
                        changed = true;
                    }
                });
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new("Model").size(11.5).color(pal().muted));
                let mut model = self.settings.active_model.clone().unwrap_or_default();
                if ui
                    .add(
                        TextEdit::singleline(&mut model)
                            .desired_width(230.0)
                            .hint_text("model id"),
                    )
                    .changed()
                {
                    self.settings.active_model = if model.trim().is_empty() {
                        None
                    } else {
                        Some(model)
                    };
                    changed = true;
                }
                if ui.button("List models").clicked() {
                    do_list = true;
                }
            });

            // Clé API : écrite dans le trousseau système, jamais dans le fichier.
            ui.horizontal(|ui| {
                ui.label(RichText::new("API key").size(11.5).color(pal().muted));
                ui.add(
                    TextEdit::singleline(&mut self.key_input)
                        .password(true)
                        .desired_width(200.0)
                        .hint_text("stored in the OS keyring"),
                );
                if ui.button("Save").clicked() {
                    do_save_key = true;
                }
                if ui.button("Forget").clicked() {
                    do_forget_key = true;
                }
            });

            if config.kind == ProviderKind::Ollama {
                ui.label(
                    RichText::new("Ollama runs locally and needs no API key.")
                        .size(11.0)
                        .color(pal().muted),
                );
            }

            // Modèles proposés par le fournisseur.
            let models = self.models.clone();
            if !models.is_empty() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for model in &models {
                        let selected =
                            self.settings.active_model.as_deref() == Some(model.id.as_str());
                        if ui
                            .selectable_label(selected, RichText::new(&model.display_name).size(11.0))
                            .clicked()
                        {
                            self.settings.active_model = Some(model.id.clone());
                            let _ = self.settings.save();
                        }
                    }
                });
            }

            if changed {
                self.settings.providers[index] = config;
                let _ = self.settings.save();
            }

            // Actions différées : elles empruntent tout `self`.
            if do_list {
                self.list_models();
            }
            if do_save_key {
                let key = self.key_input.trim().to_string();
                if key.is_empty() {
                    self.status = Some("Type a key first.".to_string());
                } else {
                    match SecretStore::set_api_key(&config_id, &key) {
                        Ok(()) => {
                            self.key_input.clear();
                            self.status = Some("Key saved to the OS keyring.".to_string());
                        }
                        Err(e) => self.status = Some(e.to_string()),
                    }
                }
            }
            if do_forget_key {
                match SecretStore::delete_api_key(&config_id) {
                    Ok(()) => self.status = Some("Key removed from the OS keyring.".to_string()),
                    Err(e) => self.status = Some(e.to_string()),
                }
            }
        }

        // --- Tâche --------------------------------------------------------
        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new("Task").strong());
        ui.horizontal_wrapped(|ui| {
            for task in [
                AiTask::Chat,
                AiTask::Summarize,
                AiTask::KeyPoints,
                AiTask::Translate,
                AiTask::Explain,
                AiTask::LegalReview,
                AiTask::FinancialReview,
            ] {
                if ui
                    .selectable_label(self.task == task, task_label(task))
                    .clicked()
                {
                    self.task = task;
                }
            }
        });
        if self.task == AiTask::Translate {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Translate to").size(11.5).color(pal().muted));
                ui.add(TextEdit::singleline(&mut self.target_language).desired_width(160.0));
            });
        }

        // --- Journal ------------------------------------------------------
        ui.separator();
        let context_label = format!(
            "Document context: {} ({} page(s))",
            doc_name.unwrap_or("(none)"),
            doc_pages.unwrap_or(0)
        );
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(context_label)
                    .size(11.0)
                    .color(pal().muted),
            );
            if !doc_text.is_empty() && ui.button("Clear").clicked() {
                self.turns.clear();
            }
        });

        ScrollArea::vertical()
            .max_height(210.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for turn in &self.turns {
                    ui.label(RichText::new(turn.role).strong().size(11.5).color(pal().accent));
                    ui.label(RichText::new(&turn.text).size(12.0));
                    ui.add_space(6.0);
                }
            });

        // --- Saisie -------------------------------------------------------
        ui.separator();
        let enter =
            ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        let clicked = ui
            .horizontal(|ui| {
                let response = ui.add(
                    TextEdit::multiline(&mut self.input)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text("Ask about the document… (⌘⏎ to send)"),
                );
                let _ = response;
                ui.button("Send").clicked()
            })
            .inner;
        if clicked || enter {
            do_submit = true;
        }

        if self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Working…").size(11.0).color(pal().muted));
            });
        }
        if let Some(status) = &self.status {
            ui.label(RichText::new(status).size(11.0).color(pal().muted));
        }

        if do_submit {
            self.submit(doc_text, doc_name, doc_pages);
        }
    }
}

impl Default for AiPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Nom lisible d'un type de fournisseur.
fn kind_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAi => "OpenAI",
        ProviderKind::OpenAiCompat => "OpenAI-compatible",
        ProviderKind::Anthropic => "Anthropic",
        ProviderKind::Gemini => "Google Gemini",
        ProviderKind::Ollama => "Ollama (local)",
    }
}

/// Nom lisible d'une tâche.
fn task_label(task: AiTask) -> &'static str {
    match task {
        AiTask::Chat => "Chat",
        AiTask::Summarize => "Summarize",
        AiTask::KeyPoints => "Key points",
        AiTask::Translate => "Translate",
        AiTask::Explain => "Explain",
        AiTask::LegalReview => "Legal review",
        AiTask::FinancialReview => "Financial review",
    }
}