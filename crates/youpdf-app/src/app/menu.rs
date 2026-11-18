//! Native macOS menu bar via objc2.
//!
//! Construit la barre des 9 menus (YouPDF · File · Edit · View · Tools · Go · Tab · Window · Help)
//! avec `objc2-app-kit`, calquée sur la structure de UPDF 2.5.6 (voir
//! `applicationDidFinishLaunching`.
//!
//! Tous les items d'action utilisent un **unique** sélecteur `menuAction:` sur le
//! délégué ; la commande est encodée dans `tag`. Le délégué pousse le tag dans
//! un `static Mutex<Vec<i64>>` que `YouPdfApp::update` vide à chaque trame pour
//! aiguiller vers les gestionnaires `MenuAction` existants (toolbar_ui.rs).
//! Les items standards (Hide, Quit, window min/max, …) ciblent `nil` et se
//! résolvent via la chaîne de répondants, sans passer par nous.

#![cfg(target_os = "macos")]

use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{declare_class, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu, NSMenuItem,
    NSMenuItemValidation, NSEventModifierFlags,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSString,
};

/// Touches d'équivalent-clavier.
const MOD_CMD: NSEventModifierFlags = NSEventModifierFlags(1 << 20); // Command
const MOD_CMD_SHIFT: NSEventModifierFlags = NSEventModifierFlags((1 << 20) | (1 << 17));
const MOD_CMD_OPT: NSEventModifierFlags = NSEventModifierFlags((1 << 20) | (1 << 19));
const MOD_NONE: NSEventModifierFlags = NSEventModifierFlags(0);

/// Pile de commandes en attente d'être consommées par `YouPdfApp::update`.
/// Remplie par `menuAction:` (fil principal), vidée par la boucle egui.
static PENDING: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// État courant de la touche « Show Cover Page in Two Page View » (tag 120).
/// `validateMenuItem:` lit ce fil principal : le délégué n'a pas accès à
/// `YouPdfApp`, et la validation est demandée par AppKit à chaque ouverture
/// du menu. `YouPdfApp::update` le met à jour une fois par trame.
static COVER_STATE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Indique à `validateMenuItem:` l'état courant de la touche de couverture.
pub fn set_cover_state(state: bool) {
    let _ = COVER_STATE.set(state);
}

/// État courant de la touche de couverture (par défaut éteint).
fn cover_state() -> bool {
    *COVER_STATE.get().unwrap_or(&false)
}

/// Mode de la barre d'outils (View ▸ Toolbar) : 0 = Keep, 1 = Scroll,
/// 2 = Hidden. Même motif que `COVER_STATE` : le délégué n'a pas accès à
/// `YouPdfApp`, `update` pousse l'état une fois par trame.
static TOOLBAR_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_toolbar_mode(mode: u8) {
    TOOLBAR_MODE.store(mode, std::sync::atomic::Ordering::Relaxed);
}

