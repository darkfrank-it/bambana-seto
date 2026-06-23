use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Timelike, Utc};
use eframe::egui::{self, CentralPanel, Ui};
use egui::TextEdit;
use sqlx::SqlitePool;
use std::collections::{BTreeMap, HashMap};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::database::db_manager::{self as dbManager, StoredSession};

pub struct MyEguiApp {
    db: SqlitePool,
    current_window_title: String,

    table_data: BTreeMap<String, HashMap<String, Vec<Duration>>>,
    table_data_totals: HashMap<String, Duration>,
    pending_session_recovery: Option<StoredSession>,

    input_text: String,
    session_id: Option<i64>,
    is_playing: bool,
    start_time: Option<DateTime<Utc>>,
    elapsed: Duration,
    session_id_tx: UnboundedSender<i64>,
    session_id_rx: UnboundedReceiver<i64>,

    // Recovery dialog state
    show_recovery_dialog: bool,
    // Idle detection state
    show_idle_dialog: bool,
    pending_idle_duration: Option<Duration>,
    idle_return_rx: UnboundedReceiver<Duration>,
    // Start time editing dialog state
    show_start_time_edit_dialog: bool,
    edited_start_hour: u32,
    edited_start_minute: u32,
    // End time editing dialog state
    show_end_time_edit_dialog: bool,
    edited_end_date: String,
    edited_end_hour: u32,
    edited_end_minute: u32,
    edit_error_message: Option<String>,
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.is_playing {
            let start_time = self.start_time.expect("Expect running session!");
            self.elapsed = Utc::now() - start_time;
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        while let Ok(offline_duration) = self.idle_return_rx.try_recv() {
            if self.is_playing && !self.show_idle_dialog {
                self.prompt_idle_recovery(offline_duration);
            }
        }

        while let Ok(id) = self.session_id_rx.try_recv() {
            self.session_id = Some(id);
        }

        if self.show_recovery_dialog {
            self.show_recovery_popup(ctx);
        }

        if self.show_idle_dialog {
            self.show_idle_popup(ctx);
        }

        if self.show_start_time_edit_dialog {
            self.show_start_time_edit_popup(ctx);
        }

        if self.show_end_time_edit_dialog {
            self.show_end_time_edit_popup(ctx);
        }

        self.update_window_title(ctx);

        CentralPanel::default().show(ctx, |ui| {
            self.top_controls(ui);
            ui.separator();
            self.show_table(ui);
        });
    }
}

impl MyEguiApp {
    pub fn with_db(
        db: SqlitePool,
        sessions: &[StoredSession],
        idle_return_rx: UnboundedReceiver<Duration>,
        session_id_tx: UnboundedSender<i64>,
        session_id_rx: UnboundedReceiver<i64>,
    ) -> Self {
        Self {
            db,
            current_window_title: "Bambana, seto!".to_owned(),
            table_data: BTreeMap::new(),
            table_data_totals: HashMap::new(),
            pending_session_recovery: None,
            input_text: String::new(),
            session_id: None,
            is_playing: false,
            start_time: None,
            elapsed: Duration::zero(),
            show_recovery_dialog: false,
            show_idle_dialog: false,
            pending_idle_duration: None,
            idle_return_rx,
            session_id_tx,
            session_id_rx,
            show_start_time_edit_dialog: false,
            edited_start_hour: 0,
            edited_start_minute: 0,
            show_end_time_edit_dialog: false,
            edited_end_date: String::new(),
            edited_end_hour: 0,
            edited_end_minute: 0,
            edit_error_message: None,
        }
        .load_sessions(sessions)
    }

    // Carica le sessioni e aggiorna lo stato dell'app di conseguenza
    fn load_sessions(mut self, sessions: &[StoredSession]) -> Self {
        let (table_data, pending) = sessions_to_table_data(sessions);
        self.table_data = table_data;
        self.pending_session_recovery = pending;
        self.show_recovery_dialog = self.pending_session_recovery.is_some();
        // Calcola il totale delle sessioni per ogni giorno
        self.table_data_totals();

        self
    }

