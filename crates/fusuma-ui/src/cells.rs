//! The widgets as text: a terminal, a serial console and a character panel draw the same tree. Each widget has a
//! cell form (`[x] Wi-Fi`, `(•) Medium`, `███░░░`, `[ OK ]`, a box around a dialog), drawn from the same layout the
//! pixel renderers use, measured in cells ([`CellMeasure`] and [`Theme::CELLS`]) so nothing about layout is forked.
//!
//! What a widget looks like is data: a [`CellTheme`] holds its glyphs (a Unicode set, an ASCII-only one) and the
//! emphasis each state gets (bold, underline, reverse), so a monochrome output still shows hover, focus and the chosen
//! row. Colours come from the pixel theme's palette and are cut down by whatever writes them (see
//! `fusuma_cells::Depth`).

use fusuma_cells::{Attrs, Cell, Color as CColor, Grid, Style, char_width};
use fusuma_render::{Color, Measure, Point, Rect, Size, TextStyle};

use crate::paragraph::Lines;
use crate::theme::{ButtonState, ButtonVariant, TextRole, Theme};
use crate::tree::{Kind, Tree};

/// Measures text in cells: a column for each column of a character, one row for any text.
#[derive(Clone, Copy, Debug, Default)]
pub struct CellMeasure;

impl Measure for CellMeasure {
    fn measure_text(&self, text: &str, _style: &TextStyle) -> Size {
        let w = fusuma_cells::str_width(text);
        let w = u32::try_from(w).unwrap_or(u32::MAX);
        Size::new(w, u32::from(w > 0))
    }
}

/// Where cells are drawn.
pub trait CellSink {
    /// Puts `cell` at column `x`, row `y`; outside the sink it does nothing.
    fn put(&mut self, x: i32, y: i32, cell: Cell);

    /// Dims everything drawn so far (what lies behind a dialog).
    fn dim(&mut self);
}

impl<S: AsRef<[Cell]> + AsMut<[Cell]>> CellSink for Grid<S> {
    fn put(&mut self, x: i32, y: i32, cell: Cell) {
        if let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) {
            self.set(x, y, cell);
        }
    }

    fn dim(&mut self) {
        let (w, h) = (self.width(), self.height());
        for y in 0..h {
            for x in 0..w {
                if let Some(mut c) = self.get(x, y) {
                    c.style = c.style.attrs(Attrs::DIM);
                    self.set(x, y, c);
                }
            }
        }
    }
}

/// The characters a text UI draws widgets with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyphs {
    /// A checked box (3 columns).
    pub check_on: &'static str,
    /// An unchecked box (3 columns).
    pub check_off: &'static str,
    /// A chosen radio button (3 columns).
    pub radio_on: &'static str,
    /// A radio button not chosen (3 columns).
    pub radio_off: &'static str,
    /// A switch that is on (6 columns).
    pub switch_on: &'static str,
    /// A switch that is off (6 columns).
    pub switch_off: &'static str,
    /// The filled part of a progress bar.
    pub bar_full: char,
    /// The empty part of a progress bar.
    pub bar_empty: char,
    /// The part of a slider's track before the thumb.
    pub slider_fill: char,
    /// The part of a slider's track after the thumb.
    pub slider_track: char,
    /// A slider's thumb.
    pub slider_thumb: char,
    /// A scrollbar's thumb.
    pub scroll_thumb: char,
    /// A scrollbar's track.
    pub scroll_track: char,
    /// The corners of a box: top left, top right, bottom left, bottom right.
    pub corners: [char; 4],
    /// The horizontal side of a box.
    pub horizontal: char,
    /// The vertical side of a box.
    pub vertical: char,
    /// In front of the chosen row of a list (2 columns); the others get as many spaces.
    pub marker: &'static str,
    /// The brackets of a button.
    pub button: [char; 2],
    /// The brackets of a button that has the focus.
    pub button_focus: [char; 2],
    /// In place of an icon of the icon font, which a terminal does not have.
    pub icon: char,
}

/// What each state adds to a widget's look, as attributes: they show on an output with no colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emphasis {
    /// The pointer is over it.
    pub hover: Attrs,
    /// It is being pressed.
    pub pressed: Attrs,
    /// The keyboard acts on it.
    pub focus: Attrs,
    /// It is chosen (a list row, a tab) or on.
    pub selected: Attrs,
    /// It cannot be used.
    pub disabled: Attrs,
    /// A heading.
    pub title: Attrs,
    /// A caption.
    pub caption: Attrs,
}

