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
            about: "Near-black, white mono text, one vermilion accent, 1 px outlines, square corners, no shadow; corner ticks, a hatch on the chosen row, a dotted track and a progress bar round the frame.",
            theme: Theme::CYBERDECK,
            palette: Palette::CYBERDECK,
        },
        System {
            id: "fusuma",
            title: "Fusuma",
            about: "Named for the sliding paper door: sumi ink (dark) and washi paper (light), one vermilion accent, small corners, no shadow, thin outlines; a second thin outline inside cards and dialogs like the lattice of a shoji screen, a bar on the chosen list row like a seal, an underline under the chosen tab.",
            theme: Theme::FUSUMA,
            palette: Palette::WASHI,
        },
    ]
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// A colour with its alpha when it is not opaque.
fn hex8(c: Color) -> String {
    if c.a == 255 {
        hex(c)
    } else {
        format!("{}{:02x}", hex(c), c.a)
    }
}

/// The roles other design systems name (Material, dodai-ui) that this one has under another name or makes from the
/// 16 it has: (token name, value). Aliases are the same colour, so a tool that asks for `outline` finds `border-strong`.
fn derived(c: &Colors) -> Vec<(&'static str, Color)> {
    use fusuma_ui::mix;
    vec![
        ("on-surface", c.text),
        ("on-surface-variant", c.text_muted),
        ("outline", c.border_strong),
        ("outline-variant", c.border),
        ("focus-ring", c.primary),
        ("link", c.primary),
        ("selection", c.primary.with_alpha(72)),
        ("scrim", Color::rgba(0, 0, 0, 150)),
        ("surface-container-lowest", c.background),
        ("surface-container-low", c.surface),
        ("surface-container", mix(c.surface, c.surface_raised, 500)),
        ("surface-container-high", c.surface_raised),
        (
            "surface-container-highest",
            mix(c.surface_raised, c.border, 500),
        ),
    ]
}