    // Allows closing a session at a specific end time
    fn close_current_db_session_at(&self, id: i64, end_time: i64) {
        let pool = self.db.clone();
        tokio::spawn(async move {
            let result = dbManager::end_open_session(&pool, id, end_time).await;

            if let Err(err) = result {
                log::error!("Failed to update session end: {err}");
            }
        });
    }

    // Allows updating the description of the current session
    fn update_current_db_session_at(&self, id: i64, description: String) {
        let pool = self.db.clone();
        tokio::spawn(async move {
            let result = dbManager::update_open_session(&pool, id, &description).await;

            if let Err(err) = result {
                log::error!("Failed to update session end: {err}");
            }
        });
    }

    fn delete_db_session(&self, id: i64) {
        let pool = self.db.clone();
        tokio::spawn(async move {
            let result = dbManager::delete_session(&pool, id).await;

            if let Err(err) = result {
                log::error!("Failed to delete session: {err}");
            }
        });
    }

    // Calcola il totale del tempo per ogni giorno e lo memorizza in `table_data_totals`
    fn table_data_totals(&mut self) {
        for (date, desc_map) in self.table_data.iter() {
            let date_total = desc_map
                .values()
                .flatten()
                .fold(Duration::zero(), |acc, d| acc + *d);
            self.table_data_totals.insert(date.clone(), date_total);
        }
    }

    fn calculate_total_time(&self) -> Duration {
        self.table_data_totals
            .values()
            .fold(Duration::zero(), |acc, d| acc + *d)
    }

