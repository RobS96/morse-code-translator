//! Cross-platform (Windows/macOS/Linux) desktop GUI for the Morse code
//! translator. Wraps `morse-core` with a simple, friendly interface:
//! type text or Morse, see the translation instantly, and hit "Transmit"
//! to watch a flashing lamp and hear a tone play out the real timing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use eframe::egui;
use morse_core::{Signal, UNIT_MS, build_signal_plan, decode, encode};
use rodio::source::{SineWave, Source};
use rodio::{OutputStream, Sink};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([480.0, 420.0]),
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
    unit_ms: u64,
    /// Shared with the background transmit thread: true while a dot/dash
    /// tone+flash is actively "on".
    is_lit: Arc<AtomicBool>,
    /// Shared with the background transmit thread: true for the whole
    /// duration of a transmission, used to disable the button + keep
    /// the UI repainting.
    is_transmitting: Arc<AtomicBool>,
}

impl Default for MorseApp {
    fn default() -> Self {
        Self {
            input: "SOS".to_string(),
            output: encode("SOS"),
            mode: Mode::Encode,
            unit_ms: UNIT_MS,
            is_lit: Arc::new(AtomicBool::new(false)),
            is_transmitting: Arc::new(AtomicBool::new(false)),
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

    fn spawn_transmission(&self) {
        // Transmission always plays the *text* form, so decode first if
        // the user has Morse loaded on the Decode tab.
        let text = match self.mode {
            Mode::Encode => self.input.clone(),
            Mode::Decode => decode(&self.input),
        };
        let unit_ms = self.unit_ms;
        let is_lit = self.is_lit.clone();
        let is_transmitting = self.is_transmitting.clone();

        is_transmitting.store(true, Ordering::SeqCst);

        thread::spawn(move || {
            // One audio output per transmission; kept alive for the
            // thread's lifetime so tones don't get cut off.
            let stream = OutputStream::try_default();
            let sink = stream
                .as_ref()
                .ok()
                .and_then(|(_, handle)| Sink::try_new(handle).ok());

            for signal in build_signal_plan(&text) {
                if signal.is_tone() {
                    let dur = Duration::from_millis(signal.duration_ms(unit_ms));
                    is_lit.store(true, Ordering::SeqCst);
                    if let Some(sink) = &sink {
                        let tone = SineWave::new(600.0).take_duration(dur).amplify(0.20);
                        sink.append(tone);
                    }
                    thread::sleep(dur);
                    is_lit.store(false, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(unit_ms)); // symbol gap
                } else {
                    let _ = matches!(signal, Signal::WordGap);
                    thread::sleep(Duration::from_millis(signal.duration_ms(unit_ms)));
                }
            }
            if let Some(sink) = &sink {
                sink.sleep_until_end();
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
            ui.heading("Morse Code Translator");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.mode == Mode::Encode, "Text -> Morse")
                    .clicked()
                {
                    self.mode = Mode::Encode;
                    self.recompute();
                }
                if ui
                    .selectable_label(self.mode == Mode::Decode, "Morse -> Text")
                    .clicked()
                {
                    self.mode = Mode::Decode;
                    self.recompute();
                }
            });

            ui.add_space(8.0);
            ui.label(match self.mode {
                Mode::Encode => "Text:",
                Mode::Decode => "Morse (letters space-separated, / between words):",
            });
            if ui.text_edit_singleline(&mut self.input).changed() {
                self.recompute();
            }

            ui.add_space(8.0);
            ui.label("Result:");
            ui.add(
                egui::TextEdit::multiline(&mut self.output.clone())
                    .desired_rows(2)
                    .interactive(false),
            );

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label("Speed:");
                ui.add(
                    egui::Slider::new(&mut self.unit_ms, 30..=200)
                        .suffix(" ms/unit")
                        .text(""),
                );
            });

            ui.add_space(12.0);
            ui.add_enabled_ui(!transmitting, |ui| {
                if ui.button("▶ Transmit (flash + beep)").clicked() {
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
            ui.small("Tip: try \"SOS\" first — three short flashes, three long, three short.");
        });
    }
}