fn family(f: Font) -> &'static str {
    match f {
        Font::Sans | Font::SansBold => "Inter",
        Font::Mono => "JetBrains Mono",
        Font::Bitmap => "Bitmap",
        Font::Icons => "Lucide",
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
        ("decor/lattice", t.decor.lattice),
        ("decor/bar", t.decor.bar),
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
    for (name, c) in derived(colors) {
        add("color", name, "color", format!("\"{}\"", hex8(c)));
    }
    // What a button and a list row look like in each state, as the theme's own hooks say.
    {
        use fusuma_ui::{ButtonState as St, ButtonVariant as Var};
        let states = [
            ("idle", St::Idle),
            ("hover", St::Hovered),
            ("pressed", St::Pressed),
            ("disabled", St::Disabled),
        ];
        for (vname, v) in [
            ("primary", Var::Primary),
            ("secondary", Var::Secondary),
            ("ghost", Var::Ghost),
        ] {
            for (sname, st) in states {
                let b = t.button(v, st);
                let none = Color::rgba(0, 0, 0, 0);
                add(
                    "button",
                    &format!("{vname}-{sname}-bg"),
                    "color",
                    format!("\"{}\"", hex8(b.background)),
                );
                add(
                    "button",
                    &format!("{vname}-{sname}-border"),
                    "color",
                    format!("\"{}\"", hex8(b.border.unwrap_or(none))),
                );
                add(
                    "button",
                    &format!("{vname}-{sname}-fg"),
                    "color",
                    format!("\"{}\"", hex8(b.text)),
                );
            }
        }
        add(
            "button",
            "radius",
            "number",
            t.button(Var::Primary, St::Idle).radius.to_string(),
        );
        for (label, st, sel) in [
            ("chosen", St::Idle, true),
            ("idle", St::Idle, false),
            ("hover", St::Hovered, false),
            ("pressed", St::Pressed, false),
        ] {
            let i = t.item(st, sel);
            let none = Color::rgba(0, 0, 0, 0);
            add(
                "item",
                &format!("{label}-fill"),
                "color",
                format!("\"{}\"", hex8(i.fill.unwrap_or(none))),
            );
            add(
                "item",
                &format!("{label}-fg"),
                "color",
                format!("\"{}\"", hex8(i.text)),
            );
        }
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

/// The colours of one palette mode as design-token JSON: the 16 roles under `color`, nothing else.
fn palette_tokens(colors: &Colors) -> String {
    let leaves: Vec<String> = roles(colors)
        .iter()
        .map(|(n, c)| format!("\"{n}\":{{\"$type\":\"color\",\"$value\":\"{}\"}}", hex(*c)))
        .collect();
    format!("{{\"color\":{{{}}}}}\n", leaves.join(","))
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
    if d.ticks == 0 && d.hatch == 0 && d.dots == 0 && !d.frame && d.lattice == 0 && d.bar == 0 {
        writeln!(o, "None: every widget is as the tokens say.\n").unwrap();
    } else {
        writeln!(
            o,
            "| piece | rule | on a glass without greys |\n| --- | --- | --- |"
        )
        .unwrap();
        if d.lattice != 0 {
            writeln!(o, "| lattice | a 1 px outline `{}` px inside the edge of a card and a dialog, in `border` mixed 80 % over the fill | kept (it is a line) |", d.lattice).unwrap();
        }
        if d.bar != 0 {
            writeln!(o, "| seal bar | a `{}` px bar in `primary` on the left edge of the chosen list row, inset a sixth of the height top and bottom | kept |", d.bar).unwrap();
        }
        if d.ticks != 0 {
            writeln!(o, "| corner ticks | `{}` px arms, {} px thick, in the four corners of a card and a dialog; the card has a quiet 1 px outline between them | kept (they are lines) |", d.ticks, t.border_width).unwrap();
        }
        if d.hatch != 0 {
            writeln!(o, "| hatch | 45 degree lines `{}` px apart in `primary` thinned to 62 % over `surface`, on the chosen list row and tab | a solid `primary` fill with `on-primary` text |", d.hatch).unwrap();
        }
        if d.dots != 0 {
            writeln!(o, "| dotted track | a solid bar to the value, then a 1 px dot every `{}` px in `border-strong` | a solid track |", d.dots).unwrap();
        }
        if d.frame {
            writeln!(o, "| frame progress | a progress with room on all four sides (taller than twice `track-height`) runs round its edge, clockwise from the top left, `track-height` thick, over a `border` track | kept (it is lines) |").unwrap();
        }
        writeln!(o).unwrap();
    }
    writeln!(o, "## Widgets\n").unwrap();
    writeln!(o, "| widget | look |\n| --- | --- |").unwrap();
    widgets(&mut o, &s.theme, &s.palette.dark);
    writeln!(
        o,
        "\nSource of the look: `crates/fusuma-ui/src/{}`.",
        if s.id == "cyberdeck" {
            "cyberdeck.rs"
        } else {
            "fusuma.rs"
        }
    )
    .unwrap();
    o
}

/// The token a colour is, by its value in this mode (the first role with that colour).
fn role_of(c: Color, colors: &Colors, text: bool) -> String {
    // A text colour is one of the text roles first (`on-primary` may equal `background`).
    let mut all = roles(colors).to_vec();
    if text {
        all.sort_by_key(|(n, _)| !(n.starts_with("text") || n.starts_with("on-")));
    }
    all.iter()
        .find(|(_, v)| *v == c)
        .map_or_else(|| hex(c), |(n, _)| format!("`{n}`"))
}

/// The widget table, made by calling the theme's own hooks, so the spec says what the toolkit paints.
fn widgets(o: &mut String, t: &Theme, colors: &Colors) {
    use fusuma_ui::{ButtonState as St, ButtonVariant as Var, PanelKind};
    let r = |c| role_of(c, colors, false);
    let tx = |c| role_of(c, colors, true);
    let outline = |b: Option<Color>| {
        b.map_or_else(|| "no outline".to_owned(), |c| format!("{} outline", r(c)))
    };
    if t.hooks.button.is_none()
        && t.hooks.item.is_none()
        && t.hooks.tab.is_none()
        && t.hooks.panel.is_none()
    {
        writeln!(o, "| every widget | the tokens above, no hooks |").unwrap();
        return;
    }
    if let Some(h) = t.hooks.button {
        for (name, v) in [
            ("primary", Var::Primary),
            ("secondary", Var::Secondary),
            ("ghost", Var::Ghost),
        ] {
            for (state, st) in [
                ("idle", St::Idle),
                ("hover", St::Hovered),
                ("pressed", St::Pressed),
                ("disabled", St::Disabled),
            ] {
                let b = h(t, 0, v, st);
                writeln!(
                    o,
                    "| {name} button, {state} | {} fill, {}, {} text, radius {} |",
                    r(b.background),
                    outline(b.border),
                    tx(b.text),
                    b.radius
                )
                .unwrap();
            }
        }
    }
    if let Some(h) = t.hooks.item {
        for (sel, st, label) in [
            (true, St::Idle, "chosen"),
            (false, St::Idle, "idle"),
            (false, St::Hovered, "hover"),
            (false, St::Pressed, "pressed"),
        ] {
            let i = h(t, 0, st, sel);
            let fill = i
                .fill
                .map_or_else(|| "no fill".to_owned(), |c| format!("{} fill", r(c)));
            let hatch = if sel && t.decor.hatch != 0 {
                ", hatched"
            } else {
                ""
            };
            writeln!(
                o,
                "| list item, {label} | {fill}{hatch}, {} text |",
                tx(i.text)
            )
            .unwrap();
        }
    }
    if let Some(h) = t.hooks.tab {
        for (sel, st, label) in [
            (true, St::Idle, "chosen"),
            (false, St::Idle, "idle"),
            (false, St::Hovered, "hover"),
        ] {
            let i = h(t, 0, st, sel);
            let fill = i
                .fill
                .map_or_else(|| "no fill".to_owned(), |c| format!("{} fill", r(c)));
            let hatch = if sel && t.decor.hatch != 0 {
                ", hatched"
            } else {
                ""
            };
            let bar = i
                .indicator
                .map_or_else(String::new, |c| format!(", {} bar under it", r(c)));
            writeln!(
                o,
                "| tab, {label} | {fill}{hatch}, {} text{bar} |",
                tx(i.text)
            )
            .unwrap();
        }
    }
    if let Some(h) = t.hooks.panel {
        for (name, k) in [
            ("dialog", PanelKind::Dialog),
            ("menu", PanelKind::Menu),
            ("toast", PanelKind::Toast),
        ] {
            let p = h(t, 0, k);
            let shadow = if p.shadow.is_some() {
                "soft shadow"
            } else {
                "no shadow"
            };
            let ticks = if t.decor.ticks != 0 && k != PanelKind::Toast {
                ", corner ticks"
            } else {
                ""
            };
            writeln!(
                o,
                "| {name} | {} fill, {}, radius {}, {shadow}{ticks} |",
                r(p.fill),
                outline(p.border),
                p.radius
            )
            .unwrap();
        }
    }
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
    // Every palette, colours only: a collection of its own with a dark and a light mode.
    for p in Palette::ALL {
        let n = p.name;
        out.push((
            format!("design/palettes/{n}/{n}.tokens.json"),
            palette_tokens(&p.dark),
        ));
        out.push((
            format!("design/palettes/{n}/{n}-light.tokens.json"),
            palette_tokens(&p.light),
        ));
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
fn every_palette_has_its_colour_tokens_in_both_modes() {
    let all = files();
    for p in Palette::ALL {
        for suffix in [".tokens.json", "-light.tokens.json"] {
            let path = format!("design/palettes/{0}/{0}{suffix}", p.name);
            let text = &all.iter().find(|(f, _)| *f == path).expect(&path).1;
            assert_eq!(text.matches("\"$type\":\"color\"").count(), 16, "{path}");
        }
    }
}

#[test]
fn the_tokens_are_valid_and_carry_every_role_and_number() {
    for s in systems() {
        let t = tokens(&s.theme, &s.palette.dark);
        // 16 roles, the roles under other names, and a fill, an outline and a text colour for each of the 3 buttons in 4
        // states and a fill and a text colour for each of the 4 list rows.
        let colors = 16 + derived(&s.palette.dark).len() + 3 * 4 * 3 + 4 * 2;
        assert_eq!(t.matches("\"$type\":\"color\"").count(), colors, "{}", s.id);
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

/// The fusuma design on the real renderer: the lattice line is `lattice` px inside a card's edge and the seal bar is `bar`
/// px wide on the chosen row, as the tokens say.
#[test]
fn the_fusuma_lattice_and_bar_are_where_the_tokens_say() {
    use fusuma_render::{Probe, Rect, Renderer};
    use fusuma_ui::{Style, Tree};
    let t = Theme::FUSUMA;
    let c = t.colors;
    let d = t.decor;
    let mut tree = Tree::<8>::new();
    let card = tree
        .column(
            None,
            Style::new()
                .padding(12)
                .background(fusuma_ui::Surface::Surface),
        )
        .unwrap();
    tree.label(Some(card), "x", fusuma_ui::TextRole::Body)
        .unwrap();
    let mut s = fusuma_render_vello_cpu::new(120, 80).unwrap();
    tree.update(&s, &t, Rect::new(0, 0, 120, 80));
    s.clear(c.background).unwrap();
    tree.paint(&mut s, &t).unwrap();
    let r = tree.rect(card).unwrap();
    let (mid, inset) = (r.y + (r.h / 2) as i32, d.lattice as i32);
    // Scanning in from the left edge: the outline, then the fill, then the lattice line, then the fill again.
    assert_eq!(s.pixel(r.x, mid), Some(c.border), "the outline");
    let fill = s.pixel(r.x + 1, mid).unwrap();
    assert_eq!(fill, c.surface);
    let line = s.pixel(r.x + inset, mid).unwrap();
    assert_ne!(line, fill, "the lattice line");
    assert_eq!(s.pixel(r.x + inset - 1, mid), Some(fill));
    assert_eq!(s.pixel(r.x + inset + 1, mid), Some(fill), "one pixel wide");

    // The chosen row: a bar `bar` px wide in the main action's colour on its left edge.
    let mut tree = Tree::<8>::new();
    let row = tree.list_item(None, "", true).unwrap();
    let mut s = fusuma_render_vello_cpu::new(160, 60).unwrap();
    tree.update(&s, &t, Rect::new(0, 0, 160, 60));
    s.clear(c.background).unwrap();
    tree.paint(&mut s, &t).unwrap();
    let r = tree.rect(row).unwrap();
    let y = r.y + (r.h / 2) as i32;
    let width = (r.x..r.right())
        .take_while(|&x| s.pixel(x, y) == Some(c.primary))
        .count();
    assert_eq!(width as u32, d.bar);
}

/// The drawing against the numbers the spec and the tokens give: the corner ticks, the hatch and the dotted track are as
/// long and as far apart as `decor` says, on the real renderer.
#[test]
fn the_pixels_are_what_the_spec_says() {
    use fusuma_render::{Probe, Rect, Renderer};
    use fusuma_ui::{Style, Tree};
    let t = Theme::CYBERDECK;
    let c = t.colors;
    let d = t.decor;
    let mut tree = Tree::<8>::new();
    let card = tree
        .column(
            None,
            Style::new()
                .padding(10)
                .background(fusuma_ui::Surface::Surface),
        )
        .unwrap();
    tree.label(Some(card), "x", fusuma_ui::TextRole::Body)
        .unwrap();
    let mut s = fusuma_render_vello_cpu::new(120, 80).unwrap();
    tree.update(&s, &t, Rect::new(0, 0, 120, 80));
    s.clear(c.background).unwrap();
    tree.paint(&mut s, &t).unwrap();
    let r = tree.rect(card).unwrap();
    let arm = d.ticks as i32;
    // The tick is `ticks` px long from each corner, then the quiet outline.
    assert_eq!(s.pixel(r.x + arm - 1, r.y), Some(c.border_strong));
    assert_eq!(s.pixel(r.x + arm, r.y), Some(c.border));
    assert_eq!(s.pixel(r.x, r.y + arm - 1), Some(c.border_strong));
    assert_eq!(s.pixel(r.x, r.y + arm), Some(c.border));

    // The chosen row: the hatch lines `hatch` px apart along a horizontal scan.
    let mut tree = Tree::<8>::new();
    let row = tree.list_item(None, "", true).unwrap();
    let mut s = fusuma_render_vello_cpu::new(160, 60).unwrap();
    tree.update(&s, &t, Rect::new(0, 0, 160, 60));
    s.clear(c.background).unwrap();
    tree.paint(&mut s, &t).unwrap();
    let r = tree.rect(row).unwrap();
    let y = r.y + (r.h / 2) as i32;
    let hatch = d.hatch as i32;
    let hits = |x| s.pixel(x, y).is_some_and(|p| p != c.surface);
    assert!((r.x..r.right()).any(hits), "the chosen row is hatched");
    for x in r.x..r.right() - hatch {
        assert_eq!(s.pixel(x, y), s.pixel(x + hatch, y), "hatch period at {x}");
    }
    let finer =
        (1..hatch).any(|k| (r.x..r.right() - k).all(|x| s.pixel(x, y) == s.pixel(x + k, y)));
    assert!(!finer, "no period shorter than the hatch step");

    // The track after the value is a dot every `dots` px.
    let mut tree = Tree::<8>::new();
    let root = tree
        .column(
            None,
            Style::new()
                .padding(4)
                .align(fusuma_ui::CrossAlign::Stretch),
        )
        .unwrap();
    let bar = tree.progress_bar(Some(root), 250).unwrap();
    let mut s = fusuma_render_vello_cpu::new(160, 30).unwrap();
    tree.update(&s, &t, Rect::new(0, 0, 160, 30));
    s.clear(c.background).unwrap();
    tree.paint(&mut s, &t).unwrap();
    let r = tree.rect(bar).unwrap();
    let dots = d.dots as i32;
    let from = r.x + (r.w / 4) as i32 + 8;
    let y = (r.y..r.bottom())
        .find(|&y| (from..r.right()).any(|x| s.pixel(x, y) == Some(c.border_strong)))
        .expect("a row with dots");
    let on: Vec<i32> = (from..r.right())
        .filter(|&x| s.pixel(x, y) == Some(c.border_strong))
        .collect();
    assert!(on.len() > 10, "{on:?}");
    assert!(
        on.windows(2).all(|w| w[1] - w[0] == dots),
        "dots {dots} px apart: {on:?}"
    );
    // The solid part is the primary colour.
    assert_eq!(
        s.pixel(
            r.x + 4,
            (r.y..r.bottom())
                .find(|&y| s.pixel(r.x + 4, y) == Some(c.primary))
                .unwrap()
        ),
        Some(c.primary)
    );
}
