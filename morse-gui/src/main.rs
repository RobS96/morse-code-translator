//! Cross-platform (Windows/macOS/Linux) desktop GUI for the Morse code
//! translator. Wraps `morse-core` with a simple, friendly interface:
//! type text or Morse, see the translation instantly, and hit "Transmit"
//! to watch a flashing lamp and hear a tone play out the real timing —
//! optionally with Farnsworth timing (fast characters, slower spacing),
//! the internationally recommended way to learn Morse. Supports the Latin,
//! Cyrillic, Greek, Hebrew, Arabic, Persian, Japanese (Wabun) and Korean
//! Morse alphabets, with the interface in 12 languages.
#![forbid(unsafe_code)]
// A release build for Windows is a GUI program: no console window opens
// alongside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod fonts;
mod i18n;
mod transmit;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use i18n::{Lang, Msg, tr};
use morse_core::{
    Alphabet, ScheduleStep, Timing, build_schedule, build_signal_plan_in, decode_in,
    decode_lossy_report_in, encode, encode_lossy_report_in, render_schedule,
};
use rodio::buffer::SamplesBuffer;
use rodio::{ChannelCount, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate};
use transmit::{
    DEFAULT_TONE_HZ, DEFAULT_VOLUME_PERCENT, MAX_TONE_HZ, MIN_TONE_HZ, POLL, TransmitGuard,
    left_out_chars, left_out_codes, run_lamp, timing_for, tone_for, wait_until,
};

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

/// Tallest the result box grows before it scrolls instead.
const RESULT_MAX_HEIGHT: f32 = 96.0;

/// Colour of the status-line warnings.
const WARNING_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 170, 60);

/// Longest the transmit thread waits, once the lamp is done, for the
/// player to hand the end of the audio to the device.
const DRAIN_LIMIT: Duration = Duration::from_secs(2);
/// How long the device is then kept open, so that what it has buffered is
/// heard before the device is closed.
const DRAIN_TAIL: Duration = Duration::from_millis(150);

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([520.0, 720.0])
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
    /// What the translation in `output` left out, as the status line
    /// lists it; empty if nothing was.
    left_out: String,
    mode: Mode,
    char_wpm: f64,
    farnsworth_enabled: bool,
    effective_wpm: f64,
    tone_hz: f32,
    volume_percent: f32,
    /// Shared with the background transmit thread: true while a dot/dash
    /// tone+flash is actively "on".
    is_lit: Arc<AtomicBool>,
    /// Shared with the background transmit thread: true for the whole
    /// duration of a transmission, used to swap which of the Transmit
    /// and Stop buttons is enabled + keep the UI repainting.
    is_transmitting: Arc<AtomicBool>,
    /// Raised by the Stop button and watched by the transmit thread.
    /// Every transmission gets a flag of its own.
    cancel: Arc<AtomicBool>,
    /// Raised by the transmit thread when it has no audio to play.
    audio_failed: Arc<AtomicBool>,
    /// A message for the status line, kept until the next transmission.
    notice: Option<Msg>,
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
            left_out: String::new(),
            mode: Mode::Encode,
            char_wpm: 20.0,
            farnsworth_enabled: false,
            effective_wpm: 5.0,
            tone_hz: DEFAULT_TONE_HZ,
            volume_percent: DEFAULT_VOLUME_PERCENT,
            is_lit: Arc::new(AtomicBool::new(false)),
            is_transmitting: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
            audio_failed: Arc::new(AtomicBool::new(false)),
            notice: None,
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
        (self.output, self.left_out) = match self.mode {
            Mode::Encode => {
                let report = encode_lossy_report_in(&self.input, alphabet);
                (report.morse, left_out_chars(&report.skipped))
            }
            Mode::Decode => {
                let report = decode_lossy_report_in(&self.input, alphabet);
                (report.text, left_out_codes(&report.skipped))
            }
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
        timing_for(
            self.char_wpm,
            self.farnsworth_enabled.then_some(self.effective_wpm),
        )
    }

    fn insert_prosign(&mut self, name: &str) {
        if !self.input.is_empty() && !self.input.ends_with(' ') {
            self.input.push(' ');
        }
        self.input.push_str(&format!("<{name}>"));
        self.recompute();
    }

    fn spawn_transmission(&mut self) {
        // Transmission always plays the *text* form, so decode first if
        // the user has Morse loaded on the Decode tab.
        let alphabet = self.effective_alphabet();
        let text = match self.mode {
            Mode::Encode => self.input.clone(),
            Mode::Decode => decode_in(&self.input, alphabet),
        };
        let schedule = build_schedule(&build_signal_plan_in(&text, alphabet), self.timing());
        if schedule.is_empty() {
            return;
        }
        let (tone_hz, volume_percent) = (self.tone_hz, self.volume_percent);
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let is_lit = self.is_lit.clone();
        let audio_failed = self.audio_failed.clone();
        let guard = TransmitGuard::engage(self.is_transmitting.clone(), self.is_lit.clone());

        thread::spawn(move || {
            let _guard = guard;
            // With no audio the lamp still runs; the status line says why
            // it is silent.
            let playback = Playback::start(&schedule, tone_hz, volume_percent);
            if playback.is_none() {
                audio_failed.store(true, Ordering::SeqCst);
            }
            let started = Instant::now();
            let completed = run_lamp(
                &schedule,
                &is_lit,
                &cancel,
                || started.elapsed(),
                thread::sleep,
            );
            if let Some(playback) = playback {
                playback.finish(completed, &cancel);
            }
        });
    }
}

