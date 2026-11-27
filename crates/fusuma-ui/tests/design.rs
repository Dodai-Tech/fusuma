#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::format_push_string
)]
//! The design as files for design tools: W3C design tokens per system (a folder is a variable collection in Figma or
//! OpenPencil, a file a mode) and a written spec, both made from the same `Theme` and `Palette` constants the toolkit
//! paints with, so the design file cannot drift from the code.
//!
//! The test compares the checked-in files in `design/` with what the constants say and fails when they differ. Run it with
//! `FUSUMA_DESIGN_WRITE=1` (or `nu scripts/design.nu tokens`) to write them again.

use std::fmt::Write as _;
use std::path::PathBuf;

use fusuma_render::{Color, Font};
use fusuma_ui::{Colors, Palette, Theme};

/// A design system: its name, what it is for, its theme tokens and its colours.
struct System {
    id: &'static str,
    title: &'static str,
    about: &'static str,
    theme: Theme,
    palette: Palette,
}

fn systems() -> [System; 2] {
    [
        System {
            id: "cyberdeck",
            title: "Cyberdeck",
            about: "Near-black, white mono text, one orange accent, 1 px outlines, square corners, no shadow; corner ticks, a hatch on the chosen row, a dotted track and a progress bar round the frame.",
            theme: Theme::CYBERDECK,
            palette: Palette::CYBERDECK,
        },
        System {
            id: "fusuma",
            title: "Fusuma",
            about: "The default: a neutral blue-grey with a blue main action, rounded corners, soft shadow under floating panels.",
            theme: Theme::DARK,
            palette: Palette::FUSUMA,
        },
    ]
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn family(f: Font) -> &'static str {
    match f {
        Font::Sans | Font::SansBold => "Inter",
        Font::Mono => "JetBrains Mono",
        Font::Bitmap => "Bitmap",
        Font::Icons => "Material Symbols",
    }
}

/// The colour roles in the order the tokens and the spec list them: (token name, value).
fn roles(c: &Colors) -> [(&'static str, Color); 16] {
    [
        ("background", c.background),
        ("surface", c.surface),
        ("surface-raised", c.surface_raised),
        ("border", c.border),
        ("border-strong", c.border_strong),
        ("text", c.text),
        ("text-muted", c.text_muted),
        ("text-disabled", c.text_disabled),
        ("primary", c.primary),
        ("primary-hover", c.primary_hover),
        ("primary-active", c.primary_active),
        ("on-primary", c.on_primary),
        ("danger", c.danger),
        ("success", c.success),
        ("warning", c.warning),
        ("info", c.info),
    ]
}

/// Every number the theme says, as (group/name, value).
fn numbers(t: &Theme) -> Vec<(String, u32)> {
    let mut v = vec![
        ("space/xs", t.spacing.xs),
        ("space/sm", t.spacing.sm),
        ("space/md", t.spacing.md),
        ("space/lg", t.spacing.lg),
        ("radius/sm", t.radii.sm),
        ("radius/md", t.radii.md),
        ("radius/lg", t.radii.lg),
        ("text/caption", t.text.caption),
        ("text/body", t.text.body),
        ("text/title", t.text.title),
        ("text/icon", t.text.icon),
        ("size/control-height", t.control_height),
        ("size/border", t.border_width),
        ("size/indicator", t.indicator),
        ("size/track-height", t.track_height),
        ("size/elevation", t.elevation),
        ("decor/ticks", t.decor.ticks),
        ("decor/hatch", t.decor.hatch),
        ("decor/dots", t.decor.dots),
        ("motion/fast", t.motion.fast_ms),
        ("motion/normal", t.motion.normal_ms),
        ("motion/slow", t.motion.slow_ms),
    ];
    v.sort_by_key(|(k, _)| *k);
    v.into_iter().map(|(k, n)| (k.to_string(), n)).collect()
}

/// The tokens of one mode as W3C design-token JSON, groups nested by the first path segment.
fn tokens(t: &Theme, colors: &Colors) -> String {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |group: &str, name: &str, ty: &str, value: String| {
        let leaf = format!("\"{name}\":{{\"$type\":\"{ty}\",\"$value\":{value}}}");
        match groups.iter_mut().find(|(g, _)| g == group) {
            Some((_, l)) => l.push(leaf),
            None => groups.push((group.to_string(), vec![leaf])),
        }
    };
    for (name, c) in roles(colors) {
        add("color", name, "color", format!("\"{}\"", hex(c)));
    }
    for (path, n) in numbers(t) {
        let (group, name) = path.split_once('/').unwrap();
        // Durations are plain numbers of milliseconds: a design tool has no duration variable.
        let (ty, value) = ("number", n.to_string());
        add(group, name, ty, value);
    }
    add(
        "font",
        "body",
        "string",
        format!("\"{}\"", family(t.fonts.body)),
    );
    add(
        "font",
        "title",
        "string",
        format!("\"{}\"", family(t.fonts.title)),
    );
    add(
        "font",
        "mono",
        "string",
        format!("\"{}\"", family(t.fonts.mono)),
    );
    let mut out = String::from("{");
    for (i, (g, leaves)) in groups.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write!(out, "\"{g}\":{{{}}}", leaves.join(",")).unwrap();
    }
    out.push_str("}\n");
    out
}

