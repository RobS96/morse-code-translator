//! The interface laid out without a window. egui computes the same sizes it
//! would on screen, so this catches text that pushes a control out of the
//! window in some language, which no other test can see.

use super::*;

/// The size the window opens at and the smallest it can be dragged to
/// (`main`'s `with_inner_size` / `with_min_inner_size`).
const OPENING_SIZE: egui::Vec2 = egui::vec2(520.0, 720.0);
const MINIMUM_SIZE: egui::Vec2 = egui::vec2(420.0, 520.0);

struct Layout {
    /// What the content needs.
    content: egui::Vec2,
    /// The room the scroll area had for it.
    room: egui::Vec2,
    /// What the whole panel occupied on screen.
    panel: egui::Rect,
}

/// Lays the app out in a window of `size`. A few frames, because egui sizes
/// some widgets from the previous frame.
fn lay_out(app: &mut MorseApp, size: egui::Vec2) -> Layout {
    let ctx = egui::Context::default();
    fonts::install_fallbacks(&ctx);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };
    let mut layout = None;
    for _ in 0..4 {
        let mut output = ctx.run_ui(input.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let (content, room) = app.show(ui);
                layout = Some(Layout {
                    content,
                    room,
                    panel: ui.min_rect(),
                });
            });
        });
        // Nothing is painted here, so the font-atlas upload is not needed.
        output.textures_delta.clear();
    }
    layout.expect("the panel was shown")
}

/// The fullest the window gets: prosign chips, a long result, the second
/// speed slider, and a status line listing what was left out.
fn busiest(lang: Lang, mode: Mode) -> MorseApp {
    let mut app = MorseApp {
        lang,
        farnsworth_enabled: true,
        ..Default::default()
    };
    app.input = match mode {
        Mode::Encode => "THE QUICK BROWN FOX § JUMPS OVER THE LAZY DOG ".repeat(40),
        Mode::Decode => "... --- ... ........ / .-.-.- ......... ".repeat(40),
    };
    app.mode = mode;
    app.recompute();
    assert!(!app.left_out.is_empty(), "the status line is exercised");
    app
}

#[test]
fn everything_fits_the_window_it_opens_at_in_every_language() {
    for lang in Lang::ALL {
        for mode in [Mode::Encode, Mode::Decode] {
            let l = lay_out(&mut busiest(lang, mode), OPENING_SIZE);
            let name = lang.native_name();
            assert!(
                l.content.y <= l.room.y + 0.5,
                "{name}: content is {} high, the window has room for {}",
                l.content.y,
                l.room.y
            );
            assert!(
                l.content.x <= l.room.x + 0.5,
                "{name}: content is {} wide, the window has room for {}",
                l.content.x,
                l.room.x
            );
        }
    }
}

#[test]
fn the_smallest_window_scrolls_instead_of_cutting_controls_off() {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, MINIMUM_SIZE);
    for lang in Lang::ALL {
        let l = lay_out(&mut busiest(lang, Mode::Encode), MINIMUM_SIZE);
        let name = lang.native_name();
        // Taller than the window is fine: that is what scrolls.
        assert!(
            screen.contains_rect(l.panel),
            "{name}: the panel occupies {:?}, the window is {screen:?}",
            l.panel
        );
        assert!(
            l.content.x <= l.room.x + 0.5,
            "{name}: content is {} wide, the window has room for {}",
            l.content.x,
            l.room.x
        );
    }
}

#[test]
fn a_transmission_in_progress_lays_out_too() {
    let mut app = busiest(Lang::En, Mode::Encode);
    app.is_transmitting.store(true, Ordering::SeqCst);
    app.is_lit.store(true, Ordering::SeqCst);
    app.notice = Some(Msg::AudioUnavailable);
    let l = lay_out(&mut app, OPENING_SIZE);
    assert!(l.content.y <= l.room.y + 0.5);
}

#[test]
fn ukrainian_is_detected_or_picked_and_its_label_fits_the_smallest_window() {
    for lang in Lang::ALL {
        let mut app = busiest(lang, Mode::Encode);
        app.input = "ПРИВІТ § ".repeat(40);
        app.recompute();
        assert_eq!(app.effective_alphabet(), Alphabet::Ukrainian);
        assert!(app.output.starts_with(".--. .-. -.-- .-- .. - / "));
        let l = lay_out(&mut app, MINIMUM_SIZE);
        let name = lang.native_name();
        assert!(
            l.content.x <= l.room.x + 0.5,
            "{name}: content is {} wide, the window has room for {}",
            l.content.x,
            l.room.x
        );
    }
    // Picked by hand, it decodes: Morse cannot be detected.
    let mut app = MorseApp {
        mode: Mode::Decode,
        input: ".--. .-. -.-- .-- .. -".to_string(),
        ..Default::default()
    };
    app.recompute();
    assert_eq!(app.output, "PRYWIT");
    app.alphabet = Some(Alphabet::Ukrainian);
    app.recompute();
    assert_eq!(app.output, "ПРИВІТ");
    let l = lay_out(&mut app, OPENING_SIZE);
    assert!(l.content.y <= l.room.y + 0.5);
}

#[test]
fn the_bundled_fonts_cover_the_alphabets_that_need_no_system_font() {
    // No fallback fonts installed: only what egui ships.
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
    // A character no font has is drawn as a replacement glyph, and its
    // own width is reported as zero.
    let covered = |text: &str| {
        [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
            .into_iter()
            .all(|family| {
                let font = egui::FontId::new(14.0, family);
                text.chars()
                    .all(|c| ctx.fonts_mut(|fonts| fonts.glyph_width(&font, c)) > 0.0)
            })
    };
    assert!(covered("ABCXYZabcxyz"));
    assert!(covered(
        "АБВГДЕЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯЁабвгдежзийклмнопрстуфхцчшщъыьэюяё"
    ));
    assert!(covered("ҐЄІЇґєії"));
    assert!(covered("ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩαβγδεζηθικλμνξοπρστυφχψως"));
    for uncovered in ["א", "ب", "あ", "한"] {
        assert!(!covered(uncovered), "{uncovered}");
    }
    for alphabet in [
        Alphabet::Latin,
        Alphabet::Cyrillic,
        Alphabet::Ukrainian,
        Alphabet::Greek,
    ] {
        let app = MorseApp {
            alphabet: Some(alphabet),
            fonts_available: false,
            ..Default::default()
        };
        assert!(!app.missing_font(), "{alphabet:?}");
    }
    let app = MorseApp {
        alphabet: Some(Alphabet::Hebrew),
        fonts_available: false,
        ..Default::default()
    };
    assert!(app.missing_font());
}
