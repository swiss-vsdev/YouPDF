//! Tâches IA de haut niveau.
//!
//! ## Principe
//!
//! Une « tâche » est une combinaison de :
//!
//! 1. un **prompt système** adapté à l'objectif ;
//! 2. un **contexte** (le texte du document, éventuellement tronqué) ;
//! 3. la **question** de l'utilisateur.
//!
//! Le résultat est une liste de [`Message`] prête à être envoyée à
//! n'importe quel fournisseur via [`crate::AiProvider::chat`].
//!
//! Cette couche est **indépendante du provider** : elle produit des
//! messages neutres, c'est le provider qui les traduit dans son format.

use crate::message::Message;

/// Contexte du document à donner à l'IA.
#[derive(Debug, Clone, Default)]
pub struct ChatContext {
    /// Nom du document (affiché au modèle).
    pub document_name: Option<String>,
    /// Texte extrait du document, éventuellement tronqué.
    pub document_text: Option<String>,
    /// Nombre total de pages (informatif).
    pub page_count: Option<usize>,
    /// Langue cible pour la traduction.
    pub target_language: Option<String>,
}

impl ChatContext {
    /// Construit un contexte à partir d'un texte de document.
    ///
    /// Le texte est tronqué à `max_chars` caractères (UTF-8 safe) pour
    /// rester dans la fenêtre de contexte de la plupart des modèles.
    /// Un marqueur explicite indique au modèle que la fin est tronquée.
    pub fn from_document_text(
        name: Option<String>,
        text: String,
        page_count: Option<usize>,
        max_chars: usize,
    ) -> Self {
        let truncated = if text.chars().count() > max_chars {
            let taken: String = text.chars().take(max_chars).collect();
            format!(
                "{taken}\n\n[…] document tronqué : {} caractères sur {} au total affichés.",
                max_chars,
                text.chars().count()
            )
        } else {
            text
        };
        Self {
            document_name: name,
            document_text: Some(truncated),
            page_count,
            target_language: None,
        }
    }

    pub fn with_target_language(mut self, language: impl Into<String>) -> Self {
        self.target_language = Some(language.into());
        self
    }
}

/// Tâche IA prédéfinie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTask {
    /// Conversation ouverte sur le document.
    Chat,
    /// Résumé du document.
    Summarize,
    /// Traduction vers une langue cible.
    Translate,
    /// Extraction de points clés sous forme de liste.
    KeyPoints,
    /// Explication d'un passage (l'utilisateur fournit le passage).
    Explain,
    /// Analyse juridique (clauses, risques).
    LegalReview,
    /// Analyse financière (tableaux, ratios).
    FinancialReview,
    /// Amélioration d'un texte (grammaire, clarté, flow).
    Polish,
    /// Raccourcissement d'un texte tout en conservant l'essentiel.
    Shorten,
    /// Développement d'un texte pour le rendre plus explicatif.
    Expand,
    /// Relève d'un texte (fautes, style).
    Proofread,
    /// Réécriture dans un ton donné.
    Tone {
        /// Ton souhaité.
        style: ToneStyle,
    },
}

/// Suggestions de questions basées sur le contexte du document.
///
/// Génère des questions suggérées en fonction du type de document ou de son
/// contenu détecté. Retourne jusqu'à `n` questions sous forme de [`Message`]
/// user pour être pré-remplies dans l'interface de chat.
pub fn suggested_questions(_document_name: Option<String>, page_count: Option<usize>, n: usize) -> Vec<Message> {
    use crate::message::Message;
    let mut questions = Vec::new();

    // Questions par défaut qui s'appliquent à la plupart des documents
    let defaults = [
        "Résume ce document.",
        "Quels sont les points clés ?",
        "Explique ce passage.",
        "Y a-t-il des erreurs ou incohérences ?",
    ];

    let count = defaults.len().min(n);
    for i in 0..count {
        questions.push(Message::user(defaults[i].to_string()));
    }

    // Si on a des pages, ajouter une question spécifique
    if let Some(pages) = page_count {
        if pages > 1 && questions.len() < n {
            questions.push(Message::user(format!("Compare les pages 1 et {pages}.")));
        }
    }

    questions
}