fn lum(c: Color) -> f64 {
    let f = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
}

fn contrast(a: Color, b: Color) -> f64 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The written spec of one system.
fn spec(s: &System) -> String {
    let (t, p) = (&s.theme, &s.palette);
    let mut o = String::new();
    writeln!(o, "# {} design spec\n", s.title).unwrap();
    writeln!(o, "{}\n", s.about).unwrap();
    writeln!(o, "Generated from `Theme` and `Palette` by `crates/fusuma-ui/tests/design.rs`; do not edit. The tokens are in `design/tokens/{}/`, the design file is built with `nu scripts/design.nu build`.\n", s.id).unwrap();
    writeln!(o, "## Colour roles\n").unwrap();
    writeln!(
        o,
        "| role | dark | light | contrast dark | contrast light |"
    )
    .unwrap();
    writeln!(o, "| --- | --- | --- | --- | --- |").unwrap();
    let (d, l) = (roles(&p.dark), roles(&p.light));
    for ((name, dc), (_, lc)) in d.iter().zip(l.iter()) {
        // Text roles are read on the surface, the rest are shown against the page.
        let on = |c: &Colors| {
            if name.starts_with("text")
                || matches!(*name, "primary" | "danger" | "success" | "warning" | "info")
            {
                c.surface
            } else if *name == "on-primary" {
                c.primary
            } else {
                c.background
            }
        };
        writeln!(
            o,
            "| `{name}` | `{}` | `{}` | {:.1} | {:.1} |",
            hex(*dc),
            hex(*lc),
            contrast(*dc, on(&p.dark)),
            contrast(*lc, on(&p.light))
        )
        .unwrap();
    }
    writeln!(o, "\nContrast is the WCAG ratio of the colour against the surface it is read on (`on-primary` on `primary`, outlines and fills against the page).\n").unwrap();
    writeln!(o, "## Typography\n").unwrap();
    writeln!(o, "| role | font | size |\n| --- | --- | --- |").unwrap();
    writeln!(
        o,
        "| title | {} | {} |",
        family(t.fonts.title),
        t.text.title
    )
    .unwrap();
    writeln!(o, "| body | {} | {} |", family(t.fonts.body), t.text.body).unwrap();
    writeln!(
        o,
        "| caption | {} | {} |",
        family(t.fonts.body),
        t.text.caption
    )
    .unwrap();
    writeln!(o, "| code | {} | {} |", family(t.fonts.mono), t.text.body).unwrap();
    writeln!(
        o,
        "| icon | {} | {} |\n",
        family(t.fonts.icons),
        t.text.icon
    )
    .unwrap();
    writeln!(o, "## Spacing, shape and size\n").unwrap();
    writeln!(o, "| token | px |\n| --- | --- |").unwrap();
    for (k, n) in numbers(t)
        .iter()
        .filter(|(k, _)| !k.starts_with("motion") && !k.starts_with("text"))
    {
        writeln!(o, "| `{k}` | {n} |").unwrap();
    }
    writeln!(
        o,
        "\nA control is at least {} px tall (the smallest touch target). Shadows: {}.\n",
        t.control_height,
        if t.elevation == 0 {
            "none".to_string()
        } else {
            format!("{} px blur under floating panels", t.elevation)
        }
    )
    .unwrap();
    writeln!(o, "## Motion\n").unwrap();
    writeln!(
        o,
        "fast {} ms, normal {} ms, slow {} ms.\n",
        t.motion.fast_ms, t.motion.normal_ms, t.motion.slow_ms
    )
    .unwrap();
    writeln!(o, "## Decoration\n").unwrap();
    let d = t.decor;
    if d.ticks == 0 && d.hatch == 0 && d.dots == 0 && !d.frame {
        writeln!(o, "None: every widget is as the tokens say.\n").unwrap();
    } else {
        writeln!(
            o,
            "| piece | rule | on a glass without greys |\n| --- | --- | --- |"
        )
        .unwrap();
        writeln!(o, "| corner ticks | `{}` px arms, {} px thick, in the four corners of a card and a dialog; the card has a quiet 1 px outline between them | kept (they are lines) |", d.ticks, t.border_width).unwrap();
        writeln!(o, "| hatch | 45 degree lines `{}` px apart in `primary` thinned to 62 % over `surface`, on the chosen list row and tab | a solid `primary` fill with `on-primary` text |", d.hatch).unwrap();
        writeln!(o, "| dotted track | a solid bar to the value, then a 1 px dot every `{}` px in `border-strong` | a solid track |", d.dots).unwrap();
        writeln!(o, "| frame progress | a progress with room on all four sides (taller than twice `track-height`) runs round its edge, clockwise from the top left, `track-height` thick, over a `border` track | kept (it is lines) |\n").unwrap();
    }
    writeln!(o, "## Widgets\n").unwrap();
    writeln!(o, "| widget | look |\n| --- | --- |").unwrap();
    if s.id == "cyberdeck" {
        writeln!(o, "| primary button | `primary` fill, `on-primary` text, no outline; hover `primary-hover`, pressed `primary-active` |").unwrap();
        writeln!(o, "| secondary button | `surface` fill, `border-strong` outline, `text`; hover `surface-raised`, pressed `border` |").unwrap();
        writeln!(o, "| list item, tab | chosen: `surface` fill with the hatch (tab also a `primary` bar under it); idle: no fill |").unwrap();
        writeln!(
            o,
            "| dialog, menu | `surface`, `border-strong` outline, `radius/md`, ticks, no shadow |"
        )
        .unwrap();
        writeln!(o, "| toast | `surface-raised`, `primary` outline |").unwrap();
    } else {
        writeln!(o, "| every widget | the tokens above, no hooks |").unwrap();
    }
    writeln!(
        o,
        "\nSource of the look: `crates/fusuma-ui/src/{}`.",
        if s.id == "cyberdeck" {
            "cyberdeck.rs"
        } else {
            "theme.rs"
        }
    )
    .unwrap();
    o
}

