use std::collections::HashMap;
use eframe::egui::{self, CentralPanel, Ui};
use std::time::{Duration, Instant};
use chrono::{DateTime, Duration as ChronoDuration, Local, Utc, Timelike};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::database::db_manager::{self as dbManager, StoredSession};

pub struct MyEguiApp {
    input_text: String,
    current_description: String,
    current_window_title: String,
    is_playing: bool,
    start_time: Option<Instant>,
    elapsed: Duration,
    elapsed_offset: Duration,
    current_session_start_time: Option<String>,
    db: SqlitePool,
    table_data: HashMap<String, HashMap<String, Vec<Duration>>>,
    pending_session_recovery: Option<StoredSession>,
    pending_afk_duration: Option<Duration>,
    show_recovery_dialog: bool,
    idle_return_rx: UnboundedReceiver<Duration>,
    // Time editing dialog state
    show_time_edit_dialog: bool,
    edited_start_hour: u32,
    edited_start_minute: u32,
    edit_error_message: Option<String>,
}

impl Default for MyEguiApp {
    fn default() -> Self {
        let (_idle_tx, idle_rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            input_text: String::new(),
            current_description: String::new(),
            current_window_title: "Bambana, seto!".to_owned(),
            is_playing: false,
            start_time: None,
            elapsed: Duration::ZERO,
            elapsed_offset: Duration::ZERO,
            current_session_start_time: None,
            db: SqlitePool::connect_lazy("sqlite::memory:").expect("dummy pool"),
            table_data: HashMap::new(),
            pending_session_recovery: None,
            pending_afk_duration: None,
            show_recovery_dialog: false,
            idle_return_rx: idle_rx,
            show_time_edit_dialog: false,
            edited_start_hour: 0,
            edited_start_minute: 0,
            edit_error_message: None,
        }
    }
}

impl MyEguiApp {
    pub fn with_db(
        db: SqlitePool,
        table_data: HashMap<String, HashMap<String, Vec<Duration>>>,
        pending_recovery: Option<StoredSession>,
        idle_return_rx: UnboundedReceiver<Duration>,
    ) -> Self {
        let show_dialog = pending_recovery.is_some();
        Self {
            input_text: String::new(),
            current_description: String::new(),
            current_window_title: "Bambana, seto!".to_owned(),
            is_playing: false,
            start_time: None,
            elapsed: Duration::ZERO,
            elapsed_offset: Duration::ZERO,
            current_session_start_time: None,
            db,
            table_data,
            pending_session_recovery: pending_recovery,
            pending_afk_duration: None,
            show_recovery_dialog: show_dialog,
            idle_return_rx,
            show_time_edit_dialog: false,
            edited_start_hour: 0,
            edited_start_minute: 0,
            edit_error_message: None,
        }
    }

    fn open_time_edit_dialog(&mut self) {
        // Pre-populate with current local time
        let now = Local::now();
        self.edited_start_hour = now.hour() as u32;
        self.edited_start_minute = now.minute() as u32;
        self.edit_error_message = None;
        self.show_time_edit_dialog = true;
    }

    fn apply_new_start_time(&mut self) {
        // Validate input
        if self.edited_start_hour > 23 {
            self.edit_error_message = Some("Ora deve essere tra 0 e 23".to_string());
            return;
        }
        if self.edited_start_minute > 59 {
            self.edit_error_message = Some("Minuti devono essere tra 0 e 59".to_string());
            return;
        }

        // Calculate new start_time as today at the specified hour:minute in Local time
        let now = Local::now();
        let new_start_local = now
            .date_naive()
            .and_hms_opt(self.edited_start_hour as u32, self.edited_start_minute as u32, 0)
            .expect("valid time");
        
        let new_start_utc = new_start_local
            .and_local_timezone(Local)
            .single()
            .expect("valid timezone")
            .with_timezone(&Utc);
        
        let new_start_time_str = new_start_utc.to_rfc3339();

        // Update current session state
        self.current_session_start_time = Some(new_start_time_str.clone());
        
        // Reset timer to recalculate elapsed from new start time
        self.start_time = Some(Instant::now());
        self.elapsed_offset = Duration::ZERO;
        self.elapsed = Duration::ZERO;

        // Update database asynchronously
        let pool = self.db.clone();
        tokio::spawn(async move {
            if let Err(err) = dbManager::update_open_session_start_time(&pool, &new_start_time_str).await {
                log::error!("Failed to update session start time: {err}");
            }
        });

        // Close dialog
        self.show_time_edit_dialog = false;
        self.edit_error_message = None;
    }