/// How a text UI looks: its glyphs and its emphasis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellTheme {
    /// What it draws with.
    pub glyphs: Glyphs,
    /// What states add.
    pub emphasis: Emphasis,
    /// Whether it uses the palette's colours; false shows everything with attributes alone.
    pub colors: bool,
}

impl CellTheme {
    /// Unicode: box drawing, blocks, bullets.
    pub const UNICODE: Self = Self {
        glyphs: Glyphs {
            check_on: "[x]",
            check_off: "[ ]",
            radio_on: "(•)",
            radio_off: "( )",
            switch_on: "[ ON ]",
            switch_off: "[OFF ]",
            bar_full: '█',
            bar_empty: '░',
            slider_fill: '━',
            slider_track: '─',
            slider_thumb: '●',
            scroll_thumb: '█',
            scroll_track: '│',
            corners: ['┌', '┐', '└', '┘'],
            horizontal: '─',
            vertical: '│',
            marker: "▸ ",
            button: ['[', ']'],
            button_focus: ['❮', '❯'],
            icon: '◆',
        },
        emphasis: Self::EMPHASIS,
        colors: true,
    };

    /// ASCII only: for a terminal or a console that has no box drawing, or a font that lacks it.
    pub const ASCII: Self = Self {
        glyphs: Glyphs {
            check_on: "[x]",
            check_off: "[ ]",
            radio_on: "(*)",
            radio_off: "( )",
            switch_on: "[ ON ]",
            switch_off: "[OFF ]",
            bar_full: '#',
            bar_empty: '.',
            slider_fill: '=',
            slider_track: '-',
            slider_thumb: 'o',
            scroll_thumb: '#',
            scroll_track: '|',
            corners: ['+', '+', '+', '+'],
            horizontal: '-',
            vertical: '|',
            marker: "> ",
            button: ['[', ']'],
            button_focus: ['<', '>'],
            icon: '*',
        },
        emphasis: Self::EMPHASIS,
        colors: true,
    };

    /// Unicode glyphs, no colour: every state shows by attributes.
    pub const MONO: Self = Self {
        colors: false,
        ..Self::UNICODE
    };

    const EMPHASIS: Emphasis = Emphasis {
        hover: Attrs::UNDERLINE,
        pressed: Attrs::REVERSE,
        focus: Attrs::BOLD,
        selected: Attrs::BOLD,
        disabled: Attrs::DIM,
        title: Attrs::BOLD,
        caption: Attrs::DIM,
    };
}

impl Default for CellTheme {
    fn default() -> Self {
        Self::UNICODE
    }
}

fn rgb(c: Color) -> CColor {
    CColor::Rgb(c.r, c.g, c.b)
}

/// Draws into a sink, cut to a rectangle.
struct Pen<'a, S: CellSink> {
    sink: &'a mut S,
    clip: Rect,
}

impl<S: CellSink> Pen<'_, S> {
    fn put(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if self.clip.contains(Point::new(x, y)) {
            self.sink.put(x, y, Cell::new(ch, style));
        }
    }

    /// Writes `text` from column `x` of row `y`; returns the column after it.
    fn text(&mut self, x: i32, y: i32, text: &str, style: Style) -> i32 {
        let mut at = x;
        for ch in text.chars() {
            match char_width(ch) {
                0 => {}
                1 => {
                    self.put(at, y, ch, style);
                    at = at.saturating_add(1);
                }
                _ => {
                    // Both columns or a space: half a wide character is not drawn.
                    if self.clip.contains(Point::new(at, y))
                        && self.clip.contains(Point::new(at.saturating_add(1), y))
                    {
                        self.sink.put(at, y, Cell::new(ch, style));
                        self.sink.put(at.saturating_add(1), y, Cell::tail(style));
                    } else {
                        self.put(at, y, ' ', style);
                    }
                    at = at.saturating_add(2);
                }
            }
        }
        at
    }

    fn fill(&mut self, r: Rect, ch: char, style: Style) {
        let r = r.intersect(&self.clip);
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                self.sink.put(x, y, Cell::new(ch, style));
            }
        }
    }
}

/// The colours and attributes a widget is drawn with, from the theme and the state.
struct Look<'a> {
    theme: &'a Theme,
    cells: &'a CellTheme,
}

impl Look<'_> {
    fn style(&self, fg: Color, bg: Option<Color>, attrs: Attrs) -> Style {
        if self.cells.colors {
            Style {
                fg: rgb(fg),
                bg: bg.map_or(CColor::Default, rgb),
                attrs,
            }
        } else {
            Style {
                fg: CColor::Default,
                bg: CColor::Default,
                attrs,
            }
        }
    }

    fn text(&self, attrs: Attrs) -> Style {
        self.style(self.theme.colors.text, None, attrs)
    }

    fn state_attrs(&self, state: ButtonState, focused: bool) -> Attrs {
        let e = &self.cells.emphasis;
        let mut a = match state {
            ButtonState::Hovered => e.hover,
            ButtonState::Pressed => e.pressed,
            ButtonState::Disabled => e.disabled,
            ButtonState::Idle => Attrs::NONE,
        };
        if focused {
            a = a.with(e.focus);
        }
        a
    }
}

