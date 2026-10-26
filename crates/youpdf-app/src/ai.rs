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
use youpdf_shared::{BODY, LABEL, caption, modal_frame, muted, pal, section, t, SPACE_S, SPACE_L, SPACE_M};

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use egui::{RichText, ScrollArea, TextEdit};
use youpdf_ai::{
    build_provider, AiTask, AppSettings, ChatContext, ChatOptions, Message, ModelInfo, ProviderConfig,
    ProviderKind, SecretStore, ToneStyle,
};

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
    /// Traduire chaque page d'un document, une requête par page.
    ///
    /// Le fil de travail appelle le modèle **une fois par page** (pas une
    /// seule fois pour le document entier) : c'est la règle de la
    /// traduction préservant la mise en page. Les réponses sont renvoyées
    /// dans l'ordre des pages, une chaîne par page.
    TranslateAll {
        config: ProviderConfig,
        model: String,
        target_language: String,
        page_texts: Vec<String>,
    },
}

/// Réponse renvoyée par le fil de travail.
enum AiReply {
    Models(Vec<ModelInfo>),
    Chat(String),
    /// Traduction d'un document entier : une chaîne par page, dans l'ordre.
    /// Les pages sans texte ont une chaîne vide.
    Translated(Vec<String>),
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
    requests: Receiver<(u64, AiRequest)>,
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

