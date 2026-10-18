//! Écran d'accueil : fichiers récents, favoris et raccourcis d'outils.
//!
//! L'écran occupe le canevas quand aucun document n'est ouvert, ou quand la
//! pastille « maison » de la barre de titre est active. Le dessin lui-même
//! vit dans `app.rs` (il lui faut l'état privé de `YouPdfApp`) ; ce module ne
//! porte que l'état de l'écran et les aides pures, testables.

use std::path::{Path, PathBuf};

/// Onglet sélectionné dans l'écran d'accueil.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HomeCategory {
    /// Fichiers récemment ouverts.
    #[default]
    Recents,
    /// Fichiers marqués d'une étoile.
    Starred,
    /// Raccourcis vers les outils.
    Tools,
}

impl HomeCategory {
    /// Tous les onglets, dans l'ordre d'affichage.
    pub const ALL: [HomeCategory; 3] = [Self::Recents, Self::Starred, Self::Tools];

    /// Libellé affiché.
    pub fn label(self) -> &'static str {
        match self {
            Self::Recents => "Recents",
            Self::Starred => "Starred",
            Self::Tools => "Tools",
        }
    }
}

/// Présentation de la liste des récents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Cartes avec vignette.
    #[default]
    Grid,
    /// Table compacte.
    List,
}

impl ViewMode {
    /// Libellé de l'info-bulle.
    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => "Grid view",
            Self::List => "List view",
        }
    }

    /// Bascule grille ⇄ liste.
    pub fn toggled(self) -> ViewMode {
        match self {
            Self::Grid => Self::List,
            Self::List => Self::Grid,
        }
    }
}

/// État persistant de l'écran d'accueil.
#[derive(Default)]
pub struct HomeView {
    /// Filtre de recherche sur le nom de fichier.
    pub search_filter: String,
    /// Onglet courant.
    pub category: HomeCategory,
    /// Grille ou liste.
    pub view_mode: ViewMode,
}

impl HomeView {
    /// Vrai si `name` passe le filtre de recherche (insensible à la casse).
    ///
    /// Un filtre vide accepte tout.
    pub fn matches(&self, name: &str) -> bool {
        let needle = self.search_filter.trim().to_lowercase();
        needle.is_empty() || name.to_lowercase().contains(&needle)
    }
}

/// Ancienneté lisible d'un horodatage Unix face à « maintenant ».
///
/// Les unités restent grossières (minutes, heures, jours, mois, années) : un
/// écran d'accueil n'a pas besoin de la seconde près.
pub fn relative_time(then_secs: i64, now_secs: i64) -> String {
    let delta = (now_secs - then_secs).max(0);
    let plural = |n: i64, unit: &str| {
        if n == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{n} {unit}s ago")
        }
    };
    if delta < 60 {
        "Just now".to_owned()
    } else if delta < 60 * 60 {
        plural(delta / 60, "minute")
    } else if delta < 24 * 60 * 60 {
        plural(delta / (60 * 60), "hour")
    } else if delta < 30 * 24 * 60 * 60 {
        plural(delta / (24 * 60 * 60), "day")
    } else if delta < 365 * 24 * 60 * 60 {
        plural(delta / (30 * 24 * 60 * 60), "month")
    } else {
        plural(delta / (365 * 24 * 60 * 60), "year")
    }
}

/// Fichier des favoris (`~/.youpdf/starred.txt`, un chemin par ligne).
fn stars_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".youpdf").join("starred.txt"))
}

/// Lit la liste des favoris ; liste vide si le fichier manque.
pub fn load_stars() -> Vec<PathBuf> {
    let Some(path) = stars_file() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| PathBuf::from(line.trim()))
        .collect()
}

/// Écrit la liste des favoris, au mieux : un échec d'écriture est ignoré.
pub fn save_stars(stars: &[PathBuf]) {
    let Some(path) = stars_file() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let body: String = stars
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(path, body);
}

/// Vrai si `path` est marqué d'une étoile.
pub fn is_star(stars: &[PathBuf], path: &Path) -> bool {
    stars.iter().any(|star| star == path)
}

/// Ajoute ou retire `path` des favoris. Renvoie le nouvel état (vrai = favori).
pub fn toggle_star(stars: &mut Vec<PathBuf>, path: &Path) -> bool {
    if let Some(pos) = stars.iter().position(|star| star == path) {
        stars.remove(pos);
        false
    } else {
        stars.push(path.to_path_buf());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_filter_matches_everything() {
        let view = HomeView::default();
        assert!(view.matches("anything.pdf"));
    }

    #[test]
    fn filter_is_case_insensitive() {
        let view = HomeView {
            search_filter: "INVOICE".to_owned(),
            ..Default::default()
        };
        assert!(view.matches("invoice-2024.pdf"));
        assert!(view.matches("My_Invoice.pdf"));
        assert!(!view.matches("report.pdf"));
    }

    #[test]
    fn relative_time_buckets() {
        assert_eq!(relative_time(1_000, 1_030), "Just now");
        assert_eq!(relative_time(1_000, 1_120), "2 minutes ago");
        assert_eq!(relative_time(0, 60 * 60), "1 hour ago");
        assert_eq!(relative_time(0, 3 * 24 * 60 * 60), "3 days ago");
        assert_eq!(relative_time(0, 400 * 24 * 60 * 60), "1 year ago");
        // Un horodatage futur ne doit pas produire de valeur négative.
        assert_eq!(relative_time(9_999, 1_000), "Just now");
    }

    #[test]
    fn toggle_star_flips_membership() {
        let mut stars = Vec::new();
        let path = PathBuf::from("/tmp/a.pdf");
        assert!(toggle_star(&mut stars, &path));
        assert!(is_star(&stars, &path));
        assert!(!toggle_star(&mut stars, &path));
        assert!(!is_star(&stars, &path));
    }
}
