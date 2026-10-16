//! Registre des fournisseurs disponibles.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::provider::AiProvider;

/// Identifiant unique d'un fournisseur.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Ensemble des fournisseurs disponibles à l'exécution.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<ProviderId, Arc<dyn AiProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let id = provider.id().clone();
        tracing::debug!(provider = %id, "fournisseur IA enregistré");
        self.providers.insert(id, provider);
    }

    pub fn unregister(&mut self, id: &ProviderId) -> Option<Arc<dyn AiProvider>> {
        self.providers.remove(id)
    }

    pub fn get(&self, id: &ProviderId) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn all(&self) -> Vec<Arc<dyn AiProvider>> {
        let mut v: Vec<_> = self.providers.values().cloned().collect();
        v.sort_by(|a, b| a.id().as_str().cmp(b.id().as_str()));
        v
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}