impl<const N: usize> Tree<N> {
    /// Draws the tree as cells into `sink`, laid out in `Theme::CELLS` (call [`update`](Self::update) with a
    /// [`CellMeasure`] first). The page first, then the open dialogs and menus over it (the page dimmed behind a
    /// dialog), then the toasts.
    pub fn paint_cells<S: CellSink>(&self, sink: &mut S, theme: &Theme, cells: &CellTheme) {
        let Some(root) = self.root.map(usize::from) else {
            return;
        };
        let look = Look { theme, cells };
        let screen = self.nodes[root].rect;
        let mut pen = Pen { sink, clip: screen };
        let page = look.style(
            theme.colors.text,
            Some(theme.colors.background),
            Attrs::NONE,
        );
        pen.fill(screen, ' ', page);
        self.cells_node(root, &mut pen, &look);
        // Dialogs and menus, in tree order, on top.
        let mut cur = Some(root);
        while let Some(i) = cur {
            match self.nodes[i].kind {
                Kind::Dialog {
                    open,
                    popover,
                    shown,
                    ..
                } if open || (popover.is_none() && shown > 0) => {
                    if popover.is_none() {
                        pen.sink.dim();
                    }
                    pen.clip = screen;
                    self.cells_dialog(i, &mut pen, &look);
                    cur = self.preorder_next(i, root);
                }
                Kind::Dialog { .. } => cur = self.after_subtree(i, root),
                _ => cur = self.preorder_next(i, root),
            }
        }
        for i in 0..N {
            if self.nodes[i].alive {
                pen.clip = screen;
                self.cells_toast(i, &mut pen, &look);
            }
        }
    }

    fn cells_dialog<S: CellSink>(&self, i: usize, pen: &mut Pen<'_, S>, look: &Look<'_>) {
        let n = &self.nodes[i];
        let g = &look.cells.glyphs;
        let c = &look.theme.colors;
        let fill = look.style(c.text, Some(c.surface_raised), Attrs::NONE);
        pen.fill(n.rect, ' ', fill);
        let edge = look.style(c.border_strong, Some(c.surface_raised), Attrs::NONE);
        let r = n.rect;
        if r.w >= 2 && r.h >= 2 {
            let (x1, y1) = (r.right() - 1, r.bottom() - 1);
            for x in r.x + 1..x1 {
                pen.put(x, r.y, g.horizontal, edge);
                pen.put(x, y1, g.horizontal, edge);
            }
            for y in r.y + 1..y1 {
                pen.put(r.x, y, g.vertical, edge);
                pen.put(x1, y, g.vertical, edge);
            }
            pen.put(r.x, r.y, g.corners[0], edge);
            pen.put(x1, r.y, g.corners[1], edge);
            pen.put(r.x, y1, g.corners[2], edge);
            pen.put(x1, y1, g.corners[3], edge);
        }
        self.cells_children(i, pen, look);
    }

    fn cells_toast<S: CellSink>(&self, i: usize, pen: &mut Pen<'_, S>, look: &Look<'_>) {
        let Kind::Toast { text, phase, .. } = self.nodes[i].kind else {
            return;
        };
        if phase == crate::toast::Phase::Hidden {
            return;
        }
        let c = &look.theme.colors;
        let st = look.style(c.background, Some(c.text), Attrs::BOLD.with(Attrs::REVERSE));
        let r = self.nodes[i].rect;
        pen.fill(r, ' ', st);
        let w = u32::try_from(fusuma_cells::str_width(text.as_str())).unwrap_or(0);
        let at = Point::new(
            r.x + (r.w.saturating_sub(w) / 2) as i32,
            r.y + (r.h.saturating_sub(1) / 2) as i32,
        );
        pen.text(at.x, at.y, text.as_str(), st);
    }

    /// The children of `i` that are in its flow, each drawn.
    fn cells_children<S: CellSink>(&self, i: usize, pen: &mut Pen<'_, S>, look: &Look<'_>) {
        let mut c = self.nodes[i].first.map(usize::from);
        while let Some(ci) = c {
            if !self.nodes[ci].kind.out_of_flow() {
                self.cells_node(ci, pen, look);
            }
            c = self.nodes[ci].next.map(usize::from);
        }
    }

