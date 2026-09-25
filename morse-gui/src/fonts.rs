//! System-font fallbacks for scripts egui's bundled fonts don't cover.
//!
//! egui ships fonts for Latin, Greek and Cyrillic only. Hebrew, Arabic,
//! kana, Hangul and Han characters need a font from the operating system;
//! nothing is bundled or downloaded. Fonts are appended *after* egui's own,
//! so they're only used for glyphs the defaults lack.

use std::sync::Arc;

use eframe::egui;

/// Candidate files, grouped by what they cover. The first readable file in
/// a group is loaded. A "broad" font covering every script ends the search.
const BROAD: &[&str] = &["/System/Library/Fonts/Supplemental/Arial Unicode.ttf"];
const GROUPS: &[&[&str]] = &[
    // Japanese + Chinese
    &[
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "C:\\Windows\\Fonts\\YuGothM.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
    ],
    // Korean
    &[
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
        "C:\\Windows\\Fonts\\malgun.ttf",
    ],
    // Hebrew + Arabic
    &[
        "/System/Library/Fonts/ArialHB.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ],
    &[
        "/System/Library/Fonts/GeezaPro.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
    ],
    // Simplified Chinese on Windows (Yu Gothic lacks many hanzi)
    &["C:\\Windows\\Fonts\\msyh.ttc"],
];

/// Skip anything implausibly large rather than holding it all in memory.
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

fn read_font(path: &str) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FONT_BYTES {
        return None;
    }
    std::fs::read(path).ok()
}

/// Install fallback fonts. Returns true if at least one was found.
pub fn install_fallbacks(ctx: &egui::Context) -> bool {
    let mut chosen: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some((path, bytes)) = BROAD.iter().find_map(|p| read_font(p).map(|b| (*p, b))) {
        chosen.push((path.to_string(), bytes));
    } else {
        for group in GROUPS {
            if let Some((path, bytes)) = group.iter().find_map(|p| read_font(p).map(|b| (*p, b))) {
                chosen.push((path.to_string(), bytes));
            }
        }
    }
    if chosen.is_empty() {
        return false;
    }

    let mut defs = egui::FontDefinitions::default();
    for (name, bytes) in chosen {
        defs.font_data
            .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            defs.families.entry(family).or_default().push(name.clone());
        }
    }
    ctx.set_fonts(defs);
    true
}
