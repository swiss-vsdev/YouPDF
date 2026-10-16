//! Point d'entrée de l'application YouPDF.

mod app;

use app::YouPdfApp;

fn main() -> eframe::Result<()> {
    init_tracing();

    let initial = std::env::args().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1470.0, 923.0])
            .with_min_inner_size([960.0, 620.0])
            .with_title("YouPDF"),
        ..Default::default()
    };

    eframe::run_native(
        "YouPDF",
        options,
        Box::new(|cc| Ok(Box::new(YouPdfApp::new(cc, initial)))),
    )
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("youpdf=info,warn"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