    fn cells_node<S: CellSink>(&self, i: usize, pen: &mut Pen<'_, S>, look: &Look<'_>) {
        let n = &self.nodes[i];
        let rect = n.rect;
        if rect.is_empty() {
            return;
        }
        let g = &look.cells.glyphs;
        let c = &look.theme.colors;
        let state = self.widget_state(i);
        let focused = self.focus_visible && self.focus == Some(self.id_of(i));
        let attrs = look.state_attrs(state, focused);
        // The row a one-line widget sits on.
        let row = rect.y.saturating_add((rect.h.saturating_sub(1) / 2) as i32);
        let disabled = state == ButtonState::Disabled;
        let ink = if disabled { c.text_disabled } else { c.text };
        match n.kind {
            Kind::Stack(_) => {
                if let Some(bg) = look.theme.surface(n.style.background) {
                    pen.fill(rect, ' ', look.style(c.text, Some(bg), Attrs::NONE));
                }
                self.cells_children(i, pen, look);
            }
            Kind::Scroll {
                offset, content, ..
            } => {
                if let Some(bg) = look.theme.surface(n.style.background) {
                    pen.fill(rect, ' ', look.style(c.text, Some(bg), Attrs::NONE));
                }
                let outer = pen.clip;
                pen.clip = outer.intersect(&rect);
                self.cells_children(i, pen, look);
                // The scrollbar: always there when there is more than fits (nothing fades in a text UI).
                let view = rect.inset(n.style.padding).h;
                if content > view && rect.w > 0 && view > 0 {
                    let x = rect.right() - 1;
                    let track = look.style(c.border, None, Attrs::DIM);
                    let thumb = look.style(c.primary, None, Attrs::NONE);
                    let len =
                        ((u64::from(view) * u64::from(view)) / u64::from(content)).max(1) as u32;
                    let len = len.min(view);
                    let span = u64::from(content - view).max(1);
                    let top = (u64::from(view - len) * offset.max(0) as u64 / span) as i32;
                    for k in 0..rect.h as i32 {
                        let in_thumb = k >= top && k < top + len as i32;
                        pen.put(
                            x,
                            rect.y + k,
                            if in_thumb {
                                g.scroll_thumb
                            } else {
                                g.scroll_track
                            },
                            if in_thumb { thumb } else { track },
                        );
                    }
                }
                pen.clip = outer;
            }
            Kind::Spacer | Kind::Dialog { .. } | Kind::Toast { .. } => {}
            Kind::Label { text, role } => {
                let (st, s) = match role {
                    TextRole::Title => (look.text(look.cells.emphasis.title), text.as_str()),
                    TextRole::Caption => (
                        look.style(c.text_muted, None, look.cells.emphasis.caption),
                        text.as_str(),
                    ),
                    _ => (look.text(Attrs::NONE), text.as_str()),
                };
                if role == TextRole::Icon {
                    for (k, ch) in s.chars().enumerate() {
                        let ch = if ('\u{e000}'..='\u{f8ff}').contains(&ch) {
                            g.icon
                        } else {
                            ch
                        };
                        pen.put(rect.x + k as i32, row, ch, st);
                    }
                } else {
                    pen.text(rect.x.saturating_add(n.style.padding as i32), row, s, st);
                }
            }
            Kind::Paragraph { text, role, .. } => {
                let inner = rect.inset(n.style.padding);
                let st = match role {
                    TextRole::Title => look.text(look.cells.emphasis.title),
                    TextRole::Caption => {
                        look.style(c.text_muted, None, look.cells.emphasis.caption)
                    }
                    _ => look.text(Attrs::NONE),
                };
                let text = text.as_str();
                let width = |s: &str| u32::try_from(fusuma_cells::str_width(s)).unwrap_or(u32::MAX);
                for (k, (a, b)) in Lines::new(text, inner.w, width).enumerate() {
                    pen.text(inner.x, inner.y.saturating_add(k as i32), &text[a..b], st);
                }
            }
            Kind::Button {
                text,
                variant,
                icon,
                ..
            } => {
                let (fg, bg) = match (variant, disabled) {
                    (_, true) => (c.text_disabled, c.surface),
                    (ButtonVariant::Primary, _) => (c.on_primary, c.primary),
                    (ButtonVariant::Secondary, _) => (c.text, c.surface_raised),
                };
                let st = look.style(
                    fg,
                    Some(bg),
                    attrs.with(if variant == ButtonVariant::Primary {
                        Attrs::BOLD
                    } else {
                        Attrs::NONE
                    }),
                );
                pen.fill(Rect::new(rect.x, row, rect.w, 1), ' ', st);
                let b = if focused { g.button_focus } else { g.button };
                pen.put(rect.x, row, b[0], st);
                pen.put(rect.right() - 1, row, b[1], st);
                self.cells_icon_text(pen, look, rect, row, icon, text.as_str(), st, true);
            }
            Kind::Checkbox { text, checked } | Kind::Radio { text, checked } => {
                let radio = matches!(n.kind, Kind::Radio { .. });
                let mark = match (radio, checked) {
                    (false, true) => g.check_on,
                    (false, false) => g.check_off,
                    (true, true) => g.radio_on,
                    (true, false) => g.radio_off,
                };
                let on = if checked && !disabled { c.primary } else { ink };
                let mk = look.style(
                    on,
                    None,
                    attrs.with(if checked {
                        look.cells.emphasis.selected
                    } else {
                        Attrs::NONE
                    }),
                );
                let x = pen.text(rect.x, row, mark, mk);
                pen.text(x + 1, row, text.as_str(), look.style(ink, None, attrs));
            }
            Kind::Switch { text, on } => {
                let mark = if on { g.switch_on } else { g.switch_off };
                let colour = if on && !disabled { c.primary } else { ink };
                let mk = look.style(
                    colour,
                    None,
                    attrs.with(if on {
                        look.cells.emphasis.selected
                    } else {
                        Attrs::NONE
                    }),
                );
                let x = pen.text(rect.x, row, mark, mk);
                pen.text(x + 1, row, text.as_str(), look.style(ink, None, attrs));
            }
            Kind::ListItem {
                text,
                selected,
                icon,
            } => {
                let (fg, bg) = if disabled {
                    (c.text_disabled, None)
                } else if selected {
                    (c.on_primary, Some(c.primary))
                } else if state == ButtonState::Pressed {
                    (c.text, Some(c.border))
                } else if state == ButtonState::Hovered {
                    (c.text, Some(c.surface_raised))
                } else {
                    (c.text, None)
                };
                let st = look.style(
                    fg,
                    bg,
                    attrs.with(if selected {
                        look.cells.emphasis.selected
                    } else {
                        Attrs::NONE
                    }),
                );
                pen.fill(Rect::new(rect.x, row, rect.w, 1), ' ', st);
                let marker = if selected { g.marker } else { "  " };
                pen.text(rect.x, row, marker, st);
                self.cells_icon_text(
                    pen,
                    look,
                    Rect::new(rect.x + 2, rect.y, rect.w.saturating_sub(2), rect.h),
                    row,
                    icon,
                    text.as_str(),
                    st,
                    false,
                );
            }
            Kind::Tab {
                text,
                selected,
                icon,
            } => {
                let st = if selected {
                    look.style(
                        if disabled { c.text_disabled } else { c.primary },
                        None,
                        attrs
                            .with(look.cells.emphasis.selected)
                            .with(Attrs::UNDERLINE),
                    )
                } else {
                    look.style(
                        if disabled {
                            c.text_disabled
                        } else {
                            c.text_muted
                        },
                        None,
                        attrs,
                    )
                };
                let inner = Rect::new(rect.x + 1, rect.y, rect.w.saturating_sub(2), rect.h);
                if selected {
                    pen.put(rect.x, row, g.button[0], st);
                    pen.put(rect.right() - 1, row, g.button[1], st);
                }
                self.cells_icon_text(pen, look, inner, row, icon, text.as_str(), st, true);
            }
            Kind::Progress { permille } => {
                let full = (u64::from(rect.w) * u64::from(permille) / 1000) as i32;
                let on = look.style(
                    if disabled { c.border } else { c.primary },
                    None,
                    Attrs::NONE,
                );
                let off = look.style(c.border, None, Attrs::NONE);
                for k in 0..rect.w as i32 {
                    if k < full {
                        pen.put(rect.x + k, row, g.bar_full, on);
                    } else {
                        pen.put(rect.x + k, row, g.bar_empty, off);
                    }
                }
            }
            Kind::Slider { permille } => {
                let span = rect.w.saturating_sub(1);
                let at = (u64::from(span) * u64::from(permille) / 1000) as i32;
                let on = look.style(if disabled { c.border } else { c.primary }, None, attrs);
                let off = look.style(c.border_strong, None, Attrs::NONE);
                for k in 0..rect.w as i32 {
                    let (ch, st) = if k < at {
                        (g.slider_fill, on)
                    } else if k == at {
                        (g.slider_thumb, on)
                    } else {
                        (g.slider_track, off)
                    };
                    pen.put(rect.x + k, row, ch, st);
                }
            }
            Kind::TextInput {
                text,
                cursor,
                anchor,
                placeholder,
            } => {
                let focus = self.focus == Some(self.id_of(i));
                let bg = if disabled {
                    c.surface
                } else {
                    c.surface_raised
                };
                let base = look.style(ink, Some(bg), attrs);
                pen.fill(Rect::new(rect.x, row, rect.w, 1), ' ', base);
                let s = text.as_str();
                let cur = usize::from(cursor).min(s.len());
                let before = u32::try_from(fusuma_cells::str_width(&s[..cur])).unwrap_or(0);
                let inner = rect.w.saturating_sub(2);
                // Scrolled so the cursor is in view.
                let shift = before.saturating_sub(inner.saturating_sub(1)) as i32;
                let origin = rect.x + 1 - shift;
                let outer = pen.clip;
                pen.clip = outer.intersect(&Rect::new(rect.x + 1, row, inner, 1));
                if s.is_empty() && !placeholder.is_empty() {
                    pen.text(
                        origin,
                        row,
                        placeholder,
                        look.style(c.text_muted, Some(bg), Attrs::DIM),
                    );
                } else {
                    pen.text(origin, row, s, base);
                }
                let (lo, hi) = (
                    usize::from(cursor.min(anchor)),
                    usize::from(cursor.max(anchor)),
                );
                if focus && lo != hi {
                    let x0 = origin + fusuma_cells::str_width(&s[..lo]) as i32;
                    let sel = look.style(c.on_primary, Some(c.primary), Attrs::NONE);
                    pen.text(x0, row, &s[lo..hi], sel);
                }
                if focus && !disabled {
                    let x = origin + before as i32;
                    let ch = s[cur..].chars().next().unwrap_or(' ');
                    pen.put(
                        x,
                        row,
                        ch,
                        look.style(c.background, Some(c.text), Attrs::REVERSE),
                    );
                }
                pen.clip = outer;
            }
        }
    }