    // Updates the window title based on current session state
    fn update_window_title(&mut self, ctx: &egui::Context) {
        let desired_title = if self.is_playing && !self.input_text.is_empty() {
            self.input_text.clone()
        } else {
            "Bambana, seto!".to_owned()
        };

        if desired_title != self.current_window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(desired_title.clone()));
            self.current_window_title = desired_title;
        }
    }

    // Opens a dialog to edit the start time of the current session
    fn open_time_edit_dialog(&mut self) {
        // Pre-populate with current local time
        let now = Utc::now();
        self.edited_start_hour = now.hour() as u32;
        self.edited_start_minute = now.minute() as u32;
        self.edit_error_message = None;
        self.show_start_time_edit_dialog = true;
    }

    fn open_end_time_edit_dialog(&mut self) {
        let now = Utc::now();
        self.edited_end_date = now.format("%Y-%m-%d").to_string();
        self.edited_end_hour = now.hour();
        self.edited_end_minute = now.minute();
        self.edit_error_message = None;
        self.show_end_time_edit_dialog = true;
    }

    // Applies the new start time entered by the user
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

        log::info!(
            "Editing start time to: {:02}:{:02}",
            self.edited_start_hour,
            self.edited_start_minute
        );

        // Calculate new start_time as today at the specified hour:minute in Local time
        let now = Utc::now();
        let new_start_local = now
            .date_naive()
            .and_hms_opt(
                self.edited_start_hour as u32,
                self.edited_start_minute as u32,
                0,
            )
            .expect("valid time");

        let new_start_utc = new_start_local.and_utc();

        self.start_time = Some(new_start_utc);
        self.elapsed = Utc::now().signed_duration_since(new_start_utc);

        let id = self.session_id.expect("No active session");

        // Update database asynchronously
        let pool = self.db.clone();
        tokio::spawn(async move {
            if let Err(err) =
                dbManager::update_open_session_start_time(&pool, id, new_start_utc.timestamp())
                    .await
            {
                log::error!("Failed to update session start time: {err}");
            }
        });

        // Close dialog
        self.show_start_time_edit_dialog = false;
        self.edit_error_message = None;
    }

    fn apply_new_end_time(&mut self) {
        // Validate input
        if self.edited_end_hour > 23 {
            self.edit_error_message = Some("Ora deve essere tra 0 e 23".to_string());
            return;
        }
        if self.edited_end_minute > 59 {
            self.edit_error_message = Some("Minuti devono essere tra 0 e 59".to_string());
            return;
        }

        let s = self.edited_end_date.clone()
            + " "
            + &format!("{:02}:{:02}", self.edited_end_hour, self.edited_end_minute);

        log::info!("Editing end time to: {}", s);

        // Calculate new end_time
        let naive = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M").ok();

        let new_end_utc = naive.expect("Expect end date!").and_utc();

        let id = self.session_id.expect("No active session");

        // Update database asynchronously
        let pool = self.db.clone();
        tokio::spawn(async move {
            if let Err(err) = dbManager::end_open_session(&pool, id, new_end_utc.timestamp()).await
            {
                log::error!("Failed to update session end time: {err}");
            }
        });

        self.elapsed = new_end_utc.signed_duration_since(self.start_time.unwrap());

        let date = self
            .start_time
            .expect("expected stat_time")
            .format("%Y-%m-%d")
            .to_string();
        self.table_data
            .entry(date)
            .or_default()
            .entry(self.input_text.clone())
            .or_default()
            .push(self.elapsed);
        // Calcola il totale delle sessioni per ogni giorno
        self.table_data_totals();

        self.session_id = None;
        self.start_time = None;
        self.elapsed = Duration::zero();
        self.input_text = "".to_string();

        // Close dialog
        self.show_end_time_edit_dialog = false;
        self.edit_error_message = None;
    }

    fn begin_session(&mut self) {
        self.is_playing = true;
        self.start_time = Some(Utc::now());
        self.elapsed = Duration::zero();

        let start_time = Utc::now().timestamp();
        let description = self.input_text.trim();
        let description = if description.is_empty() {
            "(nessuna descrizione)".to_string()
        } else {
            description.to_string()
        };

        let pool = self.db.clone();
        let tx = self.session_id_tx.clone();

        tokio::spawn(async move {
            match dbManager::insert_session(&pool, &description, start_time).await {
                Ok(id) => {
                    if let Err(err) = tx.send(id) {
                        log::error!("Failed to send inserted session id: {err}");
                    }
                }
                Err(err) => {
                    log::error!("Failed to insert session: {err}");
                }
            }
        });
    }

    fn update_session_description(&mut self) {
        let description = self.input_text.trim();
        let description = if description.is_empty() {
            "(nessuna descrizione)".to_string()
        } else {
            description.to_string()
        };
        self.update_current_db_session_at(
            self.session_id.expect("Sessione senza ID!"),
            description,
        );
    }

    fn end_session(&mut self) {
        self.is_playing = false;
        let session_end = Utc::now()
            - self
                .pending_idle_duration
                .unwrap_or_else(|| Duration::zero());
        let id = match self.session_id {
            Some(id) => id,
            None => {
                log::error!("Expect session open");
                return;
            }
        };
        self.close_current_db_session_at(id, session_end.timestamp());

        let date = Utc::now().format("%Y-%m-%d").to_string();
        self.table_data
            .entry(date)
            .or_default()
            .entry(self.input_text.clone())
            .or_default()
            .push(self.elapsed);
        // Calcola il totale delle sessioni per ogni giorno
        self.table_data_totals();

        self.session_id = None;
        self.start_time = None;
        self.elapsed = Duration::zero();
        self.input_text = "".to_string();
    }

    fn prompt_idle_recovery(&mut self, offline_duration: Duration) {
        self.pending_idle_duration = Some(offline_duration);
        self.show_idle_dialog = true;
    }

    // IDLE POPUP
    fn show_idle_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_idle_dialog;
        egui::Window::new("Sessione Inattiva")
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let start_time = self.start_time.expect("Expect running session!");
                let elapsed = Utc::now() - start_time;
                let current = Utc::now() - elapsed;

                let datetime: DateTime<Utc> = current.into();
                let formatted = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

                ui.heading("Sei stato inattivo per un po' di tempo!");
                ui.label(format!("Descrizione: {}", self.input_text));
                ui.label(format!("Avviata: {}", formatted));

                ui.label(format!(
                    "Tempo inattivo: {}",
                    format_duration(
                        self.pending_idle_duration
                            .unwrap_or_else(|| Duration::zero()),
                        DurationFormat::WithSeconds
                    )
                ));

                ui.separator();
                ui.label("Cosa desideri fare?");
                ui.separator();

                if ui.button("Mantieni il tempo e continua").clicked() {
                    self.pending_idle_duration = None;
                    self.show_idle_dialog = false;
                }

                if ui.button("Scarta tempo").clicked() {
                    self.end_session();

                    self.pending_idle_duration = None;
                    self.show_idle_dialog = false;
                }

                if ui.button("Scarta tempo e continua").clicked() {
                    self.end_session();

                    // nuova sessione
                    self.begin_session();

                    self.pending_idle_duration = None;
                    self.show_idle_dialog = false;
                }
            });
    }

    // RECOVERY POPUP
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

                    let start_time = Utc.timestamp_opt(session.start_time, 0).single();
                    let date = start_time.unwrap().format("%Y-%m-%d %H:%M").to_string();
                    ui.label(format!("Avviata: {}", date));

                    let afk_duration: Duration = Utc::now()
                        .signed_duration_since(Utc.timestamp_opt(session.start_time, 0).unwrap());
                    //let afk_duration
                    ui.label(format!(
                        "Tempo totale sessione: {}",
                        format_duration(afk_duration, DurationFormat::WithoutSeconds)
                    ));

                    ui.separator();
                    ui.label("Cosa desideri fare?");
                    ui.separator();

                    if ui.button("Mantieni il tempo e continua").clicked() {
                        self.input_text = session.description.clone();

                        self.is_playing = true;
                        self.session_id = Some(session.id);
                        self.start_time = DateTime::<Utc>::from_timestamp(session.start_time, 0);
                        self.elapsed = self.start_time.unwrap().signed_duration_since(Utc::now());

                        self.pending_session_recovery = None;
                        self.show_recovery_dialog = false;
                    }

                    if ui
                        .button("Termina sessione inserendo il tempo di fine")
                        .clicked()
                    {
                        self.pending_session_recovery = None;
                        self.show_recovery_dialog = false;

                        self.session_id = Some(session.id);
                        self.start_time = DateTime::<Utc>::from_timestamp(session.start_time, 0);
                        self.input_text = session.description.clone();
                        self.open_end_time_edit_dialog();
                    }

                    if ui.button("Scarta tempo").clicked() {
                        self.delete_db_session(session.id);

                        self.pending_session_recovery = None;
                        self.show_recovery_dialog = false;
                    }
                }
            });
    }

    // START TIME EDIT POPUP
    fn show_start_time_edit_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_start_time_edit_dialog;
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
                    let mut hour_input = self.edited_start_hour.to_string();
                    if ui.text_edit_singleline(&mut hour_input).changed() {
                        if let Ok(h) = hour_input.trim().parse::<u32>() {
                            self.edited_start_hour = h;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Minuti:");
                    let mut min_input = self.edited_start_minute.to_string();
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
                        self.show_start_time_edit_dialog = false;
                        self.edit_error_message = None;
                    }

                    if ui.button("💾 Salva").clicked() {
                        self.apply_new_start_time();
                    }
                });
            });
    }

    // END TIME EDIT POPUP
    fn show_end_time_edit_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_end_time_edit_dialog;
        egui::Window::new("Modifica Date e Ora di Fine")
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Inserisci la data e l'ora di fine della sessione:");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Data (YYYY-MM-DD):");
                    let mut date_input = self.edited_end_date.clone();
                    if ui.text_edit_singleline(&mut date_input).changed() {
                        self.edited_end_date = date_input.trim().to_string();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Ora:");
                    let mut hour_input = self.edited_end_hour.to_string();
                    if ui.text_edit_singleline(&mut hour_input).changed() {
                        if let Ok(h) = hour_input.trim().parse::<u32>() {
                            self.edited_end_hour = h;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Minuti:");
                    let mut min_input = self.edited_end_minute.to_string();
                    if ui.text_edit_singleline(&mut min_input).changed() {
                        if let Ok(m) = min_input.trim().parse::<u32>() {
                            self.edited_end_minute = m;
                        }
                    }
                });

                if let Some(error) = &self.edit_error_message {
                    ui.colored_label(egui::Color32::RED, error);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("💾 Salva").clicked() {
                        self.apply_new_end_time();
                    }
                });
            });
    }

    // TOP CONTROLS
    fn top_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let text_response = ui.add(
                TextEdit::singleline(&mut self.input_text).hint_text("A cosa stai lavorando?"),
            );

            let button_text = if self.is_playing { "⏹" } else { "▶" };

            let enter_pressed =
                text_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if ui.button(button_text).clicked() || enter_pressed {
                if self.is_playing {
                    if enter_pressed {
                        self.update_session_description();
                    } else {
                        self.end_session();
                    }
                } else {
                    self.begin_session();
                }
            }

            // Time display - clickable only when timer is active
            let time_label_response = ui.label(format!(
                "Tempo: {}",
                format_duration(self.elapsed, DurationFormat::WithSeconds)
            ));
            if self.is_playing && time_label_response.clicked() {
                self.open_time_edit_dialog();
            }
        });
    }

    // TABLE DISPLAY
    fn show_table(&mut self, ui: &mut Ui) {
        let entries: Vec<(String, HashMap<String, Vec<Duration>>)> = self
            .table_data
            .iter()
            .rev()
            .map(|(date, tasks)| (date.clone(), tasks.clone()))
            .collect();
        for (date, tasks) in entries {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("Data: {}", date));
                            let mut total_time = self.calculate_total_time();

                            // Add active session time if it's for today
                            if self.is_playing {
                                let today = Utc::now().format("%Y-%m-%d").to_string();
                                if date == today {
                                    let base_total: Duration = self
                                        .table_data_totals
                                        .get(&date)
                                        .cloned()
                                        .unwrap_or_else(|| Duration::zero());
                                    total_time = base_total + self.elapsed;
                                }
                            }

                            ui.label(format!(
                                "Totale: {}",
                                format_duration(total_time, DurationFormat::WithSeconds)
                            ));
                        });
                        for (desc, durations) in tasks {
                            ui.horizontal(|ui| {
                                if ui.button("▶").clicked() {
                                    if self.is_playing {
                                        self.end_session();
                                    }

                                    self.input_text = desc.clone();
                                    self.begin_session();
                                }
                                ui.label(desc);
                                let duration = self
                                    .table_data_totals
                                    .get(&date)
                                    .cloned()
                                    .unwrap_or_else(|| Duration::zero());
                                ui.label(format!(
                                    "Totale: {}",
                                    format_duration(duration, DurationFormat::WithSeconds)
                                ));
                            });
                            for duration in durations {
                                ui.horizontal(|ui| {
                                    ui.label("   sessione:");
                                    ui.label(format_duration(
                                        duration,
                                        DurationFormat::WithSeconds,
                                    ));
                                });
                            }
                        }
                    });
                    ui.separator();
                },
            );
        }
    }
}