        while let Ok((generation, request)) = requests.recv() {
            // `generation` vient de la mise en file, pas du réveil du fil :
            // couper l'IA entre les deux fait échouer ce test.
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
                AiRequest::TranslateAll {
                    config,
                    model,
                    target_language,
                    page_texts,
                } => match build_provider(&config) {
                    Ok(provider) if alive() => runtime.block_on(async {
                        let mut out: Vec<String> = Vec::with_capacity(page_texts.len());
                        for page_text in &page_texts {
                            if !alive() {
                                break;
                            }
                            if page_text.trim().is_empty() {
                                out.push(String::new());
                                continue;
                            }
                            let system = "Tu es un traducteur professionnel. Traduis le                                 texte fourni vers la langue demandée. Préserve les sauts de                                 ligne : chaque ligne d'entrée doit produire une ligne de                                 sortie. Ne commente pas, ne rajoute pas de notes : produis                                 uniquement la traduction.";
                            let messages = vec![
                                Message::system(system),
                                Message::user(format!(
                                    "Traduis ce texte vers {target_language} :\n\n---\n{page_text}\n---"
                                )),
                            ];
                            match provider
                                .chat(&model, &messages, &ChatOptions::default())
                                .await
                            {
                                Ok(response) => out.push(response.content),
                                Err(e) => out.push(format!("[translation error: {e}]")),
                            }
                        }
                        AiReply::Translated(out)
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

/// Défaut détecté sur une page (panneau « Check pages »).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageDefect {
    /// Aucune texte extractible.
    Blank,
    /// Page absente de la table des pages.
    Invalid,
    /// Rotation différente de celle de la majorité des pages.
    NeedsRotation {
        /// Degrés à appliquer pour rejoindre la rotation majoritaire.
        degrees: i32,
    },
    /// Numéro de page attendu différent du numéro trouvé.
    ///
    /// NonConsecutive detector requires PageLabels (Wave 1 Agent A1); stub
    /// returns empty until A1 lands.
    #[allow(dead_code)]
    NonConsecutive { expected: u32, found: u32 },
    /// La page porte des annotations.
    ContainsComments,
    /// Format différent du format majoritaire.
    MismatchedDimensions {
        /// Format majoritaire, en points.
        expected: (f32, f32),
        /// Format de la page, en points.
        found: (f32, f32),
    },
    /// Page scannée : pas de texte, image plein cadre.
    Scanned,
}

/// Défauts d'une page. `page` est 1-indexé.
///
/// Les valeurs majoritaires (format, rotation) sont celles de la page 1 :
/// ponytail: un vrai vote de la majorité demande de parcourir toutes les
/// tailles avant de conclure, à faire si les documents mélangés deviennent
/// courants.
pub fn detect_page_defects(doc: &youpdf_core::Document, page: u32) -> Vec<PageDefect> {
    let mut defects = Vec::new();
    let total = doc.page_count() as u32;
    if page == 0 || page > total {
        defects.push(PageDefect::Invalid);
        return defects;
    }

    let text = doc.page_text(page).unwrap_or_default();
    if text.trim().is_empty() {
        if doc.looks_scanned(page) {
            defects.push(PageDefect::Scanned);
        } else {
            defects.push(PageDefect::Blank);
        }
    }

    // Boîte non tournée (`page_view`) et non `page_size` : une page simplement
    // pivotée doit sortir sous NeedsRotation, pas aussi sous MismatchedDimensions.
    let reference = doc.page_view(1);
    let view = doc.page_view(page);
    if (view.width - reference.width).abs() > 10.0 || (view.height - reference.height).abs() > 10.0 {
        defects.push(PageDefect::MismatchedDimensions {
            expected: (reference.width, reference.height),
            found: (view.width, view.height),
        });
    }
    if view.rotation != reference.rotation {
        defects.push(PageDefect::NeedsRotation {
            degrees: (reference.rotation - view.rotation).rem_euclid(360),
        });
    }

    // Une page annotée porte des commentaires : `read_annotations` les lit
    // toutes, on ne garde que celles de cette page.
    // ponytail: O(annotations) par page ; un compteur par page se justifie si
    // la liste complète ralentit le panneau sur les documents très annotés.
    if doc
        .read_annotations()
        .iter()
        .any(|annot| annot.page as u32 == page)
    {
        defects.push(PageDefect::ContainsComments);
    }

    // NonConsecutive detector requires PageLabels (Wave 1 Agent A1); stub
    // returns empty until A1 lands.
    defects
}

/// Description d'un défaut, pour la liste du panneau.
fn defect_label(defect: &PageDefect) -> String {
    match defect {
        PageDefect::Blank => "Blank page".to_string(),
        PageDefect::Invalid => "Invalid page".to_string(),
        PageDefect::NeedsRotation { degrees } => format!("Needs rotation ({degrees:+}°)"),
        PageDefect::NonConsecutive { expected, found } => {
            format!("Page numbering: expected {expected}, found {found}")
        }
        PageDefect::ContainsComments => "Contains comments".to_string(),
        PageDefect::MismatchedDimensions { expected, found } => format!(
            "Mismatched size ({:.0}×{:.0}, expected {:.0}×{:.0})",
            found.0, found.1, expected.0, expected.1
        ),
        PageDefect::Scanned => "Scanned page (image, no text)".to_string(),
    }
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
    /// Ton choisi quand la tâche est `Tone`.
    tone: ToneStyle,
    /// Langue cible de la traduction.
    target_language: String,
    /// Options de la traduction bilingale (fenêtre « Translate PDF »).
    translate_options: youpdf_core::TranslationOptions,
    /// Résultat de la dernière traduction : une chaîne par page.
    translate_result: Option<Vec<String>>,
    /// Pages visées par la dernière traduction (1-indexées).
    translate_pages: Vec<u32>,
    /// Fenêtre « Translate PDF » (traduction bilingale en place).
    translate_open: bool,
    /// Le bouton « Apply to document » a été pressé : l'application doit
    /// écrire la traduction dans le document actif.
    ///
    /// C'est un drapeau, pas un canal : le panneau IA le pose, et l'application
    /// le lit et le réinitialise à la prochaine trame. Sans `Option`, le
    /// `take()` n'existe pas sur un `bool`.
    pub(crate) translate_apply: Option<bool>,
    /// Défauts relevés par « Check pages » (`None` = jamais lancé).
    defects: Option<Vec<(u32, PageDefect)>>,
    /// Page vers laquelle l'application doit défiler (« Aller à la page »).
    pub goto_page: Option<usize>,
    /// Dernier message d'état.
    status: Option<String>,
    /// Une demande est en vol.
    busy: bool,
    /// Canal vers le fil de travail ; `None` tant qu'aucune demande n'a été
    /// envoyée, donc tant que l'IA n'a pas servi. Chaque message porte la
    /// génération fixée à la mise en file.
    requests: Option<Sender<(u64, AiRequest)>>,
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
            tone: ToneStyle::Professional,
            target_language: "English".to_string(),
            translate_options: youpdf_core::TranslationOptions::default(),
            translate_result: None,
            translate_pages: Vec::new(),
            translate_open: false,
            translate_apply: None,
            defects: None,
            goto_page: None,
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
            .is_some_and(|requests| {
                // La génération est fixée au moment de la mise en file : si
                // l'utilisateur coupe l'IA avant que le fil ne prenne la
                // demande, le fil verra deux générations différentes et
                // n'ouvrira aucune connexion.
                let generation = self.cancel.load(Ordering::Relaxed);
                requests.send((generation, request)).is_ok()
            });
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
                AiReply::Translated(pages) => {
                    self.translate_result = Some(pages);
                    self.status = Some(
                        "Translation ready — apply it to the document from the toolbar."
                            .to_string(),
                    );
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

    /// Traduit chaque page d'un document, une requête IA par page.
    ///
    /// Le texte de chaque page est traduit indépendamment, puis réinjecté
    /// dans le document par [`Document::translate_page`] : le texte original
    /// est effacé par la primitive de caviardage, et la traduction est posée
    /// à un corps réduit jusqu'à ce qu'elle tienne dans la boîte.
    ///
    /// `doc` est le document actif (le fil de travail ne touche pas le
    /// document : il ne fait que le traduire, et c'est l'interface qui
    /// écrit le résultat).
    fn translate_pdf(&mut self, doc: &youpdf_core::Document) {
        if self.busy {
            self.status = Some("A request is already running.".to_string());
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
        let target = self.translate_options.target_language.clone();
        let total = doc.page_count();
        if total == 0 {
            self.status = Some("No pages to translate.".to_string());
            return;
        }
        let page_texts: Vec<String> = (1..=total as u32)
            .map(|page| doc.page_text(page).unwrap_or_default())
            .collect();
        let pages: Vec<u32> = (1..=total as u32).collect();
        self.send(AiRequest::TranslateAll {
            config,
            model,
            target_language: target,
            page_texts,
        });
        self.translate_pages = pages;
        self.translate_result = None;
        self.status = Some("Translating page by page…".to_string());
    }

    /// Applique la traduction reçue au document actif.
    ///
    /// `doc` est mutable : l'interface le remplace par sa copie modifiée
    /// après l'opération. `renderer` est celui de l'application (pdfium, ou
    /// un factice si pdfium n'est pas disponible).
    ///
    /// La méthode est `pub` : c'est l'application (qui possède le document
    /// mutable) qui l'appelle, depuis son panneau IA.
    pub fn apply_translation(
        &mut self,
        doc: &mut youpdf_core::Document,
        renderer: &dyn youpdf_core::Renderer,
    ) -> Result<(), youpdf_shared::Error> {
        let Some(lines_per_page) = &self.translate_result else {
            return Ok(());
        };
        // ponytail: la traduction arrive sous forme de chaîne par page ; on
        // la découpe en lignes par sauts de ligne, sans tenter de
        // rétablir les correspondances avec les lignes d'origine (zip_lines
        // le ferait par position, pas par contenu).
        let mut report = youpdf_core::TranslationReport::default();
        for (page, raw) in self.translate_pages.iter().zip(lines_per_page.iter()) {
            let text = doc.page_text(*page).unwrap_or_default();
            if text.trim().is_empty() {
                report.pages_skipped += 1;
                continue;
            }
            let lines: Vec<String> = raw
                .split(['\n', '\u{c}'])
                .map(|l| l.trim_end().to_string())
                .filter(|l| !l.trim().is_empty())
                .collect();
            doc.translate_page(*page, &self.translate_options, &lines, renderer)?;
            report.pages_translated += 1;
            report.lines_placed += lines.len();
        }
        if report.pages_translated == 0 {
            return Err(youpdf_shared::Error::Pdf("no page translated".into()));
        }
        tracing::info!(
            translated = report.pages_translated,
            skipped = report.pages_skipped,
            lines = report.lines_placed,
            "traduction bilingale appliquée"
        );
        Ok(())
    }

    /// Peint la fenêtre. `doc_text` est le texte déjà extrait du document actif,
    /// `doc` le document lui-même (pour « Check pages »).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        doc: Option<&youpdf_core::Document>,
        doc_text: &str,
        doc_name: Option<&str>,
        doc_pages: Option<usize>,
    ) {
        if !self.open {
            return;
        }
        self.poll();

        let mut open = self.open;
        egui::Window::new(t("AI assistant"))
            .open(&mut open)
            .default_size([560.0, 640.0])
            .min_width(420.0)
            .frame(modal_frame(ctx))
            .show(ctx, |ui| {
                self.body(ui, doc, doc_text, doc_name, doc_pages);
            });
        self.open = open;
    }

    /// Contenu de la fenêtre.
    fn body(
        &mut self,
        ui: &mut egui::Ui,
        doc: Option<&youpdf_core::Document>,
        doc_text: &str,
        doc_name: Option<&str>,
        doc_pages: Option<usize>,
    ) {
        let mut enabled = self.settings.ai_enabled;
        if ui.checkbox(&mut enabled, t("Enable AI features")).changed() {
            self.settings.ai_enabled = enabled;
            if !enabled {
                self.disable();
            }
            let _ = self.settings.save();
        }

        if !self.settings.ai_enabled {
            ui.add_space(SPACE_L);
            ui.label(
                RichText::new(
                    "AI is off. YouPDF sends nothing anywhere while this is off: no provider is \
                     built, the system keyring is not read, and no network connection is opened.",
                )
                
                .color(pal().muted),
            );
            ui.add_space(SPACE_L);
            ui.label(
                RichText::new(
                    "YouPDF hosts no AI of its own. When you turn this on, you pick a provider \
                     and supply your own key (or point at a local server such as Ollama). Your \
                     document text goes only to that provider, and only when you press a button.",
                )
                
                .color(pal().muted),
            );
            return;
        }

        ui.label(
            RichText::new(t("Bring your own provider. No telemetry. Nothing leaves this machine unless you press a button."))
                .size(BODY)
                .color(pal().muted),
        );
        ui.separator();

        // --- Choix du fournisseur -----------------------------------------
        ui.label(RichText::new(t("Provider")).strong());
        let mut chosen: Option<String> = None;
        for config in &self.settings.providers {
            let active = self.settings.active_provider.as_deref() == Some(config.id.as_str());
            ui.horizontal(|ui| {
                if ui.radio(active, &config.display_name).clicked() {
                    chosen = Some(config.id.clone());
                }
                ui.label(caption(kind_label(config.kind)));
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

            ui.add_space(SPACE_S);

            // URL de base : utile pour les fournisseurs locaux ou compatibles.
            if matches!(
                config.kind,
                ProviderKind::Ollama | ProviderKind::OpenAiCompat | ProviderKind::Gemini
            ) {
                ui.horizontal(|ui| {
                    ui.label(section(t("Base URL")));
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
                ui.label(section(t("Model")));
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
                if ui.button(t("List models")).clicked() {
                    do_list = true;
                }
            });

            // Clé API : écrite dans le trousseau système, jamais dans le fichier.
            ui.horizontal(|ui| {
                ui.label(section(t("API key")));
                ui.add(
                    TextEdit::singleline(&mut self.key_input)
                        .password(true)
                        .desired_width(200.0)
                        .hint_text("stored in the OS keyring"),
                );
                if ui.button(t("Save")).clicked() {
                    do_save_key = true;
                }
                if ui.button(t("Forget")).clicked() {
                    do_forget_key = true;
                }
            });

            if config.kind == ProviderKind::Ollama {
                ui.label(
                    RichText::new(t("Ollama runs locally and needs no API key."))
                        .size(LABEL)
                        .color(pal().muted),
                );
            }

            // Modèles proposés par le fournisseur.
            let models = self.models.clone();
            if !models.is_empty() {
                ui.add_space(SPACE_S);
                ui.horizontal_wrapped(|ui| {
                    for model in &models {
                        let selected =
                            self.settings.active_model.as_deref() == Some(model.id.as_str());
                        if ui
                            .selectable_label(selected, RichText::new(&model.display_name).size(LABEL))
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
        ui.add_space(SPACE_M);
        ui.separator();
        ui.label(RichText::new(t("Task")).strong());
        ui.horizontal_wrapped(|ui| {
            for task in [
                AiTask::Chat,
                AiTask::Summarize,
                AiTask::KeyPoints,
                AiTask::Translate,
                AiTask::Explain,
                AiTask::LegalReview,
                AiTask::FinancialReview,
                AiTask::Polish,
                AiTask::Shorten,
                AiTask::Expand,
                AiTask::Proofread,
                AiTask::Tone { style: self.tone },
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
                ui.label(section(t("Translate to")));
                ui.add(TextEdit::singleline(&mut self.target_language).desired_width(160.0));
            });
            if ui.button(t("Whole PDF (Preserve Layout)")).clicked() {
                self.translate_open = true;
            }
        }
        if matches!(self.task, AiTask::Tone { .. }) {
            ui.horizontal(|ui| {
                ui.label(section(t("Tone")));
                egui::ComboBox::from_id_salt("ai-tone")
                    .selected_text(self.tone.as_str())
                    .show_ui(ui, |ui| {
                        for style in [
                            ToneStyle::Professional,
                            ToneStyle::Casual,
                            ToneStyle::Confident,
                            ToneStyle::Friendly,
                        ] {
                            if ui
                                .selectable_value(&mut self.tone, style, style.as_str())
                                .changed()
                            {
                                self.task = AiTask::Tone { style };
                            }
                        }
                    });
            });
        }

        // --- Check pages --------------------------------------------------
        ui.horizontal(|ui| {
            if ui.button(t("Check pages")).clicked() {
                self.defects = doc.map(|document| {
                    (1..=document.page_count() as u32)
                        .flat_map(|page| {
                            detect_page_defects(document, page)
                                .into_iter()
                                .map(move |defect| (page, defect))
                        })
                        .collect()
                });
            }
            if let Some(defects) = &self.defects {
                ui.label(muted(format!("{} defect(s)", defects.len()), LABEL));
            }
        });
        let mut goto = None;
        if let Some(defects) = &self.defects {
            if defects.is_empty() {
                ui.label(muted(t("No defects found."), LABEL));
            } else {
                ScrollArea::vertical()
                    .max_height(140.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (page, defect) in defects {
                            ui.horizontal(|ui| {
                                ui.label(format!("{page}"));
                                ui.label(muted(defect_label(defect), LABEL));
                                if ui.button(t("Go to page")).clicked() {
                                    goto = Some(*page as usize);
                                }
                            });
                        }
                    });
            }
        }
        if goto.is_some() {
            self.goto_page = goto;
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
                    .size(LABEL)
                    .color(pal().muted),
            );
            if !doc_text.is_empty() && ui.button(t("Clear")).clicked() {
                self.turns.clear();
            }
        });

        ScrollArea::vertical()
            .max_height(210.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for turn in &self.turns {
                    ui.label(RichText::new(turn.role).strong().size(BODY).color(pal().accent));
                    ui.label(RichText::new(&turn.text));
                    ui.add_space(SPACE_M);
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
                ui.button(t("Send")).clicked()
            })
            .inner;
        if clicked || enter {
            do_submit = true;
        }

        if self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted(t("Working…"), LABEL));
            });
        }
        if let Some(status) = &self.status {
            ui.label(muted(status, LABEL));
        }

        if do_submit {
            self.submit(doc_text, doc_name, doc_pages);
        }
    }

    /// Fenêtre « Translate PDF » : cible, bilingue, préservation du layout.
    ///
    /// L'opération est en deux temps : d'abord on demande la traduction au
    /// fournisseur (une requête par page), puis on l'applique au document
    /// par le bouton « Apply to document ». C'est la règle de la
    /// traduction en place : le fil de travail ne touche pas le document,
    /// il ne le traduit que, et c'est l'interface qui écrit le résultat.
    pub fn translate_window(&mut self, ctx: &egui::Context, doc: Option<&youpdf_core::Document>) {
        let mut open = self.translate_open;
        let mut do_translate = false;
        let mut do_apply = false;
        egui::Window::new(t("Translate PDF (preserve layout)"))
            .id(egui::Id::new("translate-pdf"))
            .open(&mut open)
            .default_size([420.0, 280.0])
            .min_width(360.0)
            .frame(modal_frame(ctx))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Translate every page in place. The original text is white-out and \
                         the translation is placed in the same box, with the font shrunk \
                         until it fits. Approximate result: multi-column layouts and \
                         tables are not preserved.",
                    )
                    .size(BODY)
                    .color(pal().muted),
                );
                ui.add_space(SPACE_S);
                ui.label(muted("Approximate result — layout is not reflowed.", LABEL));
                ui.add_space(SPACE_M);

                ui.horizontal(|ui| {
                    ui.label(section(t("Translate to")));
                    ui.add(TextEdit::singleline(&mut self.translate_options.target_language).desired_width(180.0));
                });
                ui.add_space(SPACE_S);
                ui.checkbox(&mut self.translate_options.bilingual, t("Bilingual (original + translation)"));
                ui.checkbox(&mut self.translate_options.preserve_layout, t("Preserve layout (shrink to fit)"));
                ui.add_space(SPACE_M);

                let Some(document) = doc else {
                    ui.label(muted("Open a document first.", LABEL));
                    return;
                };
                let total = document.page_count();
                ui.label(muted(format!("{} page(s) to translate", total), LABEL));
                ui.add_space(SPACE_S);

                if self.busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(muted(t("Working…"), LABEL));
                    });
                } else if let Some(pages) = &self.translate_result {
                    let done = pages.iter().filter(|p| !p.trim().is_empty()).count();
                    ui.label(muted(format!("{done}/{total} page(s) translated"), LABEL));
                    ui.add_space(SPACE_S);
                    if ui.button(t("Apply to document")).clicked() {
                        do_apply = true;
                    }
                } else {
                    if ui.button(t("Translate")).clicked() {
                        do_translate = true;
                    }
                }
                if let Some(status) = &self.status {
                    ui.add_space(SPACE_S);
                    ui.label(muted(status, LABEL));
                }
            });
        self.translate_open = open;
        if do_translate {
            if let Some(document) = doc {
                self.translate_pdf(document);
            }
        }
        if do_apply {
            self.translate_apply = Some(true);
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
        AiTask::Polish => "Polish",
        AiTask::Shorten => "Shorten",
        AiTask::Expand => "Expand",
        AiTask::Proofread => "Proofread",
        AiTask::Tone { .. } => "Change tone",
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_page_defects, PageDefect};
    use youpdf_core::blank_document;

    /// Page sans texte = Blank, page hors table = Invalid, format différent de
    /// la référence = MismatchedDimensions.
    #[test]
    fn detecte_blank_et_dimensions() {
        let mut doc = blank_document(1, 595.0, 842.0).unwrap();
        assert_eq!(detect_page_defects(&doc, 1), [PageDefect::Blank]);
        assert_eq!(detect_page_defects(&doc, 2), [PageDefect::Invalid]);

        doc.insert_blank_page_after(1, 842.0, 595.0)
            .expect("page insérée");
        assert_eq!(
            detect_page_defects(&doc, 2),
            [
                PageDefect::Blank,
                PageDefect::MismatchedDimensions {
                    expected: (595.0, 842.0),
                    found: (842.0, 595.0),
                },
            ]
        );
    }

    /// Rotation différente de la référence = NeedsRotation.
    #[test]
    fn detecte_rotation_different() {
        let mut doc = blank_document(2, 595.0, 842.0).unwrap();
        doc.rotate_pages(&[2], 180).expect("rotation");
        assert_eq!(
            detect_page_defects(&doc, 2),
            [PageDefect::Blank, PageDefect::NeedsRotation { degrees: 180 }]
        );
    }
}