/// Ton d'écriture pour la tâche [`AiTask::Tone`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneStyle {
    /// Ton formel, adapté aux documents business/officiels.
    Professional,
    /// Ton décontracté, proche de la conversation.
    Casual,
    /// Ton assuré, sans hésitation.
    Confident,
    /// Ton chaleureux et amical.
    Friendly,
}

impl ToneStyle {
    /// Nom lisible du ton, en anglais minuscule.
    pub fn as_str(&self) -> &'static str {
        match self {
            ToneStyle::Professional => "professional",
            ToneStyle::Casual => "casual",
            ToneStyle::Confident => "confident",
            ToneStyle::Friendly => "friendly",
        }
    }
}

impl AiTask {
    /// Prompt système associé à la tâche.
    pub fn system_prompt(&self) -> &'static str {
        match self {
            AiTask::Chat => {
                "Tu es un assistant documentaire intégré à un éditeur PDF. \
                 Tu réponds de manière concise, précise et factuelle. \
                 Quand tu te réfères au document, cite la page si l'information \
                 est disponible. Si l'information ne figure pas dans le document, \
                 dis-le clairement plutôt que d'inventer. Réponds dans la langue \
                 de la question de l'utilisateur."
            }
            AiTask::Summarize => {
                "Tu es un assistant spécialisé dans la synthèse de documents. \
                 Produis un résumé structuré du document fourni : un paragraphe \
                 d'introduction, puis les points clés sous forme de liste à puces. \
                 Sois fidèle au contenu, ne rajoute rien qui ne soit pas dans le \
                 document. Réponds dans la langue du document."
            }
            AiTask::Translate => {
                "Tu es un traducteur professionnel. Traduis le contenu du document \
                 fourni vers la langue demandée. Préserve la structure (titres, \
                 paragraphes, listes). Ne commente pas, ne rajoute pas de notes : \
                 produis uniquement la traduction."
            }
            AiTask::KeyPoints => {
                "Tu es un assistant qui extrait les points clés d'un document. \
                 Produis une liste à puces de 5 à 10 points essentiels, chacun \
                 en une phrase. Va à l'essentiel, pas de paraphrase inutile. \
                 Réponds dans la langue du document."
            }
            AiTask::Explain => {
                "Tu es un pédagogue. Explique le passage fourni de manière claire \
                 et accessible, comme si tu l'expliquais à quelqu'un qui découvre \
                 le sujet. Utilise des exemples si utile. Réponds dans la langue \
                 du passage."
            }
            AiTask::LegalReview => {
                "Tu es un assistant d'analyse juridique. Analyse le document fourni \
                 et identifie : les clauses importantes, les obligations des parties, \
                 les risques potentiels, les points d'attention. Présente le résultat \
                 sous forme structurée. Tu n'es pas un avocat : rappelle en fin de \
                 réponse que ceci ne remplace pas un conseil juridique professionnel."
            }
            AiTask::FinancialReview => {
                "Tu es un assistant d'analyse financière. Analyse le document fourni \
                 et extrais : les chiffres clés, les tableaux financiers, les ratios \
                 remarquables, les éventuelles anomalies. Présente le résultat de \
                 manière structurée. Ne fais pas de prédiction ni de recommandation \
                 d'investissement."
            }
            AiTask::Polish => {
                "Tu es un relecteur professionnel. Améliore le texte fourni : \
                 corrige la grammaire et l'orthographe, améliore la clarté et le \
                 flow des phrases, garde le sens et le ton d'origine. Ne réécris pas \
                 le texte de A à Z : travaille page par page, paragraphe par \
                 paragraphe. Réponds uniquement avec le texte amélioré, sans \
                 commentary."
            }
            AiTask::Shorten => {
                "Tu es un assistant de rédaction. Raccourcis le texte fourni en \
                 conservant l'essentiel : les idées clés, les chiffres, les noms \
                 propres. Supprime les répétitions, les phrases superflues et les \
                 tournures lourdes. Réponds uniquement avec le texte réduit, sans \
                 commentary."
            }
            AiTask::Expand => {
                "Tu es un assistant de rédaction. Développe le texte fourni pour le \
                 rendre plus explicatif et pédagogique : ajoute des explications, des \
                 exemples concrets et des transitions naturelles entre les idées. \
                 Garde le sens d'origine et le ton. Réponds uniquement avec le texte \
                 développé, sans commentary."
            }
            AiTask::Proofread => {
                "Tu es un relecteur attentif. Relève le texte fourni : fautes \
                 d'orthographe, de grammaire, de conjugaison, coquilles, incohérences \
                 de style, ponctuation incorrecte. Présente le résultat sous forme de \
                 liste numérotée par page, avec la citation de la faute et sa \
                 correction proposée. Réponds dans la langue du texte."
            }
            AiTask::Tone { style } => {
                match style {
                    ToneStyle::Professional => {
                        "Tu es un rédacteur professionnel. Réécris le texte fourni \
                         dans un ton formel, adapté à un document business ou officiel : \
                         phrases complètes, vocabulaire précis, pas de familiarités. \
                         Garde le fond inchangé. Réponds uniquement avec le texte \
                         réécrit, sans commentary."
                    }
                    ToneStyle::Casual => {
                        "Tu es un rédacteur décontracté. Réécris le texte fourni dans \
                         un ton casual, proche de la conversation : phrases courtes, \
                         vocabulaire courant, pas de jargon inutile. Garde le fond \
                         inchangé. Réponds uniquement avec le texte réécrit, sans \
                         commentary."
                    }
                    ToneStyle::Confident => {
                        "Tu es un rédacteur assuré. Réécris le texte fourni dans un ton \
                         confiant, sans hésitation : affirmations nettes, pas de \
                         tournures hésitantes, pas de conditionnels inutiles. Garde le \
                         fond inchangé. Réponds uniquement avec le texte réécrit, sans \
                         commentary."
                    }
                    ToneStyle::Friendly => {
                        "Tu es un rédacteur chaleureux. Réécris le texte fourni dans un \
                         ton friendly et amical : phrases accueillantes, vocabulaire \
                         positif, pas de ton sec. Garde le fond inchangé. Réponds \
                         uniquement avec le texte réécrit, sans commentary."
                    }
                }
            }
        }
    }

    /// Construit la liste complète des messages pour cette tâche.
    pub fn build_messages(&self, context: &ChatContext, user_input: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::system(self.system_prompt()));

        // Message d'introduction avec le contexte du document.
        if let Some(text) = &context.document_text {
            let header = match (&context.document_name, context.page_count) {
                (Some(name), Some(pages)) => {
                    format!("Document : « {name} » ({pages} page(s)).\n\nContenu :\n---\n")
                }
                (Some(name), None) => {
                    format!("Document : « {name} ».\n\nContenu :\n---\n")
                }
                (None, _) => "Contenu du document :\n---\n".to_string(),
            };

            let footer = "\n---\nFin du contenu du document.";

            messages.push(Message::user(format!("{header}{text}{footer}")));
            // Confirmation côté assistant : le modèle dispose du contexte.
            messages.push(Message::assistant(
                "J'ai bien pris connaissance du contenu du document.".to_string(),
            ));
        }

        // Question effective de l'utilisateur.
        match self {
            AiTask::Translate => {
                let target = context.target_language.as_deref().unwrap_or("anglais");
                messages.push(Message::user(format!(
                    "Traduis ce document vers la langue suivante : {target}."
                )));
            }
            AiTask::Summarize => {
                messages.push(Message::user("Résume ce document."));
            }
            AiTask::KeyPoints => {
                messages.push(Message::user("Extrais les points clés de ce document."));
            }
            AiTask::Polish => {
                messages.push(Message::user("Améliore le texte fourni."));
            }
            AiTask::Shorten => {
                messages.push(Message::user("Raccourcis le texte fourni."));
            }
            AiTask::Expand => {
                messages.push(Message::user("Développe le texte fourni."));
            }
            AiTask::Proofread => {
                messages.push(Message::user("Relève le texte fourni."));
            }
            AiTask::Tone { .. } => {
                messages.push(Message::user("Réécris le texte fourni dans le ton demandé."));
            }
            _ => {
                if !user_input.trim().is_empty() {
                    messages.push(Message::user(user_input));
                }
            }
        }

        messages
    }
}
