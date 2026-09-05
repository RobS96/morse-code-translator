//! Cross-platform (Windows/macOS/Linux) desktop GUI for the Morse code
//! translator. Wraps `morse-core` with a simple, friendly interface:
//! type text or Morse, see the translation instantly, and hit "Transmit"
//! to watch a flashing lamp and hear a tone play out the real timing —
//! optionally with Farnsworth timing (fast characters, slower spacing),
//! the internationally recommended way to learn Morse.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use eframe::egui;
use morse_core::{Timing, build_signal_plan, decode, encode, wpm_to_unit_ms};
use rodio::source::{SineWave, Source};
use rodio::{DeviceSinkBuilder, Player};

/// Common procedural signs, offered as one-click inserts in Encode mode.
/// (name, human-readable meaning)
const PROSIGNS: &[(&str, &str)] = &[
    ("AR", "end of message"),
    ("SK", "end of contact"),
    ("BT", "new paragraph"),
    ("KN", "over to you, specifically"),
    ("AS", "wait"),
    ("CT", "start copying"),
];

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([520.0, 620.0])
            .with_min_inner_size([420.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Morse Code Translator",
        options,
        Box::new(|_cc| Ok(Box::<MorseApp>::default())),
    )
}

#[derive(PartialEq)]
enum Mode {
    Encode,
    Decode,
}

struct MorseApp {
    input: String,
    output: String,
    mode: Mode,
    char_wpm: f64,
    farnsworth_enabled: bool,
    effective_wpm: f64,
    /// Shared with the background transmit thread: true while a dot/dash
    /// tone+flash is actively "on".
    is_lit: Arc<AtomicBool>,
    /// Shared with the background transmit thread: true for the whole
    /// duration of a transmission, used to disable the button + keep
    /// the UI repainting.
    is_transmitting: Arc<AtomicBool>,
    status: String,
}

impl Default for MorseApp {
    fn default() -> Self {
        Self {
            input: "SOS".to_string(),
            output: encode("SOS"),
            mode: Mode::Encode,
            char_wpm: 20.0,
            farnsworth_enabled: false,
            effective_wpm: 5.0,
            is_lit: Arc::new(AtomicBool::new(false)),
            is_transmitting: Arc::new(AtomicBool::new(false)),
            status: String::new(),
        }
    }
}

impl MorseApp {
    fn recompute(&mut self) {
        self.output = match self.mode {
            Mode::Encode => encode(&self.input),
            Mode::Decode => decode(&self.input),
        };
    }

    fn timing(&self) -> Timing {
        if self.farnsworth_enabled {
            Timing::farnsworth_wpm(self.char_wpm, self.effective_wpm)
        } else {
            Timing::uniform(wpm_to_unit_ms(self.char_wpm))
        }
    }

    fn insert_prosign(&mut self, name: &str) {
        if !self.input.is_empty() && !self.input.ends_with(' ') {
            self.input.push(' ');
        }
        self.input.push_str(&format!("<{name}>"));
        self.recompute();
    }

    fn spawn_transmission(&self) {
        // Transmission always plays the *text* form, so decode first if
        // the user has Morse loaded on the Decode tab.
        let text = match self.mode {
            Mode::Encode => self.input.clone(),
            Mode::Decode => decode(&self.input),
        };
        let timing = self.timing();
        let is_lit = self.is_lit.clone();
        let is_transmitting = self.is_transmitting.clone();

        is_transmitting.store(true, Ordering::SeqCst);

        thread::spawn(move || {
            // One audio device sink per transmission; kept alive for the
            // thread's lifetime so tones don't get cut off.
            let device_sink = DeviceSinkBuilder::open_default_sink().ok();
            let player = device_sink.as_ref().map(|d| Player::connect_new(d.mixer()));

            for signal in build_signal_plan(&text) {
                if signal.is_tone() {
                    let dur = Duration::from_millis(signal.duration_ms_timed(timing));
                    is_lit.store(true, Ordering::SeqCst);
                    if let Some(player) = &player {
                        let tone = SineWave::new(600.0).take_duration(dur).amplify(0.20);
                        player.append(tone);
                    }
                    thread::sleep(dur);
                    is_lit.store(false, Ordering::SeqCst);
                    // 1-unit gap after every symbol, at character speed.
                    thread::sleep(Duration::from_millis(timing.char_unit_ms));
                } else {
                    thread::sleep(Duration::from_millis(signal.duration_ms_timed(timing)));
                }
            }
            if let Some(player) = &player {
                player.sleep_until_end();
            }
            is_transmitting.store(false, Ordering::SeqCst);
        });
    }
}