fn toolbar_mode() -> u8 {
    TOOLBAR_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Retire toutes les commandes en attente.
pub fn take_pending() -> Vec<isize> {
    let mut q = PENDING.lock().unwrap();
    std::mem::take(&mut *q)
}

use super::MenuAction;

/// Mappe un code de tag (encodé dans les items de menu) vers l'action
/// `MenuAction` correspondante, ou `None` pour les commandes traitées à part
/// (navigation, zoom, presse-papiers) dans `YouPdfApp::update`.
pub fn code_to_action(code: isize) -> Option<MenuAction> {
    match code {
        1 => Some(MenuAction::Preferences),
        16 => Some(MenuAction::Save),
        17 => Some(MenuAction::SaveAs),
        18 => Some(MenuAction::Export),
        19 => Some(MenuAction::Reveal),
        20 => Some(MenuAction::Properties),
        15 => Some(MenuAction::Close),
        70 => Some(MenuAction::Flatten),
        71 => Some(MenuAction::Watermark),
        72 => Some(MenuAction::HeaderFooter),
        73 => Some(MenuAction::PageNumbers),
        74 => Some(MenuAction::Compress),
        75 => Some(MenuAction::Protect),
        76 => Some(MenuAction::RemoveProtection),
        77 => Some(MenuAction::Crop),
        78 => Some(MenuAction::Background),
        79 => Some(MenuAction::Bates),
        80 => Some(MenuAction::Compare),
        81 => Some(MenuAction::Ocr),
        82 => Some(MenuAction::Create),
        83 => Some(MenuAction::Batch),
        84 => Some(MenuAction::Extract),
        85 => Some(MenuAction::Combine),
        121 => Some(MenuAction::ToolbarKeep),
        122 => Some(MenuAction::ToolbarScroll),
        123 => Some(MenuAction::ToolbarHidden),
        124 => Some(MenuAction::ToolbarShow),
        _ => None,
    }
}

declare_class!(
    pub struct YouPdfAppDelegate;

    // SAFETY:
    // - La superclasse NSObject n'a pas d'exigence de sous-classement.
    // - Mutabilité limitée au fil principal : c'est un délégué d'app.
    // - `YouPdfAppDelegate` n'implémente pas `Drop`.
    unsafe impl ClassType for YouPdfAppDelegate {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "YouPdfAppDelegate";
    }

    impl DeclaredClass for YouPdfAppDelegate {
        type Ivars = ();
    }

    unsafe impl NSObjectProtocol for YouPdfAppDelegate {}

    unsafe impl NSApplicationDelegate for YouPdfAppDelegate {
        #[method(applicationDidFinishLaunching:)]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::new().expect("must be on main thread");
            unsafe { setup_menu(mtm, self) };
        }

        #[method(applicationShouldTerminateAfterLastWindowClosed:)]
        fn should_terminate_after_last_window_closed(&self, _sender: &NSApplication) -> bool {
            false
        }
    }

    unsafe impl NSMenuItemValidation for YouPdfAppDelegate {
        #[method(validateMenuItem:)]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let tag = unsafe { item.tag() };
            // La touche « Show Cover Page in Two Page View » est un bouton
            // à bascule : son état est maintenu par `COVER_STATE`, mis à
            // jour une fois par trame par `YouPdfApp::update`. AppKit demande
            // la validation à chaque ouverture du menu, et le délégué n'a
            // pas accès à l'état de l'application.
            if tag == 120 {
                unsafe { item.setState(cover_state() as isize) };
            }
            // View ▸ Toolbar : boutons radio (Keep / Scroll / Hidden). Un seul
            // est coché, comme dans la référence.
            if (121..=123).contains(&tag) {
                let on = toolbar_mode() as isize == tag - 121;
                unsafe { item.setState(on as isize) };
            }
            true
        }
    }

    unsafe impl YouPdfAppDelegate {
        /// Sélecteur unique de tous les items d'action : le `tag` code la
        /// commande. On pousse le tag dans la file d'attente que la boucle
        /// egui vide à la trame suivante.
        #[method(menuAction:)]
        fn menu_action(&self, sender: &NSMenuItem) {
            let tag = unsafe { sender.tag() };
            if tag != 0 {
                PENDING.lock().unwrap().push(tag);
            }
        }
    }
);

impl YouPdfAppDelegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc();
        let this = this.set_ivars(());
        unsafe { msg_send_id![super(this), init] }
    }
}

// Retient le délégué pour la durée de l'application : un `NSMenu`/`NSMenuItem`
// garde une reference à son `target`, mais on doit s'assurer que l'objet Rust
// n'est pas libéré. Winit a son propre délégué d'application ; on n'y touche
// pas (ne pas appeler `setDelegate`), on installe seulement le menu principal.
// `MainThreadOnly` interdit `Sync` → `static` ; `thread_local` suffit car tout
// se passe sur le fil principal.
std::thread_local! {
    static DELEGATE: std::cell::OnceCell<Retained<YouPdfAppDelegate>> =
        const { std::cell::OnceCell::new() };
}

/// Construit et installe la barre des 9 menus comme menu principal, sans
/// remplacer le délégué de l'application (winit/eframe le gère). Le délégué
/// est retenu dans `DELEGATE` pour servir de cible aux actions du menu.
pub fn install_menu(mtm: MainThreadMarker) {
    // SAFETY: `install_menu` est appelé depuis `YouPdfApp::new`, sur le fil
    // principal, avant que la boucle d'événements ne démarre.
    DELEGATE.with(|c| {
        let delegate = c.get_or_init(|| YouPdfAppDelegate::new(mtm));
        unsafe { setup_menu(mtm, delegate) };
    });
}

/// Installe la barre des 9 menus comme menu principal de l'application.
unsafe fn setup_menu(mtm: MainThreadMarker, delegate: &YouPdfAppDelegate) {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let main_menu = NSMenu::new(mtm);
    let titles = [
        "YouPDF", "File", "Edit", "View", "Tools", "Go", "Tab", "Window", "Help",
    ];
    let submenus = [
        build_app_menu(mtm, delegate),
        build_file_menu(mtm, delegate),
        build_edit_menu(mtm, delegate),
        build_view_menu(mtm, delegate),
        build_tools_menu(mtm, delegate),
        build_go_menu(mtm, delegate),
        build_tab_menu(mtm, delegate),
        build_window_menu(mtm, delegate),
        build_help_menu(mtm, delegate),
    ];
    for (title, submenu) in titles.iter().zip(submenus) {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str(title),
                None,
                &NSString::from_str(""),
            )
        };
        item.setSubmenu(Some(&submenu));
        main_menu.addItem(&item);
    }

    app.setMainMenu(Some(&main_menu));
}

