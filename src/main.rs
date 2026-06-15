#![windows_subsystem = "windows"]

use tokio::sync::mpsc::unbounded_channel;

use eframe::egui;

use bambana_seto::capture::idle_sentinel as idleSentinel;
use bambana_seto::database::db_manager as dbManager;
use bambana_seto::ui::app::{MyEguiApp, sessions_to_table_data};

// entry point
#[tokio::main]
async fn main() -> eframe::Result {
    let database_url = "sqlite:.data/bambana.db";
    let db = dbManager::open_db(database_url)
        .await
        .map_err(|err| eframe::Error::AppCreation(Box::new(err)))?;

    let sessions = dbManager::load_recent_sessions(&db)
        .await
        .unwrap_or_default();

    let pending_recovery = dbManager::get_open_session(&db)
        .await
        .unwrap_or(None);

    let table_data = sessions_to_table_data(&sessions);
    let (idle_tx, idle_rx) = unbounded_channel();
    idleSentinel::start_idle_watcher(idle_tx);

    let icon_data = load_icon();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon_data),
        ..Default::default()
    };

    let app = MyEguiApp::with_db(db, table_data, pending_recovery, idle_rx);

    eframe::run_native("Bambana, seto!", native_options, Box::new(move |_cc| Ok(Box::new(app))))
}


fn load_icon() -> egui::IconData {
    // Load the image from bytes (recommended: include_bytes!)
    let image = image::load_from_memory(
        include_bytes!("..\\assets\\icon.png") // adjust path
    )
    .expect("Failed to load icon")
    .into_rgba8();

    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    egui::IconData {
        rgba,
        width,
        height,
    }
}