    fn begin_session(&mut self, description: String) {
        let start_time = Utc::now().to_rfc3339();

        self.is_playing = true;
        self.start_time = Some(Instant::now());
        self.elapsed_offset = Duration::ZERO;
        self.elapsed = Duration::ZERO;
        self.current_description = description.clone();
        self.current_session_start_time = Some(start_time.clone());
        self.input_text = description.clone();

        let pool = self.db.clone();
        tokio::spawn(async move {
            if let Err(err) = dbManager::insert_session(&pool, &description, &start_time).await {
                log::error!("Failed to insert session: {err}");
            }
        });
    }

    fn close_current_db_session_at(&self, description: Option<String>, end_time: String) {
        let pool = self.db.clone();
        tokio::spawn(async move {
            let result = if let Some(description) = description.as_deref() {
                dbManager::update_last_open_session_description_and_end(&pool, description, &end_time).await
            } else {
                dbManager::update_last_open_session_end(&pool, &end_time).await
            };

            if let Err(err) = result {
                log::error!("Failed to update session end: {err}");
            }
        });
    }

    fn stop_current_timer(&mut self) {
        self.is_playing = false;
        self.start_time = None;
        self.elapsed_offset = Duration::ZERO;
        self.current_session_start_time = None;
        self.current_description.clear();
        self.elapsed = Duration::ZERO;
    }

