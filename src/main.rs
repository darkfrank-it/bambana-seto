use std::io::{self, BufRead};
use std::ops::Deref;
use std::thread;
// use std::time::Duration;
use tokio::signal;
use tokio::{sync::broadcast, time};
use std::sync::{Arc, Mutex};
use std::fs::File;
use std::error::Error;
use std::collections::HashMap;
use eframe::egui::{self, CentralPanel, Context, Ui};
use std::time::{Duration, Instant};

use crono::capture;
// use crono::draw;
use crono::database;



// entry point
#[tokio::main]
async fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("Crono", native_options, Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))))
}


struct MyEguiApp {
    input_text: String,
    is_playing: bool,
    start_time: Option<Instant>,
    elapsed: Duration,
    table_data: HashMap<String, Vec<(String, String)>>, // data -> [(descrizione, tempo)]
}

impl Default for MyEguiApp {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            is_playing: false,
            start_time: None,
            elapsed: Duration::ZERO,
            table_data: HashMap::new(),
        }
    }
}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }

    fn top_controls(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.input_text);
            let button_text = if self.is_playing { "⏹" } else { "▶" };
            if ui.button(button_text).clicked() {
                if self.is_playing {
                    // Stop
                    self.is_playing = false;
                    self.start_time = None;

                    // Salva il tempo nella tabella
                    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let entry = (self.input_text.clone(), format_duration(self.elapsed));
                    self.table_data.entry(date).or_default().push(entry);

                    self.elapsed = Duration::ZERO;
                } else {
                    // Play
                    self.is_playing = true;
                    self.start_time = Some(Instant::now());
                }
            }
            ui.label(format!("Tempo: {}", format_duration(self.elapsed)));
        });
    }

    fn show_table(&self, ui: &mut Ui) {
        for (date, entries) in &self.table_data {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Data: {}", date));
                    let total_time = calculate_total_time(entries);
                    ui.label(format!("Totale: {}", total_time));
                });
                for (desc, time) in entries {
                    ui.horizontal(|ui| {
                        ui.label(desc);
                        ui.label(time);
                    });
                }
            });
            ui.separator();
        }
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Aggiorna il tempo se il timer è attivo
        if self.is_playing {
            if let Some(start) = self.start_time {
                self.elapsed = start.elapsed();
                // ctx.request_repaint(); // forza il repaint continuo
                ctx.request_repaint_after(std::time::Duration::from_millis(100)); // provare con un intervallo più lungo per ridurre il carico CPU
            }
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

fn calculate_total_time(entries: &[(String, String)]) -> String {
    let total_secs: u64 = entries.iter().filter_map(|(_, t)| {
        let parts: Vec<u64> = t.split(':').filter_map(|p| p.parse().ok()).collect();
        if parts.len() == 3 {
            Some(parts[0] * 3600 + parts[1] * 60 + parts[2])
        } else {
            None
        }
    }).sum();

    format_duration(Duration::from_secs(total_secs))
}