    /// An icon, then the text, together in the middle of `area` on row `row`.
    #[allow(clippy::too_many_arguments)]
    fn cells_icon_text<S: CellSink>(
        &self,
        pen: &mut Pen<'_, S>,
        look: &Look<'_>,
        area: Rect,
        row: i32,
        icon: Option<char>,
        text: &str,
        st: Style,
        centered: bool,
    ) {
        let g = &look.cells.glyphs;
        let tw = u32::try_from(fusuma_cells::str_width(text)).unwrap_or(0);
        let iw = u32::from(icon.is_some());
        let gap = u32::from(icon.is_some() && tw > 0);
        let total = iw + gap + tw;
        let mut x = if centered {
            area.x + (area.w.saturating_sub(total) / 2) as i32
        } else {
            area.x
        };
        if let Some(ch) = icon {
            let ch = if ('\u{e000}'..='\u{f8ff}').contains(&ch) {
                g.icon
            } else {
                ch
            };
            pen.put(x, row, ch, st);
            x += 1 + gap as i32;
        }
        pen.text(x, row, text, st);
    }
}

#[cfg(test)]
mod tests {
    use std::string::String;
    use std::vec::Vec;

    use fusuma_cells::{Cell, Grid};

    use super::*;
    use crate::theme::ButtonVariant;
    use crate::tree::{CrossAlign, Style as LayoutStyle};

