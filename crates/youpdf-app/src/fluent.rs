//! Icônes 3D Microsoft Fluent Emoji (MIT), chargées comme textures egui.
//!
//! Phosphor reste la police des petites icônes fonctionnelles ; ces images ne
//! servent qu'aux endroits « vivants » (dock, bouton d'ouverture, outils).

use egui::{ColorImage, TextureHandle, TextureId, TextureOptions};

/// Jeu d'icônes 3D chargé une fois au démarrage.
pub struct FluentIcons {
    folder: Option<TextureHandle>,
    gear: Option<TextureHandle>,
    magic: Option<TextureHandle>,
}

/// Quelle icône 3D dessiner.
#[derive(Clone, Copy)]
pub enum Icon {
    Folder,
    Gear,
    Magic,
}

impl FluentIcons {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            folder: load(ctx, "fluent-folder", include_bytes!("assets/fluent/file_folder.png")),
            gear: load(ctx, "fluent-gear", include_bytes!("assets/fluent/gear.png")),
            magic: load(ctx, "fluent-magic", include_bytes!("assets/fluent/magic_wand.png")),
        }
    }

    pub fn id(&self, icon: Icon) -> Option<TextureId> {
        let handle = match icon {
            Icon::Folder => &self.folder,
            Icon::Gear => &self.gear,
            Icon::Magic => &self.magic,
        };
        handle.as_ref().map(|h| h.id())
    }
}

fn load(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let data = eframe::icon_data::from_png_bytes(bytes).ok()?;
    let image = ColorImage::from_rgba_unmultiplied(
        [data.width as usize, data.height as usize],
        &data.rgba,
    );
    Some(ctx.load_texture(name, image, TextureOptions::LINEAR))
}