/// Ajoute un item d'action : target = délégué, action = `menuAction:`, la
/// commande est encodée dans `tag`. `key` est l'équivalent-clavier ("" = aucun).
/// Retourne l'item crée pour que les appels puissent le configurer (état,
/// image, …) après l'ajout.
fn add_item_item(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    title: &str,
    tag: isize,
    key: &str,
    modifier: NSEventModifierFlags,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(title),
            Some(sel!(menuAction:)),
            &NSString::from_str(key),
        )
    };
    unsafe { item.setTag(tag) };
    if !modifier.is_empty() {
        item.setKeyEquivalentModifierMask(modifier);
    }
    menu.addItem(&item);
    item
}

/// Ajoute un item d'action : target = délégué, action = `menuAction:`, la
/// commande est encodée dans `tag`. `key` est l'équivalent-clavier ("" = aucun).
fn add_item(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    title: &str,
    tag: isize,
    key: &str,
    modifier: NSEventModifierFlags,
) {
    add_item_item(mtm, menu, title, tag, key, modifier);
}

/// Ajoute un item standard : target = nil, résolu par la chaîne de répondants.
fn add_standard(mtm: MainThreadMarker, menu: &NSMenu, title: &str, sel: Sel, key: &str) {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(title),
            Some(sel),
            &NSString::from_str(key),
        )
    };
    // target nil → NSApplication / la chaîne de répondants gère.
    menu.addItem(&item);
}

fn add_separator(menu: &NSMenu) {
    let sep = NSMenuItem::separatorItem(MainThreadMarker::new().unwrap());
    menu.addItem(&sep);
}

/// Construit un sous-menu et l'ajoute comme item de `menu`.
fn build_app_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    // Items standard : target nil, résolus par NSApplication / répondants.
    add_standard(mtm, &menu, "About YouPDF", sel!(orderFrontStandardAboutPanel:), "");
    add_separator(&menu);
    // Settings… → nos préférences (tag 1).
    add_item(mtm, &menu, "Settings…", 1, ",", MOD_CMD);
    add_separator(&menu);
    add_standard(mtm, &menu, "Hide YouPDF", sel!(hide:), "h");
    add_standard(mtm, &menu, "Hide Others", sel!(hideOtherApplications:), "h");
    add_standard(mtm, &menu, "Show All", sel!(unhideAllApplications:), "");
    add_separator(&menu);
    add_standard(mtm, &menu, "Quit YouPDF", sel!(terminate:), "q");
    menu
}

fn build_file_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    add_item(mtm, &menu, "New Blank Document…", 10, "n", MOD_CMD);
    add_item(mtm, &menu, "From Images…", 11, "i", MOD_CMD);
    add_item(mtm, &menu, "From Office…", 12, "e", MOD_CMD);
    add_separator(&menu);
    add_item(mtm, &menu, "Open…", 13, "o", MOD_CMD);
    add_item(mtm, &menu, "Open Quickly…", 14, "f", MOD_CMD);
    add_separator(&menu);
    add_item(mtm, &menu, "Close Tab", 15, "w", MOD_CMD);
    add_separator(&menu);
    add_item(mtm, &menu, "Save…", 16, "s", MOD_CMD);
    add_item(mtm, &menu, "Save as…", 17, "S", MOD_CMD_SHIFT);
    add_separator(&menu);
    add_item(mtm, &menu, "Export…", 18, "e", MOD_CMD_OPT);
    add_separator(&menu);
    add_item(mtm, &menu, "Reveal in Finder", 19, "R", MOD_CMD_SHIFT);
    add_item(mtm, &menu, "Properties…", 20, "D", MOD_CMD_SHIFT);
    add_separator(&menu);
    add_item(mtm, &menu, "Print…", 21, "p", MOD_CMD);
    menu
}

fn build_edit_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    add_item(mtm, &menu, "Undo", 30, "z", MOD_CMD);
    add_item(mtm, &menu, "Redo", 31, "Z", MOD_CMD_SHIFT);
    add_separator(&menu);
    add_item(mtm, &menu, "Cut", 32, "x", MOD_CMD);
    add_item(mtm, &menu, "Copy", 33, "c", MOD_CMD);
    add_item(mtm, &menu, "Paste", 34, "v", MOD_CMD);
    add_item(mtm, &menu, "Delete", 35, "", MOD_NONE);
    add_item(mtm, &menu, "Select All", 36, "a", MOD_CMD);
    menu
}

