use std::collections::HashMap;
use eframe::egui::{self, CentralPanel, Ui};
use std::time::{Duration, Instant};
use chrono::{DateTime, Local, Utc};
use sqlx::SqlitePool;

use crono::database::dbmanager::{self as dbmanager, StoredSession};



// entry point
#[tokio::main]
async fn main() -> eframe::Result {
    let database_url = "sqlite:crono.db";
    let db = dbmanager::open_db(database_url)
        .await
        .map_err(|err| eframe::Error::AppCreation(Box::new(err)))?;

    let sessions = dbmanager::load_recent_sessions(&db)
        .await
        .unwrap_or_default();

    let pending_recovery = dbmanager::get_open_session(&db)
        .await
        .unwrap_or(None);

    let table_data = sessions_to_table_data(&sessions);
    let native_options = eframe::NativeOptions::default();
    let app = MyEguiApp::with_db(db, table_data, pending_recovery);

    eframe::run_native("Crono", native_options, Box::new(move |_cc| Ok(Box::new(app))))
}


struct MyEguiApp {
    input_text: String,
    current_description: String,
    is_playing: bool,
    start_time: Option<Instant>,
    elapsed: Duration,
    db: SqlitePool,
    table_data: HashMap<String, HashMap<String, Vec<Duration>>>, // data -> descrizione -> [tempi]
    pending_session_recovery: Option<StoredSession>,
    show_recovery_dialog: bool,
}

impl Default for MyEguiApp {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            current_description: String::new(),
            is_playing: false,
            start_time: None,
            elapsed: Duration::ZERO,
            db: SqlitePool::connect_lazy("sqlite::memory:").expect("dummy pool"),
            table_data: HashMap::new(),
            pending_session_recovery: None,
            show_recovery_dialog: false,
        }
    }
}

impl MyEguiApp {
    fn with_db(db: SqlitePool, table_data: HashMap<String, HashMap<String, Vec<Duration>>>, pending_recovery: Option<StoredSession>) -> Self {
        let show_dialog = pending_recovery.is_some();
        Self {
            input_text: String::new(),
            current_description: String::new(),
            is_playing: false,
            start_time: None,
            elapsed: Duration::ZERO,
            db,
            table_data,
            pending_session_recovery: pending_recovery,
            show_recovery_dialog: show_dialog,
        }
    }

    fn top_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let text_response = ui.text_edit_singleline(&mut self.input_text);
            let button_text = if self.is_playing { "⏹" } else { "▶" };
            
