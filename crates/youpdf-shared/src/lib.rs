//! # youpdf-shared
//!
//! Types fondamentaux partagés par tous les crates YouPDF.
//!
//! Ce crate **ne dépend d'aucune logique métier** : uniquement des
//! structures de données, des erreurs et des jetons de thème. Il est
//! volontairement léger pour pouvoir être utilisé partout sans créer
//! de dépendance circulaire.

pub mod error;
pub mod i18n;
pub mod theme;

pub use error::{Error, Result};
pub use i18n::{t, Language};
pub use theme::{
    caption, modal_frame, muted, pal, section, set_palette, Palette, Theme, BODY, CAPTION,
    LABEL, MONO, TITLE, RADIUS_L, RADIUS_M, RADIUS_S, RADIUS_TINY, RADIUS_XS, RADIUS_XL,
    SPACE_L, SPACE_M, SPACE_S, SPACE_TINY, SPACE_XS,
};

/// Écrit `bytes` dans `path` de façon atomique.
///
/// Un fichier temporaire du même dossier, `sync_all`, puis `rename` : une
/// panne pendant l'écriture laisse l'ancien fichier intact au lieu d'un
/// document tronqué. Le suffixe unique évite à la fois les collisions et les
/// liens symboliques préparés à l'avance.
pub fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = path.with_file_name(format!(
        ".{name}.youpdf-{}-{nanos}.tmp",
        std::process::id()
    ));

    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::atomic_write;

    #[test]
    fn atomic_write_remplace_le_fichier_sans_residu() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("youpdf-atomic-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.txt");

        atomic_write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}