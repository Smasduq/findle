use clap::Parser;
use eframe::egui;
use findle::{load_config, type_label, DispatchResult, RunResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

#[derive(Parser)]
#[command(name = "findle-gui", about = "Findle desktop GUI (static tile grid)")]
struct GuiArgs {
    /// Directory to scan for files
    #[arg(long, default_value = ".")]
    dir: PathBuf,
}

/// Per-tile UI state.
#[derive(Clone)]
enum TileState {
    Idle,
    Running,
    Done(RunResult),
    NoHandler,
    Error(String),
}

struct App {
    dir: PathBuf,
    files: Vec<PathBuf>,
    states: HashMap<PathBuf, TileState>,
    tx: Sender<(PathBuf, Result<DispatchResult, String>)>,
    rx: Receiver<(PathBuf, Result<DispatchResult, String>)>,
    config: Arc<findle::Config>,
    ctx: egui::Context,
}

impl App {
    fn scan(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    files.push(p);
                }
            }
        }
        files.sort();
        files
    }

    /// Spawn a background thread to run the file's handler so the UI never blocks.
    fn run_tile(&mut self, path: &Path) {
        self.states.insert(path.to_path_buf(), TileState::Running);

        let config = self.config.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let path = path.to_path_buf();

        thread::spawn(move || {
            let runner = findle::Runner::new(&config);
            let outcome = runner.dispatch(&path).map_err(|e| e.to_string());
            let _ = tx.send((path, outcome));
            ctx.request_repaint();
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ctx = ctx.clone();

        // Drain any finished runs and update tile state.
        while let Ok((path, outcome)) = self.rx.try_recv() {
            let next = match outcome {
                Ok(DispatchResult::NoHandler) => TileState::NoHandler,
                Ok(DispatchResult::Output(out)) => TileState::Done(out),
                Err(e) => TileState::Error(e),
            };
            self.states.insert(path, next);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Findle");
            ui.label(format!("Directory: {}", self.dir.display()));
            ui.separator();

            let files = self.files.clone();
            let cols = 4usize;

            for chunk in files.chunks(cols) {
                ui.columns(cols, |columns| {
                    for (i, file) in chunk.iter().enumerate() {
                        let col = &mut columns[i];

                        let name = file
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.display().to_string());
                        let label = type_label(file);
                        let state = self
                            .states
                            .get(file)
                            .cloned()
                            .unwrap_or(TileState::Idle);

                        egui::Frame::group(&col.style()).show(col, |ui| {
                            ui.label(egui::RichText::new(&name).strong());
                            ui.label(format!("type: {}", label));

                            match state {
                                TileState::Idle => {
                                    if ui.button("Run").clicked() {
                                        self.run_tile(file);
                                    }
                                }
                                TileState::Running => {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label("running...");
                                    });
                                }
                                TileState::Done(out) => {
                                    ui.label(format!("exit code: {}", out.exit_code));
                                    egui::ScrollArea::vertical()
                                        .max_height(120.0)
                                        .show(ui, |ui| {
                                            if !out.stdout.is_empty() {
                                                ui.label(out.stdout.clone());
                                            }
                                            if !out.stderr.is_empty() {
                                                ui.label(
                                                    egui::RichText::new(out.stderr.clone())
                                                        .color(egui::Color32::from_rgb(220, 80, 80)),
                                                );
                                            }
                                        });
                                    if ui.button("Run again").clicked() {
                                        self.run_tile(file);
                                    }
                                }
                                TileState::NoHandler => {
                                    ui.label(
                                        egui::RichText::new("No handler")
                                            .color(egui::Color32::from_rgb(220, 160, 60)),
                                    );
                                    if ui.button("Run").clicked() {
                                        self.run_tile(file);
                                    }
                                }
                                TileState::Error(e) => {
                                    ui.label(
                                        egui::RichText::new(format!("Error: {}", e))
                                            .color(egui::Color32::from_rgb(220, 80, 80)),
                                    );
                                }
                            }
                        });
                    }
                });
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let args = GuiArgs::parse();

    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {}", e);
            std::process::exit(1);
        }
    };
    let config = Arc::new(config);

    let files = App::scan(&args.dir);
    let (tx, rx) = channel();

    let app = App {
        dir: args.dir,
        files,
        states: HashMap::new(),
        tx,
        rx,
        config,
        ctx: egui::Context::default(),
    };

    let options = eframe::NativeOptions::default();
    eframe::run_native("Findle", options, Box::new(|_cc| Box::new(app)))
}
