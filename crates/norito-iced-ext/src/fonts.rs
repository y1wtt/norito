//! The text face. On Linux it follows fontconfig: cosmic-text reads only its
//! font folders, names its own sans-serif and monospace faces, and sets kanji
//! in a Chinese face unless the locale is exactly `ja`.
use iced::advanced::graphics::text::{self, cosmic_text::fontdb};
use iced::{Font, font};
use std::process::Command;

const FONTCONFIG: bool = cfg!(all(unix, not(target_os = "macos")));

/// Body and chrome text.
pub const TEXT: Font = Font {
    family: if FONTCONFIG {
        font::Family::SansSerif
    } else {
        font::Family::Name("Yu Gothic UI")
    },
    ..Font::DEFAULT
};

/// Points the sans-serif and monospace families at the faces fontconfig picks
/// for Japanese. Does nothing without fontconfig. Call before any text is shaped.
pub fn follow_fontconfig() {
    if !FONTCONFIG {
        return;
    }
    let mut font_system = text::font_system().write().expect("Write font system");
    let db = font_system.raw().db_mut();
    if let Some(family) = matched(db, "sans-serif") {
        db.set_sans_serif_family(family);
    }
    if let Some(family) = matched(db, "monospace") {
        db.set_monospace_family(family);
    }
}

/// The first name of fontconfig's Japanese match for `generic` that `db` holds.
fn matched(db: &fontdb::Database, generic: &str) -> Option<String> {
    let output = Command::new("fc-match")
        .args(["-f", "%{family}", &format!("{generic}:lang=ja")])
        .output()
        .ok()?;
    let families = String::from_utf8(output.stdout).ok()?;
    let held = |family: &str| {
        db.faces()
            .any(|face| face.families.iter().any(|(name, _)| name == family))
    };
    first_held(&families, held).map(str::to_owned)
}

/// fontconfig lists a face's names in each language, comma-separated.
fn first_held(families: &str, held: impl Fn(&str) -> bool) -> Option<&str> {
    families.split(',').find(|family| held(family))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_name_the_database_holds() {
        let families = "IPAexGothic,IPAexゴシック";
        assert_eq!(
            first_held(families, |family| family == "IPAexゴシック"),
            Some("IPAexゴシック")
        );
        assert_eq!(first_held(families, |_| true), Some("IPAexGothic"));
        assert_eq!(first_held(families, |_| false), None);
    }
}
