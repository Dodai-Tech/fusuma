//! How many colours the terminal shows, from the environment.

use fusuma_cells::Depth;

/// The colour depth the environment describes. `no_color` is a non-empty `NO_COLOR` (<https://no-color.org>),
/// `colorterm` and `term` the variables of those names, `windows_terminal` whether `WT_SESSION` is set (Windows
/// Terminal shows 24-bit colour and sets no `TERM`). Unknown terminals get the 16 colours every terminal has.
#[must_use]
pub fn depth_from(
    no_color: bool,
    colorterm: Option<&str>,
    term: Option<&str>,
    windows_terminal: bool,
) -> Depth {
    if no_color {
        return Depth::Mono;
    }
    if matches!(colorterm, Some("truecolor" | "24bit")) || windows_terminal {
        return Depth::TrueColor;
    }
    match term {
        Some("dumb") => Depth::Mono,
        Some(t) if t.contains("truecolor") || t.contains("direct") => Depth::TrueColor,
        Some(t) if t.contains("256color") => Depth::Ansi256,
        _ => Depth::Ansi16,
    }
}

/// The colour depth of the terminal this process runs in.
#[must_use]
pub fn detect_depth() -> Depth {
    let var = |name| std::env::var(name).ok();
    depth_from(
        var("NO_COLOR").is_some_and(|v| !v.is_empty()),
        var("COLORTERM").as_deref(),
        var("TERM").as_deref(),
        std::env::var_os("WT_SESSION").is_some(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_names_the_depth() {
        assert_eq!(
            depth_from(false, Some("truecolor"), Some("xterm"), false),
            Depth::TrueColor
        );
        assert_eq!(
            depth_from(false, Some("24bit"), None, false),
            Depth::TrueColor
        );
        assert_eq!(
            depth_from(false, None, Some("xterm-256color"), false),
            Depth::Ansi256
        );
        assert_eq!(
            depth_from(false, None, Some("xterm-direct"), false),
            Depth::TrueColor
        );
        assert_eq!(depth_from(false, None, Some("vt100"), false), Depth::Ansi16);
        assert_eq!(depth_from(false, None, None, false), Depth::Ansi16);
        assert_eq!(depth_from(false, None, None, true), Depth::TrueColor);
    }

    #[test]
    fn no_color_and_a_dumb_terminal_mean_no_colour() {
        assert_eq!(
            depth_from(true, Some("truecolor"), Some("xterm-256color"), true),
            Depth::Mono
        );
        assert_eq!(depth_from(false, None, Some("dumb"), false), Depth::Mono);
    }
}