fn build_view_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    add_item(mtm, &menu, "Zoom In", 40, "+", MOD_CMD);
    add_item(mtm, &menu, "Zoom Out", 41, "-", MOD_CMD);
    add_item(mtm, &menu, "Actual Size", 42, "0", MOD_CMD);
    add_item(mtm, &menu, "Fit Width", 43, "8", MOD_CMD);
    add_item(mtm, &menu, "Fit Page", 44, "9", MOD_CMD);
    add_separator(&menu);
    add_item(mtm, &menu, "Enter Full Screen", 45, "f", MOD_CMD_SHIFT);
    add_item(mtm, &menu, "Show/Hide Side Panel", 46, "", MOD_NONE);
    add_separator(&menu);
    // View ▸ Page Display ▸ Show Cover Page in Two Page View (UPDF 2.5.6).
    add_item(mtm, &menu, "Show Cover Page in Two Page View", 120, "", MOD_NONE);
    add_separator(&menu);
    // View ▸ Toolbar : radio (Keep / Scroll / Hidden) + Show, comme UPDF 2.5.6.
    add_item(mtm, &menu, "Keep Display", 121, "", MOD_NONE);
    add_item(mtm, &menu, "Scroll Pages Display", 122, "", MOD_NONE);
    add_item(mtm, &menu, "Keep Hidden", 123, "", MOD_NONE);
    add_separator(&menu);
    add_item(mtm, &menu, "Show/Hide Toolbar", 124, "t", MOD_CMD_OPT);
    menu
}

fn build_tools_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    add_item(mtm, &menu, "Flatten Annotations", 70, "", MOD_NONE);
    add_item(mtm, &menu, "Watermark…", 71, "", MOD_NONE);
    add_item(mtm, &menu, "Header & Footer…", 72, "", MOD_NONE);
    add_item(mtm, &menu, "Page Numbers…", 73, "", MOD_NONE);
    add_item(mtm, &menu, "Compress…", 74, "", MOD_NONE);
    add_item(mtm, &menu, "Protect Using Password…", 75, "", MOD_NONE);
    add_item(mtm, &menu, "Remove Protection…", 76, "", MOD_NONE);
    add_item(mtm, &menu, "Crop Pages…", 77, "", MOD_NONE);
    add_item(mtm, &menu, "Background…", 78, "", MOD_NONE);
    add_item(mtm, &menu, "Bates Numbering…", 79, "", MOD_NONE);
    add_item(mtm, &menu, "Compare PDFs…", 80, "", MOD_NONE);
    add_item(mtm, &menu, "OCR Text Recognition…", 81, "", MOD_NONE);
    add_item(mtm, &menu, "Create PDF…", 82, "", MOD_NONE);
    add_separator(&menu);
    add_item(mtm, &menu, "Batch…", 83, "", MOD_NONE);
    add_item(mtm, &menu, "Extract Data…", 84, "", MOD_NONE);
    add_item(mtm, &menu, "Combine Files…", 85, "", MOD_NONE);
    menu
}

fn build_go_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    add_item(mtm, &menu, "First Page", 100, "", MOD_NONE);
    add_item(mtm, &menu, "Previous Page", 101, "", MOD_NONE);
    add_item(mtm, &menu, "Next Page", 102, "", MOD_NONE);
    add_item(mtm, &menu, "Last Page", 103, "", MOD_NONE);
    add_separator(&menu);
    add_item(mtm, &menu, "Go to Page…", 104, "g", MOD_CMD);
    add_separator(&menu);
    add_item(mtm, &menu, "Back to Page", 105, "[", MOD_CMD);
    add_item(mtm, &menu, "Forward to Page", 106, "]", MOD_CMD);
    menu
}

fn build_tab_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    add_item(mtm, &menu, "Next Tab", 110, "}", MOD_CMD_SHIFT);
    add_item(mtm, &menu, "Previous Tab", 111, "{", MOD_CMD_SHIFT);
    menu
}

fn build_window_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    // Items standards : target nil → NSWindow gère via la chaîne de répondants.
    add_standard(mtm, &menu, "Minimize", sel!(performMiniaturize:), "m");
    add_standard(mtm, &menu, "Zoom", sel!(performZoom:), "");
    add_separator(&menu);
    add_standard(mtm, &menu, "Bring All to Front", sel!(arrangeInFront:), "");
    menu
}

fn build_help_menu(mtm: MainThreadMarker, _delegate: &YouPdfAppDelegate) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    add_item(mtm, &menu, "User Guide", 130, "", MOD_NONE);
    add_item(mtm, &menu, "Privacy Policy", 131, "", MOD_NONE);
    add_item(mtm, &menu, "End User License Agreement", 132, "", MOD_NONE);
    menu
}