    fn grid(w: u16, h: u16) -> Grid<Vec<Cell>> {
        Grid::new(
            w,
            h,
            std::vec![Cell::BLANK; usize::from(w) * usize::from(h)],
        )
        .unwrap()
    }

    #[test]
    fn text_measures_in_columns_and_one_row() {
        let m = CellMeasure;
        let s = TextStyle::new(1, Color::BLACK);
        assert_eq!(Measure::measure_text(&m, "Wi-Fi", &s), Size::new(5, 1));
        assert_eq!(
            Measure::measure_text(&m, "日本", &s),
            Size::new(4, 1),
            "wide characters are two"
        );
        assert_eq!(Measure::measure_text(&m, "", &s), Size::new(0, 0));
    }

    /// Every widget in every state a text UI shows, in a 44 x 36 screen.
    fn gallery() -> Tree<64> {
        use crate::tree::Length;
        let mut t = Tree::<64>::new();
        let root = t
            .column(None, LayoutStyle::new().align(CrossAlign::Start).gap(0))
            .unwrap();
        t.label(Some(root), "Gallery", TextRole::Title).unwrap();
        t.label(Some(root), "a caption", TextRole::Caption).unwrap();
        t.label(Some(root), "wide: 日本語 and é", TextRole::Body)
            .unwrap();
        t.icon(Some(root), '\u{e88a}').unwrap();
        let row = t.row(Some(root), LayoutStyle::new().gap(1)).unwrap();
        t.button(Some(row), "Save", ButtonVariant::Primary).unwrap();
        t.button(Some(row), "Cancel", ButtonVariant::Secondary)
            .unwrap();
        let off = t
            .button(Some(row), "Off", ButtonVariant::Secondary)
            .unwrap();
        t.set_enabled(off, false);
        t.button_with_icon(Some(root), '\u{e8b8}', "Settings", ButtonVariant::Secondary)
            .unwrap();
        t.checkbox(Some(root), "Notify", true).unwrap();
        t.checkbox(Some(root), "Track", false).unwrap();
        let off = t.checkbox(Some(root), "Locked", true).unwrap();
        t.set_enabled(off, false);
        t.radio(Some(root), "Small", false).unwrap();
        t.radio(Some(root), "Large", true).unwrap();
        t.switch(Some(root), "Wi-Fi", true).unwrap();
        t.switch(Some(root), "Bluetooth", false).unwrap();
        let bar = t.progress_bar(Some(root), 350).unwrap();
        t.set_style(bar, LayoutStyle::new().width(Length::Fixed(20)));
        let slider = t.slider(Some(root), 700).unwrap();
        t.set_style(slider, LayoutStyle::new().width(Length::Fixed(20)));
        let field = t.text_input(Some(root), "Ana Maria").unwrap();
        let empty = t.text_input(Some(root), "").unwrap();
        t.set_placeholder(empty, "Search");
        let tabs = t.tab_bar(Some(root), LayoutStyle::new()).unwrap();
        t.tab(Some(tabs), None, "Home", true).unwrap();
        t.tab(Some(tabs), Some('s'), "Find", false).unwrap();
        let list = t
            .scroll(
                Some(root),
                LayoutStyle::new()
                    .height(Length::Fixed(3))
                    .width(Length::Fixed(16))
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        for (k, name) in ["Aurora", "Birch", "Cedar", "Dune", "Ember"]
            .iter()
            .enumerate()
        {
            t.list_item(Some(list), name, k == 1).unwrap();
        }
        t.paragraph(
            Some(root),
            "A paragraph wraps to the room it has.",
            TextRole::Body,
        )
        .unwrap();
        let p = t.children(root).last().unwrap();
        t.set_style(p, LayoutStyle::new().width(Length::Fixed(18)));
        let _ = (field, empty);
        t
    }

    /// Characters, then for each row that has any a line of attributes: B bold, D dim, U underline, R reverse.
    fn snapshot(g: &Grid<Vec<Cell>>) -> String {
        let mut out = String::new();
        for y in 0..g.height() {
            let cells = g.row(y).unwrap();
            let line: String = cells
                .iter()
                .filter(|c| !c.is_tail())
                .map(|c| c.ch)
                .collect();
            out.push_str(line.trim_end());
            out.push('\n');
            let marks: String = cells
                .iter()
                .filter(|c| !c.is_tail())
                .map(|c| {
                    let a = c.style.attrs;
                    if a.has(Attrs::REVERSE) {
                        'R'
                    } else if a.has(Attrs::UNDERLINE) {
                        'U'
                    } else if a.has(Attrs::BOLD) {
                        'B'
                    } else if a.has(Attrs::DIM) {
                        'D'
                    } else {
                        '.'
                    }
                })
                .collect();
            let marks = marks.trim_end_matches('.');
            if !marks.is_empty() {
                out.push_str("  ");
                out.push_str(marks);
                out.push('\n');
            }
        }
        out
    }

    /// The golden file `name`: its text, or an error telling how to make it.
    fn golden(name: &str, got: &str) {
        let path = std::format!("{}/tests/golden/{name}.txt", env!("CARGO_MANIFEST_DIR"));
        if std::env::var("FUSUMA_UPDATE_GOLDEN").is_ok() {
            std::fs::create_dir_all(std::format!("{}/tests/golden", env!("CARGO_MANIFEST_DIR")))
                .unwrap();
            std::fs::write(&path, got).unwrap();
            return;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{path} is missing: run the tests with FUSUMA_UPDATE_GOLDEN=1")
        });
        assert!(
            want == got,
            "{name} changed (FUSUMA_UPDATE_GOLDEN=1 rewrites it):\n--- golden\n{want}\n--- now\n{got}"
        );
    }

