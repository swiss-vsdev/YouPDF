//! Format d'export des commentaires et initiales d'auteur.



/// Format d'export de la liste de commentaires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentFormat {
    /// Markdown lisible (rapport).
    Markdown,
    /// CSV `page,type,author,date,state,reply_to,content`.
    Csv,
    /// XFDF, le format d'échange de commentaires d'Acrobat.
    Xfdf,
    /// FDF, conteneur PDF de commentaires.
    Fdf,
}

impl CommentFormat {
    /// Nom lisible pour l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Csv => "CSV",
            Self::Xfdf => "XFDF",
            Self::Fdf => "FDF",
        }
    }

    /// Extension de fichier.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Csv => "csv",
            Self::Xfdf => "xfdf",
            Self::Fdf => "fdf",
        }
    }

    /// Les quatre formats, dans l'ordre du menu.
    pub const ALL: [Self; 4] = [Self::Csv, Self::Markdown, Self::Xfdf, Self::Fdf];
}

/// Initiale d'auteur, pour l'avatar de la carte.
///
/// Première lettre en majuscule, ou `?` si le champ est vide.
/// Initiale d'auteur, pour l'avatar de la carte.
pub fn author_initial(author: &str) -> String {
    author
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned())
}

#[cfg(test)]
mod author_tests {
    use super::author_initial;

/// L'avatar reprend la première lettre de l'auteur, en capitale.
#[test]
fn author_initial_is_uppercased() {
    assert_eq!(author_initial("alice"), "A");
    assert_eq!(author_initial("Bob"), "B");
    assert_eq!(author_initial(""), "?");
}
}