// ALTRE FUNZIONI

// Trasforma le sessioni memorizzate in una struttura adatta per la visualizzazione nella tabella
fn sessions_to_table_data(
    sessions: &[StoredSession],
) -> (
    BTreeMap<String, HashMap<String, Vec<Duration>>>,
    Option<StoredSession>,
) {
    let mut table_data: BTreeMap<String, HashMap<String, Vec<Duration>>> = BTreeMap::new();
    let mut pending_recovery: Option<StoredSession> = None;

    for session in sessions {
        // Solo sessioni con end_time valorizzato (sessioni concluse)
        if let Some(duration) = session_duration(session) {
            // Converti timestamp → DateTime<Local>
            let start_time = match Utc.timestamp_opt(session.start_time, 0).single() {
                Some(dt) => dt,
                None => continue, // timestamp non valido
            };

            let date = start_time.format("%Y-%m-%d").to_string();

            table_data
                .entry(date)
                .or_default()
                .entry(session.description.clone())
                .or_default()
                .push(duration);
        } else {
            pending_recovery = Some(session.clone());
        }
    }

    (table_data, pending_recovery)
}

fn session_duration(session: &StoredSession) -> Option<Duration> {
    let end = session.end_time?;
    let start = session.start_time;

    if end < start {
        return None;
    }

    Some(Duration::seconds(end - start))
}

pub enum DurationFormat {
    WithSeconds,
    WithoutSeconds,
}

fn format_duration(d: Duration, fmt: DurationFormat) -> String {
    let secs = d.num_seconds();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;

    match fmt {
        DurationFormat::WithSeconds => {
            format!("{:02}:{:02}:{:02}", h, m, s)
        }
        DurationFormat::WithoutSeconds => {
            format!("{:02}:{:02}", h, m)
        }
    }
}