/// Every generated file, as (path under the repository root, text).
fn files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for s in systems() {
        out.push((
            format!("design/tokens/{}/{}.tokens.json", s.id, s.id),
            tokens(&s.theme, &s.palette.dark),
        ));
        out.push((
            format!("design/tokens/{}/{}-light.tokens.json", s.id, s.id),
            tokens(&s.theme, &s.palette.light),
        ));
        out.push((format!("design/spec/{}.md", s.id), spec(&s)));
    }
    out
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_design_files_are_what_the_theme_says() {
    let write = std::env::var("FUSUMA_DESIGN_WRITE").is_ok();
    let mut stale = Vec::new();
    for (path, text) in files() {
        let full = root().join(&path);
        if write {
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, &text).unwrap();
        } else if std::fs::read_to_string(&full).ok().as_deref() != Some(text.as_str()) {
            stale.push(path);
        }
    }
    assert!(
        stale.is_empty(),
        "stale design files, run `nu scripts/design.nu tokens`: {stale:?}"
    );
}

#[test]
fn the_tokens_are_valid_and_carry_every_role_and_number() {
    for s in systems() {
        let t = tokens(&s.theme, &s.palette.dark);
        assert_eq!(t.matches("\"$type\":\"color\"").count(), 16, "{}", s.id);
        assert!(t.contains("\"ticks\":{\"$type\":\"number\",\"$value\":"));
        assert_eq!(
            t.matches('{').count(),
            t.matches('}').count(),
            "braces balance"
        );
    }
    // The two modes of a system differ in colour only.
    let c = Theme::CYBERDECK;
    let strip = |s: String| s.split("\"space\"").nth(1).unwrap().to_string();
    assert_eq!(
        strip(tokens(&c, &Palette::CYBERDECK.dark)),
        strip(tokens(&c, &Palette::CYBERDECK.light))
    );
    assert_ne!(
        tokens(&c, &Palette::CYBERDECK.dark),
        tokens(&c, &Palette::CYBERDECK.light)
    );
}
