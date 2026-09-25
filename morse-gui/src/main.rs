//! Cross-platform (Windows/macOS/Linux) desktop GUI for the Morse code
//! translator. Wraps `morse-core` with a simple, friendly interface:
//! type text or Morse, see the translation instantly, and hit "Transmit"
//! to watch a flashing lamp and hear a tone play out the real timing —
//! optionally with Farnsworth timing (fast characters, slower spacing),
//! the internationally recommended way to learn Morse. Supports the Latin,
//! Cyrillic, Greek, Hebrew, Arabic, Persian, Japanese (Wabun) and Korean
//! Morse alphabets, with the interface in 12 languages.
#![forbid(unsafe_code)]

mod fonts;
mod i18n;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use eframe::egui;
use i18n::{Lang, Msg, tr};
use morse_core::{
    Alphabet, Timing, build_signal_plan_in, decode_in, encode, encode_in, wpm_to_unit_ms,
};
use rodio::source::{SineWave, Source};
use rodio::{DeviceSinkBuilder, Player};

/// Common procedural signs, offered as one-click inserts in Encode mode.
/// (name, translatable meaning)
const PROSIGNS: &[(&str, Msg)] = &[
    ("AR", Msg::PsEndOfMessage),
    ("SK", Msg::PsEndOfContact),
    ("BT", Msg::PsNewParagraph),
    ("KN", Msg::PsOverToYou),
    ("AS", Msg::PsWait),
    ("CT", Msg::PsStartCopying),
];

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([520.0, 620.0])
            .with_min_inner_size([420.0, 520.0]),
        ..Default::default()
    };
    let lang = Lang::from_env();
    eframe::run_native(
        tr(lang, Msg::AppTitle),
        options,
        Box::new(move |cc| {
            let fonts_available = fonts::install_fallbacks(&cc.egui_ctx);
            Ok(Box::new(MorseApp {
                lang,
                fonts_available,
                ..Default::default()
            }))
        }),
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
    lang: Lang,
    /// `None` = auto-detect from the text (encode) / Latin (decode).
    alphabet: Option<Alphabet>,
    fonts_available: bool,
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
            lang: Lang::En,
            alphabet: None,
            fonts_available: true,
        }
    }
}

impl MorseApp {
    /// The alphabet actually in use: the user's pick, else detected from
    /// the input text. Morse input can't be detected, so decode uses Latin.
    fn effective_alphabet(&self) -> Alphabet {
        self.alphabet.unwrap_or(match self.mode {
            Mode::Encode => Alphabet::detect(&self.input),
            Mode::Decode => Alphabet::Latin,
        })
    }

    fn recompute(&mut self) {
        let alphabet = self.effective_alphabet();
        self.output = match self.mode {
            Mode::Encode => encode_in(&self.input, alphabet),
            Mode::Decode => decode_in(&self.input, alphabet),
        };
    }

    /// True if the current text or UI needs glyphs egui doesn't bundle and
    /// no system fallback font was found.
    fn missing_font(&self) -> bool {
        if self.fonts_available {
            return false;
        }
        let script_needs_font = !matches!(
            self.effective_alphabet(),
            Alphabet::Latin | Alphabet::Cyrillic | Alphabet::Greek
        );
        script_needs_font || self.lang.needs_cjk_font()
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
        let alphabet = self.effective_alphabet();
        let text = match self.mode {
            Mode::Encode => self.input.clone(),
            Mode::Decode => decode_in(&self.input, alphabet),
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

            for signal in build_signal_plan_in(&text, alphabet) {
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

        let lang = self.lang;
        let t = |msg| tr(lang, msg);

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(t(Msg::AppTitle));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("ui-language")
                        .selected_text(self.lang.native_name())
                        .show_ui(ui, |ui| {
                            for l in Lang::ALL {
                                ui.selectable_value(&mut self.lang, l, l.native_name());
                            }
                        });
                    ui.label(t(Msg::Language));
                });
            });
            ui.add_space(10.0);

            // ---- Mode switch ---------------------------------------------------
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        self.mode == Mode::Encode,
                        format!("📝  {}", t(Msg::TextToMorse)),
                    )
                    .clicked()
                {
                    self.mode = Mode::Encode;
                    self.recompute();
                }
                if ui
                    .selectable_label(
                        self.mode == Mode::Decode,
                        format!("🔤  {}", t(Msg::MorseToText)),
                    )
                    .clicked()
                {
                    self.mode = Mode::Decode;
                    self.recompute();
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t(Msg::AlphabetLabel));
                let auto_label = match self.mode {
                    Mode::Encode => format!(
                        "{} ({})",
                        t(Msg::AutoDetect),
                        Alphabet::detect(&self.input).native_name()
                    ),
                    Mode::Decode => {
                        format!("{} ({})", t(Msg::AutoDetect), Alphabet::Latin.native_name())
                    }
                };
                let selected = self
                    .alphabet
                    .map_or(auto_label.clone(), |a| a.native_name().to_string());
                let before = self.alphabet;
                egui::ComboBox::from_id_salt("alphabet")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.alphabet, None, auto_label);
                        for a in Alphabet::ALL {
                            ui.selectable_value(&mut self.alphabet, Some(a), a.native_name());
                        }
                    });
                if self.alphabet != before {
                    self.recompute();
                }
            });

            ui.add_space(10.0);
            ui.label(match self.mode {
                Mode::Encode => t(Msg::TextLabel),
                Mode::Decode => t(Msg::MorseLabel),
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
                    ui.label(t(Msg::Prosigns));
                    for (name, meaning) in PROSIGNS {
                        if ui
                            .small_button(format!("<{name}>"))
                            .on_hover_text(t(*meaning))
                            .clicked()
                        {
                            self.insert_prosign(name);
                        }
                    }
                });
            }

            ui.add_space(12.0);
            ui.label(t(Msg::Result));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.output.clone())
                        .desired_rows(2)
                        .desired_width(ui.available_width() - 70.0)
                        .interactive(false),
                );
                if ui.button(format!("📋 {}", t(Msg::Copy))).clicked() {
                    ui.ctx().copy_text(self.output.clone());
                    self.status = t(Msg::Copied).to_string();
                }
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(10.0);

            // ---- Speed controls -------------------------------------------------
            ui.label(egui::RichText::new(t(Msg::SpeedHeading)).strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(t(Msg::CharSpeed));
                ui.add(egui::Slider::new(&mut self.char_wpm, 5.0..=40.0).suffix(" WPM"));
            });

            ui.add_space(2.0);
            ui.checkbox(&mut self.farnsworth_enabled, t(Msg::Farnsworth))
                .on_hover_text(t(Msg::FarnsworthHelp));
            if self.farnsworth_enabled {
                ui.horizontal(|ui| {
                    ui.label(t(Msg::EffectiveSpeed));
                    ui.add(
                        egui::Slider::new(&mut self.effective_wpm, 2.0..=self.char_wpm.max(2.0))
                            .suffix(" WPM"),
                    );
                });
            }

            ui.add_space(16.0);
            ui.add_enabled_ui(!transmitting, |ui| {
                if ui
                    .add_sized(
                        [160.0, 32.0],
                        egui::Button::new(format!("▶  {}", t(Msg::Transmit))),
                    )
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
            if self.missing_font() {
                ui.colored_label(egui::Color32::from_rgb(230, 170, 60), t(Msg::MissingFont));
            }
            if !self.status.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(120, 200, 120), &self.status);
            } else {
                ui.small(t(Msg::Tip));
            }
        });
    }
}