/// One transmission's audio: the whole schedule rendered once, playing on
/// the default output device for as long as this is alive.
struct Playback {
    player: Player,
    // After `player`, so that it is dropped last: closing the device is
    // what ends the sound.
    _device: MixerDeviceSink,
}

impl Playback {
    /// Render `schedule` and start playing it. `None` if no output device
    /// can be opened or the schedule cannot be rendered.
    fn start(schedule: &[ScheduleStep], tone_hz: f32, volume_percent: f32) -> Option<Self> {
        let mut device = DeviceSinkBuilder::open_default_sink().ok()?;
        // Otherwise rodio prints a notice to stderr every time a
        // transmission ends.
        device.log_on_drop(false);
        let tone = tone_for(tone_hz, volume_percent, device.config().sample_rate().get());
        let samples = render_schedule(schedule, &tone).ok()?;
        let player = Player::connect_new(device.mixer());
        player.append(SamplesBuffer::new(
            ChannelCount::MIN,
            SampleRate::new(tone.sample_rate)?,
            samples,
        ));
        Some(Self {
            player,
            _device: device,
        })
    }

    /// End the transmission's audio. One that ran to its end is first
    /// given time to be heard: the device plays a little behind the clock
    /// the lamp follows. A cancelled one is cut off at once.
    fn finish(self, completed: bool, cancel: &AtomicBool) {
        if completed {
            // In slices, so that Stop still works while the audio drains.
            let since = Instant::now();
            while !self.player.empty()
                && since.elapsed() < DRAIN_LIMIT
                && !cancel.load(Ordering::SeqCst)
            {
                thread::sleep(POLL);
            }
            let since = Instant::now();
            wait_until(DRAIN_TAIL, cancel, || since.elapsed(), thread::sleep);
        }
        self.player.stop();
    }
}

impl eframe::App for MorseApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let transmitting = self.is_transmitting.load(Ordering::SeqCst);
        if transmitting {
            ui.ctx().request_repaint(); // keep animating the lamp
        }
        if self.audio_failed.swap(false, Ordering::SeqCst) {
            self.notice = Some(Msg::AudioUnavailable);
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
            ui.horizontal(|ui| {
                ui.label(t(Msg::Result));
                if ui.button(format!("📋 {}", t(Msg::Copy))).clicked() {
                    ui.ctx().copy_text(self.output.clone());
                    self.notice = Some(Msg::Copied);
                }
            });
            // A long result scrolls inside its box instead of pushing the
            // controls below it out of the window.
            egui::ScrollArea::vertical()
                .id_salt("result")
                .max_height(RESULT_MAX_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.output.as_str())
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
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

            // ---- Tone controls --------------------------------------------------
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(t(Msg::ToneLabel));
                ui.add(
                    egui::Slider::new(&mut self.tone_hz, MIN_TONE_HZ..=MAX_TONE_HZ)
                        .step_by(10.0)
                        .fixed_decimals(0)
                        .suffix(" Hz"),
                );
            });
            ui.horizontal(|ui| {
                ui.label(t(Msg::VolumeLabel));
                ui.add(
                    egui::Slider::new(&mut self.volume_percent, 0.0..=100.0)
                        .step_by(1.0)
                        .fixed_decimals(0)
                        .suffix(" %"),
                );
            });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!transmitting, |ui| {
                    if ui
                        .add_sized(
                            [160.0, 32.0],
                            egui::Button::new(format!("▶  {}", t(Msg::Transmit))),
                        )
                        .clicked()
                    {
                        self.notice = None;
                        self.spawn_transmission();
                        ui.ctx().request_repaint();
                    }
                });
                ui.add_enabled_ui(transmitting, |ui| {
                    if ui
                        .add_sized(
                            [110.0, 32.0],
                            egui::Button::new(format!("⏹  {}", t(Msg::Stop))),
                        )
                        .clicked()
                    {
                        self.cancel.store(true, Ordering::SeqCst);
                    }
                });
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
                ui.colored_label(WARNING_COLOR, t(Msg::MissingFont));
            }
            if !self.left_out.is_empty() {
                let label = match self.mode {
                    Mode::Encode => Msg::LeftOutChars,
                    Mode::Decode => Msg::LeftOutCodes,
                };
                ui.colored_label(WARNING_COLOR, format!("{} {}", t(label), self.left_out));
            }
            match self.notice {
                Some(Msg::AudioUnavailable) => {
                    ui.colored_label(WARNING_COLOR, t(Msg::AudioUnavailable));
                }
                Some(notice) => {
                    ui.colored_label(egui::Color32::from_rgb(120, 200, 120), t(notice));
                }
                None if self.left_out.is_empty() => {
                    ui.small(t(Msg::Tip));
                }
                None => {}
            }
        });
    }
}
