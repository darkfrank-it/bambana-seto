use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Timelike, Utc};
use eframe::egui::{self, CentralPanel, Ui};
use egui::TextEdit;
use rust_i18n::t;
use sqlx::SqlitePool;
use std::collections::{BTreeMap, HashMap};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::database::db_manager::{self as dbManager, StoredSession};

use rust_i18n::i18n;
i18n!("locales");

// Mappa data -> (descrizione -> durate delle sessioni)
type TableData = BTreeMap<String, HashMap<String, Vec<Duration>>>;

pub struct MyEguiApp {
    db: SqlitePool,
    current_window_title: String,

    pixels_per_point: f32,

    table_data: TableData,
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
    resume_start_time: Option<DateTime<Utc>>,
    // Start time editing dialog state
    show_start_time_edit_dialog: bool,
    edited_start_hour: String,
    edited_start_minute: String,
    // End time editing dialog state
    show_end_time_edit_dialog: bool,
    edited_end_date: String,
    edited_end_hour: String,
    edited_end_minute: String,
    edit_error_message: Option<String>,
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_pixels_per_point(self.pixels_per_point);

        if self.is_playing {
            let start_time = self.start_time.expect("Expect running session!");
            self.elapsed = Utc::now() - start_time;
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        while let Ok(offline_duration) = self.idle_return_rx.try_recv() {
            if self.is_playing && !self.show_idle_dialog {
                self.prompt_idle_recovery(offline_duration, ctx);
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
        pixels_per_point: f32,
    ) -> Self {
        Self {
            db,
            current_window_title: t!("window_title").to_string(),
            table_data: BTreeMap::new(),
            table_data_totals: HashMap::new(),
            pending_session_recovery: None,
            input_text: String::new(),
            session_id: None,
            is_playing: false,
            start_time: None,
            elapsed: Duration::zero(),
            session_id_tx,
            session_id_rx,
            pixels_per_point,
            resume_start_time: None,
            show_recovery_dialog: false,
            show_idle_dialog: false,
            pending_idle_duration: None,
            idle_return_rx,
            show_start_time_edit_dialog: false,
            edited_start_hour: String::new(),
            edited_start_minute: String::new(),
            show_end_time_edit_dialog: false,
            edited_end_date: String::new(),
            edited_end_hour: String::new(),
            edited_end_minute: String::new(),
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
        if self.show_recovery_dialog {
            log::info!(
                "Pending session recovery found: {:?}",
                self.pending_session_recovery
            );
            self.resume_start_time = Some(Utc::now());
            // richiama l'attenzione della finestra principale quando il popup è aperto
            notify_rust::Notification::new()
                .summary(t!("recovery_title").as_ref())
                .body(t!("recovery_body").as_ref())
                .show()
                .unwrap();
        }
        // Calcola il totale delle sessioni per ogni giorno
        self.calc_table_data_totals();

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
    fn calc_table_data_totals(&mut self) {
        for (date, desc_map) in self.table_data.iter() {
            // totale per data
            let date_total = desc_map
                .values()
                .flatten()
                .fold(Duration::zero(), |acc, d| acc + *d);
            self.table_data_totals.insert(date.clone(), date_total);

            // totale per (date, desc)
            for (desc, durations) in desc_map.iter() {
                let total = durations.iter().fold(Duration::zero(), |acc, d| acc + *d);

                let key = format!("{}_{}", date, desc);
                self.table_data_totals.insert(key, total);
            }
        }
    }

    // Updates the window title based on current session state
    fn update_window_title(&mut self, ctx: &egui::Context) {
        let desired_title = if self.is_playing && !self.input_text.is_empty() {
            self.input_text.clone()
        } else {
            t!("window_title").to_string()
        };

        if desired_title != self.current_window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(desired_title.clone()));
            self.current_window_title = desired_title;
        }
    }

    // Resets the state of the timer
    fn reset_timer_state(&mut self) {
        self.input_text = "".to_string();
        self.session_id = None;
        self.is_playing = false;
        self.start_time = None;
        self.elapsed = Duration::zero();
    }

    // Opens a dialog to edit the start time of the current session
    fn open_time_edit_dialog(&mut self) {
        // Pre-populate with current local time
        let now = Utc::now();
        self.edited_start_hour = format!("{:02}", now.hour());
        self.edited_start_minute = format!("{:02}", now.minute());
        self.edit_error_message = None;
        self.show_start_time_edit_dialog = true;
    }

    fn open_end_time_edit_dialog(&mut self) {
        let now = Utc::now();
        self.edited_end_date = now.format("%Y-%m-%d").to_string();
        self.edited_end_hour = format!("{:02}", now.hour());
        self.edited_end_minute = format!("{:02}", now.minute());
        self.edit_error_message = None;
        self.show_end_time_edit_dialog = true;
    }

    // Applies the new start time entered by the user
    fn apply_new_start_time(&mut self) {
        // Validate input
        let hour: u32 = match self.edited_start_hour.trim().parse() {
            Ok(h) if h <= 23 => h,
            _ => {
                self.edit_error_message = Some(t!("invalid_hour").to_string());
                return;
            }
        };
        let minute: u32 = match self.edited_start_minute.trim().parse() {
            Ok(m) if m <= 59 => m,
            _ => {
                self.edit_error_message = Some(t!("invalid_minute").to_string());
                return;
            }
        };

        log::info!("Editing start time to: {:02}:{:02}", hour, minute);

        // Calculate new start_time as today at the specified hour:minute in Local time
        let now = Utc::now();
        let new_start_local = now
            .date_naive()
            .and_hms_opt(hour, minute, 0)
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
        let hour: u32 = match self.edited_end_hour.trim().parse() {
            Ok(h) if h <= 23 => h,
            _ => {
                self.edit_error_message = Some(t!("invalid_hour").to_string());
                return;
            }
        };
        let minute: u32 = match self.edited_end_minute.trim().parse() {
            Ok(m) if m <= 59 => m,
            _ => {
                self.edit_error_message = Some(t!("invalid_minute").to_string());
                return;
            }
        };

        let s = self.edited_end_date.clone() + " " + &format!("{:02}:{:02}", hour, minute);

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
        self.calc_table_data_totals();

        // reset timer
        self.reset_timer_state();

        // Close dialog
        self.show_end_time_edit_dialog = false;
        self.edit_error_message = None;
    }

    fn begin_session(&mut self) {
        self.is_playing = true;
        self.start_time = Some(Utc::now());
        self.elapsed = Duration::zero();

        let start_time = if let Some(resume_start_time) = self.resume_start_time {
            resume_start_time.timestamp()
        } else {
            Utc::now().timestamp()
        };
        self.resume_start_time = None;

        let description = self.input_text.trim();
        let description = if description.is_empty() {
            t!("no_description").to_string()
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
            t!("no_description").to_string()
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
                .unwrap_or_else(Duration::zero);
        log::info!(
            "Ending session at: {}",
            session_end.format("%Y-%m-%d %H:%M:%S")
        );
        let id = match self.session_id {
            Some(id) => id,
            None => {
                log::error!("Expect session open");
                return;
            }
        };
        self.close_current_db_session_at(id, session_end.timestamp());

        let date = Utc::now().format("%Y-%m-%d").to_string();
        let elapsed = self.elapsed
            - self
                .pending_idle_duration
                .unwrap_or_else(Duration::zero);
        log::info!(
            "Adding session to table_data: date={}, desc={}, elapsed={}",
            date,
            self.input_text,
            format_duration(elapsed, DurationFormat::WithSeconds)
        );
        self.table_data
            .entry(date)
            .or_default()
            .entry(self.input_text.clone())
            .or_default()
            .push(elapsed);
        // Calcola il totale delle sessioni per ogni giorno
        self.calc_table_data_totals();

        // reset timer
        self.reset_timer_state();
    }

    fn prompt_idle_recovery(&mut self, offline_duration: Duration, ctx: &egui::Context) {
        // richiama l'attenzione della finestra principale quando il popup è aperto
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        notify_rust::Notification::new()
            .summary(t!("idle_session_title").as_ref())
            .body(t!("idle_session_body").as_ref())
            .show()
            .unwrap();

        self.resume_start_time = Some(Utc::now());
        self.pending_idle_duration = Some(offline_duration);
        self.show_idle_dialog = true;
    }

    // IDLE POPUP
    fn show_idle_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_idle_dialog;
        egui::Window::new(t!("idle_session_title").to_string())
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let start_time = self.start_time.expect("Expect running session!");
                let elapsed = Utc::now() - start_time;
                let current = Utc::now() - elapsed;

                let datetime: DateTime<Utc> = current;
                let formatted = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

                ui.heading(t!("idle_session_body"));
                ui.label(format!("{}: {}", t!("running_task"), self.input_text));
                ui.label(format!("{}: {}", t!("started_label"), formatted));

                ui.label(format!(
                    "{}: {}",
                    t!("idle_time_label"),
                    format_duration(
                        self.pending_idle_duration
                            .unwrap_or_else(Duration::zero),
                        DurationFormat::WithSeconds
                    )
                ));

                ui.separator();
                ui.label(t!("what_do_you_want_to_do_with_idle_time"));
                ui.separator();

                if ui.button(t!("keep_time_continue")).clicked() {
                    self.pending_idle_duration = None;
                    self.resume_start_time = None;
                    self.show_idle_dialog = false;
                }

                if ui.button(t!("discard_time")).clicked() {
                    self.end_session();

                    self.pending_idle_duration = None;
                    self.resume_start_time = None;
                    self.show_idle_dialog = false;
                }

                if ui.button(t!("discard_time_continue")).clicked() {
                    let description = self.input_text.clone();

                    self.end_session();

                    self.input_text = description;
                    // nuova sessione
                    self.begin_session();

                    self.pending_idle_duration = None;
                    self.resume_start_time = None;
                    self.show_idle_dialog = false;
                }
            });
    }