impl eframe::App for MorseApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let transmitting = self.is_transmitting.load(Ordering::SeqCst);
        if transmitting {
            ui.ctx().request_repaint(); // keep animating the lamp
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            ui.heading("Morse Code Translator");
            ui.add_space(10.0);

            // ---- Mode switch ---------------------------------------------------
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.mode == Mode::Encode, "📝  Text → Morse")
                    .clicked()
                {
                    self.mode = Mode::Encode;
                    self.recompute();
                }
                if ui
                    .selectable_label(self.mode == Mode::Decode, "🔤  Morse → Text")
                    .clicked()
                {
                    self.mode = Mode::Decode;
                    self.recompute();
                }
            });

            ui.add_space(10.0);
            ui.label(match self.mode {
                Mode::Encode => "Text:",
                Mode::Decode => "Morse (letters space-separated, / between words):",
            });
            if ui
                .add(egui::TextEdit::singleline(&mut self.input).desired_width(f32::INFINITY))
                .changed()
            {
                self.recompute();
            }

            if self.mode == Mode::Encode {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Prosigns:");
                    for (name, meaning) in PROSIGNS {
                        if ui
                            .small_button(format!("<{name}>"))
                            .on_hover_text(*meaning)
                            .clicked()
                        {
                            self.insert_prosign(name);
                        }
                    }
                });
            }

            ui.add_space(12.0);
            ui.label("Result:");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.output.clone())
                        .desired_rows(2)
                        .desired_width(ui.available_width() - 70.0)
                        .interactive(false),
                );
                if ui.button("📋 Copy").clicked() {
                    ui.ctx().copy_text(self.output.clone());
                    self.status = "Copied to clipboard.".to_string();
                }
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(10.0);

            // ---- Speed controls -------------------------------------------------
            ui.label(egui::RichText::new("Transmission speed").strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Character speed:");
                ui.add(egui::Slider::new(&mut self.char_wpm, 5.0..=40.0).suffix(" WPM"));
            });

            ui.add_space(2.0);
            ui.checkbox(&mut self.farnsworth_enabled, "Farnsworth timing")
                .on_hover_text(
                    "Send characters at full speed but stretch the pauses between \
                     letters/words — the method recommended by the ARRL and CW \
                     Academy for learning Morse, since it avoids the \"counting \
                     dits and dashes\" habit that caps your speed later.",
                );
            if self.farnsworth_enabled {
                ui.horizontal(|ui| {
                    ui.label("Effective speed:");
                    ui.add(
                        egui::Slider::new(&mut self.effective_wpm, 2.0..=self.char_wpm.max(2.0))
                            .suffix(" WPM"),
                    );
                });
            }

            ui.add_space(16.0);
            ui.add_enabled_ui(!transmitting, |ui| {
                if ui
                    .add_sized([160.0, 32.0], egui::Button::new("▶  Transmit"))
                    .clicked()
                {
                    self.status.clear();
                    self.spawn_transmission();
                }
            });

            ui.add_space(16.0);
            let lit = self.is_lit.load(Ordering::SeqCst);
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 80.0), egui::Sense::hover());
            let color = if lit {
                egui::Color32::from_rgb(255, 210, 60)
            } else {
                egui::Color32::from_gray(40)
            };
            ui.painter().rect_filled(rect, 6.0, color);

            ui.add_space(8.0);
            if !self.status.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(120, 200, 120), &self.status);
            } else {
                ui.small(
                    "Tip: try \"SOS\" first, or click a prosign like <AR> to add it to your text.",
                );
            }
        });
    }
}
