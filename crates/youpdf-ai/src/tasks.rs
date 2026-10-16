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
            _ => {
                if !user_input.trim().is_empty() {
                    messages.push(Message::user(user_input));
                }
            }
        }

        messages
    }
}