            // Controlla se è stato premuto Invio nel campo di testo
            let enter_pressed = text_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            
            if ui.button(button_text).clicked() || enter_pressed {
                if self.is_playing {
                    // Stop
                    self.is_playing = false;
                    self.start_time = None;

                    let end_time = Utc::now().to_rfc3339();
                    let pool = self.db.clone();
                    tokio::spawn(async move {
                        if let Err(err) = dbmanager::update_last_open_session_end(&pool, &end_time).await {
                            log::error!("Failed to update session end: {err}");
                        }
                    });

                    let date = Local::now().format("%Y-%m-%d").to_string();
                    self.table_data
                        .entry(date)
                        .or_default()
                        .entry(self.current_description.clone())
                        .or_default()
                        .push(self.elapsed);

                    self.elapsed = Duration::ZERO;
                    self.current_description.clear();
                } else {
                    // Play
                    let description = self.input_text.trim();
                    let description = if description.is_empty() {
                        "(nessuna descrizione)".to_string()
                    } else {
                        description.to_string()
                    };

                    self.is_playing = true;
                    self.start_time = Some(Instant::now());
                    self.current_description = description.clone();

                    let pool = self.db.clone();
                    let start_time = Utc::now().to_rfc3339();
                    tokio::spawn(async move {
                        if let Err(err) = dbmanager::insert_session(&pool, &description, &start_time).await {
                            log::error!("Failed to insert session: {err}");
                        }
                    });
                }
            }
            ui.label(format!("Tempo: {}", format_duration(self.elapsed)));
        });
    }

    fn show_table(&mut self, ui: &mut Ui) {
        for (date, tasks) in &self.table_data.clone() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Data: {}", date));
                    let total_time = calculate_total_time(tasks);
                    ui.label(format!("Totale: {}", total_time));
                });
                for (desc, durations) in tasks {
                    ui.horizontal(|ui| {
                        if ui.button("▶").clicked() {
                            // Stop sessione corrente se in corso
                            if self.is_playing {
                                self.is_playing = false;
                                self.start_time = None;
                                
                                let end_time = Utc::now().to_rfc3339();
                                let pool = self.db.clone();
                                tokio::spawn(async move {
                                    if let Err(err) = dbmanager::update_last_open_session_end(&pool, &end_time).await {
                                        log::error!("Failed to update session end: {err}");
                                    }
                                });

                                let date_str = Local::now().format("%Y-%m-%d").to_string();
                                self.table_data
                                    .entry(date_str)
                                    .or_default()
                                    .entry(self.current_description.clone())
                                    .or_default()
                                    .push(self.elapsed);

                                self.elapsed = Duration::ZERO;
                                self.current_description.clear();
                            }
                            
                            // Avvia nuova sessione con la descrizione cliccata
                            self.input_text = desc.clone();
                            self.is_playing = true;
                            self.start_time = Some(Instant::now());
                            self.current_description = desc.clone();

                            let pool = self.db.clone();
                            let start_time = Utc::now().to_rfc3339();
                            let desc_clone = desc.clone();
                            tokio::spawn(async move {
                                if let Err(err) = dbmanager::insert_session(&pool, &desc_clone, &start_time).await {
                                    log::error!("Failed to insert session: {err}");
                                }
                            });
                        }
                        ui.label(desc);
                        ui.label(format!("Totale: {}", format_duration(calculate_total_duration(durations))));
                    });
                    for duration in durations {
                        ui.horizontal(|ui| {
                            ui.label("   sessione:");
                            ui.label(format_duration(*duration));
                        });
                    }
                }
            });
            ui.separator();
        }
    }

    fn show_recovery_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_recovery_dialog;
        egui::Window::new("Sessione Interrotta")
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                if let Some(session) = &self.pending_session_recovery {
                    ui.heading("È stata trovata una sessione interrotta!");
                    ui.label(format!("Descrizione: {}", session.description));
                    ui.label(format!("Avviata: {}", session.start_time));
                    
                    ui.separator();
                    ui.label("Cosa desideri fare?");
                    ui.separator();

                    if ui.button("💾 Scarta tempo offline e continua").clicked() {
                        // Option 1: Discard offline time and continue
                        self.input_text = session.description.clone();
                        self.current_description = session.description.clone();
                        self.is_playing = true;
                        self.start_time = Some(Instant::now());
                        self.elapsed = Duration::ZERO;
                        self.show_recovery_dialog = false;
                    }

                    if ui.button("🔄 Scarta offline e nuova sessione").clicked() {
                        // Option 2: Close old session and start fresh
                        let pool = self.db.clone();
                        let session_id = session.id;
                        let end_time = Utc::now().to_rfc3339();
                        tokio::spawn(async move {
                            if let Err(err) = dbmanager::update_session_end_by_id(&pool, session_id, &end_time).await {
                                log::error!("Failed to close session: {err}");
                            }
                        });
                        
                        self.input_text.clear();
                        self.current_description.clear();
                        self.is_playing = false;
                        self.start_time = None;
                        self.elapsed = Duration::ZERO;
                        self.show_recovery_dialog = false;
                    }

                    if ui.button("⏱️ Includi tempo offline e continua").clicked() {
                        // Option 3: Include offline time and continue
                        if let Ok(start) = DateTime::parse_from_rfc3339(&session.start_time) {
                            let now = Utc::now();
                            let offline_duration = now.signed_duration_since(start.with_timezone(&Utc)).to_std().unwrap_or(Duration::ZERO);
                            
                            self.input_text = session.description.clone();
                            self.current_description = session.description.clone();
                            self.is_playing = true;
                            self.elapsed = offline_duration;
                            self.start_time = Some(Instant::now());
                            self.show_recovery_dialog = false;
                        }
                    }
                }
            });
        
        self.show_recovery_dialog = is_open;
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Aggiorna il tempo se il timer è attivo
        if self.is_playing {
            if let Some(start) = self.start_time {
                self.elapsed = start.elapsed();
                // ctx.request_repaint(); // forza il repaint continuo
                ctx.request_repaint_after(std::time::Duration::from_millis(1000)); // provare con un intervallo più lungo per ridurre il carico CPU
            }
        }

        // Mostra il popup di recovery se necessario
        if self.show_recovery_dialog {
            self.show_recovery_popup(ctx);
        }

        CentralPanel::default().show(ctx, |ui| {
            self.top_controls(ui);
            ui.separator();
            self.show_table(ui);
        });
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn calculate_total_duration(durations: &[Duration]) -> Duration {
    durations.iter().copied().sum()
}

fn calculate_total_time(tasks: &HashMap<String, Vec<Duration>>) -> String {
    let total: Duration = tasks.values().map(|durations| calculate_total_duration(durations)).sum();
    format_duration(total)
}

fn sessions_to_table_data(
    sessions: &[StoredSession],
) -> HashMap<String, HashMap<String, Vec<Duration>>> {
    let mut table_data: HashMap<String, HashMap<String, Vec<Duration>>> = HashMap::new();

    for session in sessions {
        if let Some(duration) = session_duration(session) {
            let start_time = DateTime::parse_from_rfc3339(&session.start_time)
                .map(|dt| dt.with_timezone(&Local));

            let start_time = match start_time {
                Ok(dt) => dt,
                Err(_) => continue,
            };

            let date = start_time.format("%Y-%m-%d").to_string();
            table_data
                .entry(date)
                .or_default()
                .entry(session.description.clone())
                .or_default()
                .push(duration);
        }
    }

    table_data
}

fn session_duration(session: &StoredSession) -> Option<Duration> {
    let end_time = session.end_time.as_ref()?;
    let start = DateTime::parse_from_rfc3339(&session.start_time).ok()?;
    let end = DateTime::parse_from_rfc3339(end_time).ok()?;
    end.signed_duration_since(start).to_std().ok()
}