    /// The gallery drawn in `cells`, with the states a pointer and a keyboard add.
    fn gallery_snapshot(cells: &CellTheme) -> String {
        let mut t = gallery();
        let theme = Theme::CELLS;
        let bounds = Rect::new(0, 0, 44, 34);
        t.update(&CellMeasure, &theme, bounds);
        let ids: Vec<_> = t.children(t.root_id().unwrap()).collect();
        // Hover the Cancel button, press nothing, focus the field with a selection.
        let row = t.children(ids[4]).collect::<Vec<_>>();
        let r = t.rect(row[1]).unwrap();
        t.pointer(crate::Pointer::Move(Point::new(r.x + 2, r.y)));
        let field = ids
            .iter()
            .copied()
            .find(|&i| {
                t.semantic(i)
                    .is_some_and(|s| s.role == crate::Role::TextField)
            })
            .unwrap();
        t.focus(field);
        t.key(crate::Key::Next);
        t.key(crate::Key::Prev);
        t.edit(crate::Edit::SelectAll);
        t.update(&CellMeasure, &theme, bounds);
        let mut g = grid(44, 34);
        t.paint_cells(&mut g, &theme, cells);
        snapshot(&g)
    }

    #[test]
    fn every_widget_in_each_cell_theme_is_the_text_the_golden_file_holds() {
        golden("gallery-unicode", &gallery_snapshot(&CellTheme::UNICODE));
        golden("gallery-ascii", &gallery_snapshot(&CellTheme::ASCII));
        golden("gallery-mono", &gallery_snapshot(&CellTheme::MONO));
    }