    // RECOVERY POPUP
    fn show_recovery_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_recovery_dialog;
        egui::Window::new(t!("recovery_title").to_string())
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                if let Some(session) = self.pending_session_recovery.clone() {
                    ui.heading(t!("recovery_body").to_string());
                    ui.label(format!(
                        "{}: {}",
                        t!("running_task"),
                        session.description
                    ));

                    let start_time = Utc.timestamp_opt(session.start_time, 0).single();
                    let date = start_time.unwrap().format("%Y-%m-%d %H:%M").to_string();
                    ui.label(format!("{}: {}", t!("started_label"), date));

                    let afk_duration: Duration = Utc::now()
                        .signed_duration_since(Utc.timestamp_opt(session.start_time, 0).unwrap());
                    //let afk_duration
                    ui.label(format!(
                        "{}: {}",
                        t!("inactive_time_label"),
                        format_duration(afk_duration, DurationFormat::WithoutSeconds)
                    ));

                    ui.separator();
                    ui.label(t!("what_do_you_want_to_do_with_inactive_time"));
                    ui.separator();

                    if ui.button(t!("recovery_keep_continue")).clicked() {
                        self.input_text = session.description.clone();

                        self.is_playing = true;
                        self.session_id = Some(session.id);
                        self.start_time = DateTime::<Utc>::from_timestamp(session.start_time, 0);
                        self.elapsed = self.start_time.unwrap().signed_duration_since(Utc::now());

                        self.pending_session_recovery = None;
                        self.resume_start_time = None;
                        self.show_recovery_dialog = false;
                    }

                    if ui.button(t!("recovery_end_with_time")).clicked() {
                        self.pending_session_recovery = None;
                        self.resume_start_time = None;
                        self.show_recovery_dialog = false;

                        self.session_id = Some(session.id);
                        self.start_time = DateTime::<Utc>::from_timestamp(session.start_time, 0);
                        self.input_text = session.description.clone();
                        self.open_end_time_edit_dialog();
                    }

                    if ui.button(t!("recovery_discard")).clicked() {
                        self.delete_db_session(session.id);

                        self.resume_start_time = None;
                        self.pending_session_recovery = None;
                        self.show_recovery_dialog = false;
                    }
                }
            });
    }

    // START TIME EDIT POPUP
    fn show_start_time_edit_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_start_time_edit_dialog;
        let mut cancel_clicked = false;
        egui::Window::new(t!("edit_start_time_title"))
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(t!("enter_start_time"));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(t!("hour_label"));
                    ui.text_edit_singleline(&mut self.edited_start_hour);
                });

                ui.horizontal(|ui| {
                    ui.label(t!("minute_label"));
                    ui.text_edit_singleline(&mut self.edited_start_minute);
                });

                if let Some(error) = &self.edit_error_message {
                    ui.colored_label(egui::Color32::RED, error);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(t!("cancel")).clicked() {
                        cancel_clicked = true;
                    }

                    if ui.button(t!("save")).clicked() {
                        self.apply_new_start_time();
                    }
                });
            });

        if !is_open || cancel_clicked {
            self.show_start_time_edit_dialog = false;
            self.edit_error_message = None;
        }
    }

    // END TIME EDIT POPUP
    fn show_end_time_edit_popup(&mut self, ctx: &egui::Context) {
        let mut is_open = self.show_end_time_edit_dialog;
        egui::Window::new(t!("edit_end_time_title"))
            .resizable(false)
            .collapsible(false)
            .open(&mut is_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(t!("enter_end_time"));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(t!("date_label"));
                    let mut date_input = self.edited_end_date.clone();
                    if ui.text_edit_singleline(&mut date_input).changed() {
                        self.edited_end_date = date_input.trim().to_string();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(t!("hour_label"));
                    ui.text_edit_singleline(&mut self.edited_end_hour);
                });

                ui.horizontal(|ui| {
                    ui.label(t!("minute_label"));
                    ui.text_edit_singleline(&mut self.edited_end_minute);
                });

                if let Some(error) = &self.edit_error_message {
                    ui.colored_label(egui::Color32::RED, error);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(t!("save")).clicked() {
                        self.apply_new_end_time();
                    }
                });
            });

        if !is_open {
            self.show_end_time_edit_dialog = false;
            self.edit_error_message = None;
        }
    }

    // TOP CONTROLS
    fn top_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let text_response = ui.add(
                TextEdit::singleline(&mut self.input_text).hint_text(t!("what_are_you_working_on")),
            );

            let button_text = if self.is_playing { "⏹" } else { "▶" };
            let button_color = if self.is_playing {
                egui::Color32::from_rgb(255, 90, 90)
            } else {
                egui::Color32::from_rgb(90, 170, 255)
            };
            let button = egui::Button::new(button_text)
                .fill(button_color)
                .stroke(egui::Stroke::new(1.0_f32, button_color));

            let enter_pressed =
                text_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if ui.add(button).clicked() || enter_pressed {
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
                "{}: {}",
                t!("elapsed_time"),
                format_duration(self.elapsed, DurationFormat::WithSeconds)
            ));
            if self.is_playing && time_label_response.clicked() {
                self.open_time_edit_dialog();
            }
        });
    }

    // TABLE DISPLAY
    fn show_table(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                let entries: Vec<(String, HashMap<String, Vec<Duration>>)> = self
                    .table_data
                    .iter()
                    .rev()
                    .map(|(date, tasks)| (date.clone(), tasks.clone()))
                    .collect();
                let today = Utc::now().format("%Y-%m-%d").to_string();

                for (date, tasks) in entries {
                    let mut total_time: Duration = self
                        .table_data_totals
                        .get(&date)
                        .cloned()
                        .unwrap_or_else(Duration::zero);
                    // Add active session time if it's for today
                    if self.is_playing
                        && date == today {
                            total_time += self.elapsed;
                        }
                    let total_time_label = format!(
                        "{}  {}              {}:  {}",
                        t!("date_label"),
                        date,
                        t!("total_time"),
                        format_duration(total_time, DurationFormat::WithSeconds)
                    );
                    let open = date == today;

                    egui::CollapsingHeader::new(total_time_label)
                        .id_salt(&date) // importante se cambia label
                        .default_open(open)
                        .show(ui, |ui| {
                            ui.group(|ui| {
                                egui::Grid::new(format!("tasks_grid_{}", date))
                                    .striped(true) // righe alternate
                                    .spacing([16.0, 6.0]) // spazio tra colonne/righe
                                    .show(ui, |ui| {
                                        // 🔹 Header tabella
                                        ui.label("");
                                        ui.label(t!("task_label"));
                                        ui.label(t!("session_label"));
                                        ui.label(t!("total_time"));
                                        ui.end_row();

                                        for (desc, durations) in tasks {
                                            let key = format!("{}_{}", date, desc);
                                            let total_duration = self
                                                .table_data_totals
                                                .get(&key)
                                                .cloned()
                                                .unwrap_or_else(Duration::zero);

                                            // 🔹 Riga principale (task)
                                            ui.horizontal(|ui| {
                                                if ui.button("▶").clicked() {
                                                    if self.is_playing {
                                                        self.end_session();
                                                    }

                                                    self.input_text = desc.clone();
                                                    self.begin_session();
                                                }
                                            });

                                            ui.label(&desc);

                                            ui.label(format_duration(
                                                total_duration,
                                                DurationFormat::WithSeconds,
                                            ));

                                            ui.label(""); // vuoto per allineare colonna session
                                            ui.end_row();

                                            // 🔹 Righe delle sessioni
                                            for duration in durations {
                                                ui.label(""); // niente play
                                                ui.label(t!("session_label")); // indent visivo

                                                ui.label(""); // niente totale

                                                ui.label(format_duration(
                                                    duration,
                                                    DurationFormat::WithSeconds,
                                                ));

                                                ui.end_row();
                                            }
                                        }
                                    });
                            });
                        });
                }
            });
    }
}