    fn update_window_title(&mut self, ctx: &egui::Context) {
        let desired_title = if self.is_playing && !self.current_description.is_empty() {
            self.current_description.clone()
        } else {
            "Bambana, seto!".to_owned()
        };

        if desired_title != self.current_window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(desired_title.clone()));
            self.current_window_title = desired_title;
        }
    }

    fn prompt_afk_recovery(&mut self, offline_duration: Duration) {
        if self.is_playing {
            if let Some(start) = self.start_time {
                self.elapsed = self.elapsed_offset + start.elapsed();
            }
            self.elapsed_offset = self.elapsed;
            self.is_playing = false;
            self.pending_afk_duration = Some(offline_duration);
            self.pending_session_recovery = Some(StoredSession {
                id: -1,
                description: self.current_description.clone(),
                start_time: self.current_session_start_time.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
                end_time: None,
            });
            self.show_recovery_dialog = true;
        }
    }

    fn top_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let text_response = ui.text_edit_singleline(&mut self.input_text);
            let button_text = if self.is_playing { "⏹" } else { "▶" };

            let enter_pressed = text_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if ui.button(button_text).clicked() || enter_pressed {
                if self.is_playing {
                    let end_time = Utc::now().to_rfc3339();
                    let description = self.input_text.trim();
                    let session_description = if description.is_empty() {
                        "(nessuna descrizione)".to_string()
                    } else {
                        description.to_string()
                    };

                    self.close_current_db_session_at(Some(session_description.clone()), end_time);

                    let date = Local::now().format("%Y-%m-%d").to_string();
                    self.table_data
                        .entry(date)
                        .or_default()
                        .entry(session_description.clone())
                        .or_default()
                        .push(self.elapsed);

                    self.stop_current_timer();
                } else {
                    let description = self.input_text.trim();
                    let description = if description.is_empty() {
                        "(nessuna descrizione)".to_string()
                    } else {
                        description.to_string()
                    };

                    self.begin_session(description);
                }
            }
            
            // Time display - clickable only when timer is active
            let time_label_response = ui.label(format!("Tempo: {}", format_duration(self.elapsed)));
            if self.is_playing && time_label_response.clicked() {
                self.open_time_edit_dialog();
            }
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
                            if self.is_playing {
                                let end_time = Utc::now().to_rfc3339();
                                self.close_current_db_session_at(None, end_time);

                                let date_str = Local::now().format("%Y-%m-%d").to_string();
                                self.table_data
                                    .entry(date_str)
                                    .or_default()
                                    .entry(self.current_description.clone())
                                    .or_default()
                                    .push(self.elapsed);

                                self.stop_current_timer();
                            }

                            self.input_text = desc.clone();
                            self.begin_session(desc.clone());
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
                if let Some(session) = self.pending_session_recovery.clone() {
                    ui.heading("È stata trovata una sessione interrotta!");
                    ui.label(format!("Descrizione: {}", session.description));
                    ui.label(format!("Avviata: {}", session.start_time));
                    if let Some(afk_duration) = self.pending_afk_duration {
                        ui.label(format!("Tempo AFK: {}", format_duration(afk_duration)));
                    }

                    ui.separator();
                    ui.label("Cosa desideri fare?");
                    ui.separator();

                    if ui.button("💾 Scarta tempo offline e continua").clicked() {
                        if let Some(afk_duration) = self.pending_afk_duration {
                            let idle_end = Utc::now();
                            let idle_start = idle_end
                                - ChronoDuration::from_std(afk_duration).unwrap_or_else(|_| ChronoDuration::zero());
                            self.close_current_db_session_at(None, idle_start.to_rfc3339());

                            self.begin_session(session.description.clone());
                        } else {
                            self.input_text = session.description.clone();
                            self.current_description = session.description.clone();
                            self.is_playing = true;
                            self.start_time = Some(Instant::now());
                            self.elapsed_offset = Duration::ZERO;
                            self.elapsed = Duration::ZERO;
                        }

                        self.show_recovery_dialog = false;
                        self.pending_afk_duration = None;
                        self.pending_session_recovery = None;
                    }

                    if ui.button("🔄 Scarta offline e nuova sessione").clicked() {
                        let end_time = Utc::now().to_rfc3339();
                        self.close_current_db_session_at(None, end_time);

                        self.input_text.clear();
                        self.current_description.clear();
                        self.is_playing = false;
                        self.start_time = None;
                        self.elapsed_offset = Duration::ZERO;
                        self.elapsed = Duration::ZERO;
                        self.current_session_start_time = None;
                        self.pending_afk_duration = None;
                        self.pending_session_recovery = None;
                        self.show_recovery_dialog = false;
                    }

                    if ui.button("🕒 Includi tempo offline e continua").clicked() {
                        if let Some(afk_duration) = self.pending_afk_duration {
                            self.input_text = session.description.clone();
                            self.current_description = session.description.clone();
                            self.is_playing = true;
                            self.start_time = Some(Instant::now());
                            self.elapsed_offset = self.elapsed + afk_duration;
                            self.elapsed = self.elapsed_offset;
                        } else if let Ok(start) = DateTime::parse_from_rfc3339(&session.start_time) {
                            let now = Utc::now();
                            let offline_duration = now
                                .signed_duration_since(start.with_timezone(&Utc))
                                .to_std()
                                .unwrap_or(Duration::ZERO);
                            self.input_text = session.description.clone();
                            self.current_description = session.description.clone();
                            self.is_playing = true;
                            self.start_time = Some(Instant::now());
                            self.elapsed_offset = offline_duration;
                            self.elapsed = offline_duration;
                        }

                        self.pending_afk_duration = None;
                        self.pending_session_recovery = None;
                        self.show_recovery_dialog = false;
                    }
                }
            });
    }

    fn show_time_edit_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_time_edit_dialog;
        egui::Window::new("Modifica Ora di Inizio")
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Inserisci l'ora e i minuti di inizio della sessione:");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Ora:");
                    let hour_str = self.edited_start_hour.to_string();
                    let mut hour_input = hour_str.clone();
                    if ui.text_edit_singleline(&mut hour_input).changed() {
                        if let Ok(h) = hour_input.trim().parse::<u32>() {
                            self.edited_start_hour = h;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Minuti:");
                    let min_str = self.edited_start_minute.to_string();
                    let mut min_input = min_str.clone();
                    if ui.text_edit_singleline(&mut min_input).changed() {
                        if let Ok(m) = min_input.trim().parse::<u32>() {
                            self.edited_start_minute = m;
                        }
                    }
                });

                if let Some(error) = &self.edit_error_message {
                    ui.colored_label(egui::Color32::RED, error);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Annulla").clicked() {
                        self.show_time_edit_dialog = false;
                        self.edit_error_message = None;
                    }

                    if ui.button("💾 Salva").clicked() {
                        self.apply_new_start_time();
                    }
                });
            });

        self.show_time_edit_dialog = is_open;
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.is_playing {
            if let Some(start) = self.start_time {
                self.elapsed = self.elapsed_offset + start.elapsed();
                ctx.request_repaint_after(std::time::Duration::from_millis(1000));
            }
        }

        while let Ok(offline_duration) = self.idle_return_rx.try_recv() {
            if self.is_playing && !self.show_recovery_dialog {
                self.prompt_afk_recovery(offline_duration);
            }
        }

        if self.show_recovery_dialog {
            self.show_recovery_popup(ctx);
        }

        if self.show_time_edit_dialog {
            self.show_time_edit_popup(ctx);
        }

        self.update_window_title(ctx);

        CentralPanel::default().show(ctx, |ui| {
            self.top_controls(ui);
            ui.separator();
            self.show_table(ui);
        });
    }
}

pub fn format_duration(d: Duration) -> String {
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

pub fn sessions_to_table_data(
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