    #[test]
    fn the_ascii_theme_is_ascii_and_mono_has_no_colour() {
        let ascii = gallery_snapshot(&CellTheme::ASCII);
        for line in ascii.lines() {
            // The gallery's own text has a wide word and an accent; everything a widget draws is ASCII.
            if line.contains("wide:") {
                continue;
            }
            assert!(line.is_ascii(), "{line:?}");
        }
        let mut t = gallery();
        t.update(&CellMeasure, &Theme::CELLS, Rect::new(0, 0, 44, 34));
        let mut g = grid(44, 34);
        t.paint_cells(&mut g, &Theme::CELLS, &CellTheme::MONO);
        assert!(
            g.cells()
                .iter()
                .all(|c| c.style.fg == CColor::Default && c.style.bg == CColor::Default)
        );
        let mut g = grid(44, 34);
        t.paint_cells(&mut g, &Theme::CELLS, &CellTheme::UNICODE);
        assert!(
            g.cells()
                .iter()
                .any(|c| matches!(c.style.fg, CColor::Rgb(..))),
            "colour is there"
        );
    }

    /// A page with a dialog open over it, a menu and a toast.
    fn overlays_snapshot(cells: &CellTheme) -> String {
        let mut t = Tree::<40>::new();
        let theme = Theme::CELLS;
        let root = t
            .column(None, LayoutStyle::new().align(CrossAlign::Start))
            .unwrap();
        t.label(Some(root), "Page behind", TextRole::Title).unwrap();
        let menu_button = t
            .button(Some(root), "Menu", ButtonVariant::Secondary)
            .unwrap();
        t.checkbox(Some(root), "Something", true).unwrap();
        let menu = t.menu(Some(root), LayoutStyle::new()).unwrap();
        for name in ["Copy", "Paste"] {
            t.list_item(Some(menu), name, false).unwrap();
        }
        let dialog = t
            .dialog(Some(root), LayoutStyle::new().padding(1).gap(1))
            .unwrap();
        t.label(Some(dialog), "Delete item?", TextRole::Title)
            .unwrap();
        let row = t.row(Some(dialog), LayoutStyle::new().gap(1)).unwrap();
        t.button(Some(row), "No", ButtonVariant::Secondary).unwrap();
        t.button(Some(row), "Yes", ButtonVariant::Primary).unwrap();
        let toast = t.toast(Some(root)).unwrap();
        let bounds = Rect::new(0, 0, 36, 14);
        t.update(&CellMeasure, &theme, bounds);
        t.open_menu(menu, menu_button);
        t.show_toast(toast, "Saved", 1000);
        t.advance(&theme, 100);
        t.update(&CellMeasure, &theme, bounds);
        let mut g = grid(36, 14);
        t.paint_cells(&mut g, &theme, cells);
        let with_menu = snapshot(&g);
        t.close_menu(menu);
        t.open_dialog(dialog);
        t.advance(&theme, 100);
        t.update(&CellMeasure, &theme, bounds);
        let mut g = grid(36, 14);
        t.paint_cells(&mut g, &theme, cells);
        std::format!("{with_menu}---\n{}", snapshot(&g))
    }

    #[test]
    fn menus_dialogs_and_toasts_are_boxes_over_a_dimmed_page() {
        golden("overlays-unicode", &overlays_snapshot(&CellTheme::UNICODE));
        golden("overlays-ascii", &overlays_snapshot(&CellTheme::ASCII));
    }
}