// ALTRE FUNZIONI

// Trasforma le sessioni memorizzate in una struttura adatta per la visualizzazione nella tabella
fn sessions_to_table_data(sessions: &[StoredSession]) -> (TableData, Option<StoredSession>) {
    let mut table_data: TableData = BTreeMap::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::sync::mpsc;

    // ---- session_duration ----

    #[test]
    fn session_duration_computes_seconds_between_start_and_end() {
        let session = StoredSession {
            id: 1,
            description: "task".into(),
            start_time: 1_000,
            end_time: Some(1_090),
        };
        assert_eq!(session_duration(&session), Some(Duration::seconds(90)));
    }

    #[test]
    fn session_duration_is_none_when_still_open() {
        let session = StoredSession {
            id: 1,
            description: "task".into(),
            start_time: 1_000,
            end_time: None,
        };
        assert_eq!(session_duration(&session), None);
    }

    #[test]
    fn session_duration_is_none_when_end_before_start() {
        let session = StoredSession {
            id: 1,
            description: "task".into(),
            start_time: 1_000,
            end_time: Some(500),
        };
        assert_eq!(session_duration(&session), None);
    }

    #[test]
    fn session_duration_allows_zero_length_session() {
        let session = StoredSession {
            id: 1,
            description: "task".into(),
            start_time: 1_000,
            end_time: Some(1_000),
        };
        assert_eq!(session_duration(&session), Some(Duration::zero()));
    }

    // ---- format_duration ----

    #[test]
    fn format_duration_with_seconds() {
        let d = Duration::seconds(3_661); // 1h 1m 1s
        assert_eq!(
            format_duration(d, DurationFormat::WithSeconds),
            "01:01:01"
        );
    }

    #[test]
    fn format_duration_without_seconds() {
        let d = Duration::seconds(3_661);
        assert_eq!(format_duration(d, DurationFormat::WithoutSeconds), "01:01");
    }

    #[test]
    fn format_duration_zero() {
        assert_eq!(
            format_duration(Duration::zero(), DurationFormat::WithSeconds),
            "00:00:00"
        );
    }

    #[test]
    fn format_duration_over_24_hours_does_not_wrap() {
        let d = Duration::seconds(25 * 3600); // 25h
        assert_eq!(
            format_duration(d, DurationFormat::WithSeconds),
            "25:00:00"
        );
    }

    // ---- sessions_to_table_data ----

    #[test]
    fn sessions_to_table_data_groups_by_date_and_description() {
        let sessions = vec![
            StoredSession {
                id: 1,
                description: "task a".into(),
                start_time: 1_700_000_000,
                end_time: Some(1_700_000_000 + 60),
            },
            StoredSession {
                id: 2,
                description: "task a".into(),
                start_time: 1_700_000_200,
                end_time: Some(1_700_000_200 + 120),
            },
        ];

        let (table_data, pending) = sessions_to_table_data(&sessions);

        assert!(pending.is_none());
        let date = Utc
            .timestamp_opt(1_700_000_000, 0)
            .single()
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();
        let durations = table_data.get(&date).unwrap().get("task a").unwrap();
        assert_eq!(
            durations,
            &vec![Duration::seconds(60), Duration::seconds(120)]
        );
    }

    #[test]
    fn sessions_to_table_data_reports_open_session_as_pending_recovery() {
        let sessions = vec![StoredSession {
            id: 1,
            description: "task a".into(),
            start_time: 1_700_000_000,
            end_time: None,
        }];

        let (table_data, pending) = sessions_to_table_data(&sessions);

        assert!(table_data.is_empty());
        assert_eq!(pending.unwrap().id, 1);
    }

    #[test]
    fn sessions_to_table_data_skips_sessions_with_invalid_start_time() {
        let sessions = vec![StoredSession {
            id: 1,
            description: "task a".into(),
            start_time: i64::MAX,
            end_time: Some(i64::MAX),
        }];

        let (table_data, pending) = sessions_to_table_data(&sessions);

        assert!(table_data.is_empty());
        assert!(pending.is_none());
    }

    // ---- start/end time edit validation ----

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    // Wraps a MyEguiApp backed by a temp db file and removes the file once
    // the test is done, so repeated test runs don't leave junk in the temp dir.
    struct TestApp {
        app: MyEguiApp,
        path: std::path::PathBuf,
    }

    impl std::ops::Deref for TestApp {
        type Target = MyEguiApp;
        fn deref(&self) -> &MyEguiApp {
            &self.app
        }
    }

    impl std::ops::DerefMut for TestApp {
        fn deref_mut(&mut self) -> &mut MyEguiApp {
            &mut self.app
        }
    }

    impl TestApp {
        // SQLite on Windows can't delete a file while a connection still has
        // it open, so the pool must be closed before removing the file.
        async fn close(self) {
            self.app.db.close().await;
            let _ = std::fs::remove_file(&self.path);
        }
    }

    async fn test_app() -> TestApp {
        let id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "bambana_seto_app_test_{}_{}.db",
            std::process::id(),
            id
        ));
        let url = format!("sqlite:{}", path.display());
        let db = dbManager::open_db(&url).await.expect("open test db");

        let (_idle_tx, idle_rx) = mpsc::unbounded_channel();
        let (session_id_tx, session_id_rx) = mpsc::unbounded_channel();

        let app = MyEguiApp::with_db(db, &[], idle_rx, session_id_tx, session_id_rx, 1.0);
        TestApp { app, path }
    }

    #[tokio::test]
    async fn apply_new_start_time_rejects_invalid_hour() {
        let mut app = test_app().await;
        app.show_start_time_edit_dialog = true;
        app.edited_start_hour = "24".into();
        app.edited_start_minute = "00".into();

        app.apply_new_start_time();

        assert!(app.edit_error_message.is_some());
        // On validation failure the dialog must stay open.
        assert!(app.show_start_time_edit_dialog);
        app.close().await;
    }

    #[tokio::test]
    async fn apply_new_start_time_rejects_non_numeric_minute() {
        let mut app = test_app().await;
        app.edited_start_hour = "10".into();
        app.edited_start_minute = "abc".into();

        app.apply_new_start_time();

        assert!(app.edit_error_message.is_some());
        app.close().await;
    }

    #[tokio::test]
    async fn apply_new_start_time_accepts_valid_input() {
        let mut app = test_app().await;
        app.session_id = Some(1);
        app.edited_start_hour = "09".into();
        app.edited_start_minute = "30".into();

        app.apply_new_start_time();

        assert!(app.edit_error_message.is_none());
        assert!(!app.show_start_time_edit_dialog);
        let start = app.start_time.expect("start_time should be set");
        assert_eq!(start.hour(), 9);
        assert_eq!(start.minute(), 30);
        app.close().await;
    }

    #[tokio::test]
    async fn apply_new_end_time_rejects_invalid_minute() {
        let mut app = test_app().await;
        app.edited_end_hour = "10".into();
        app.edited_end_minute = "60".into();

        app.apply_new_end_time();

        assert!(app.edit_error_message.is_some());
        app.close().await;
    }
}
