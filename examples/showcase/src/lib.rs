//! A showcase of the Fusuma components: one small app that uses every widget there is.
//!
//! It is the same program everywhere. It owns a [`Tree`] and reacts to [`Pointer`] and [`Key`] events and to
//! the passing of time; a host (a test, a window, a board) feeds those in and calls [`Showcase::frame`] to
//! have what changed drawn on any [`Renderer`]. `no_std`, no allocator.
#![no_std]

mod pages;

use core::fmt::{self, Write};

use fusuma::render::{Rect, Renderer};
use fusuma::{
    ActionKind, App, ButtonVariant, CrossAlign, Key, Length, NodeId, Palette, Pointer, Round,
    Style, Surface, TextRole, Theme, Tree,
};

/// Nodes the tree can hold; the busiest page uses about 40.
pub const NODES: usize = 256;

/// How long the progress bar takes to fill, in milliseconds.
const FILL_MS: u32 = 8000;

/// The theme the showcase uses: `palette`'s dark or light colours, sized for a 7" 1024 x 600 touch screen (bigger
/// controls and spacing than the phone-sized defaults).
#[must_use]
pub const fn theme(dark: bool, palette: &Palette) -> Theme {
    let base = if dark { Theme::DARK } else { Theme::LIGHT }.with_colors(palette.colors(dark));
    Theme {
        spacing: fusuma::Spacing {
            xs: 4,
            sm: 10,
            md: 18,
            lg: 28,
        },
        radii: fusuma::Radii {
            sm: 6,
            md: 12,
            lg: 20,
        },
        text: fusuma::TextSizes {
            caption: 15,
            body: 20,
            title: 30,
            icon: 24,
        },
        control_height: 56,
        indicator: 28,
        track_height: 14,
        ..base
    }
}

/// `v` pixels at the display scale `percent`.
const fn scaled(v: u32, percent: u32) -> u32 {
    let r = v as u64 * percent as u64 / 100;
    if r > u32::MAX as u64 {
        u32::MAX
    } else {
        r as u32
    }
}

pub use fusuma::{Class, PHONE_BELOW, TABLET_BELOW, WATCH_BELOW};

/// How many logical pixels a column of a text UI counts for, when it picks a layout by width.
const CELL_PX: u32 = 9;

/// The icons the Typography page shows, by name.
#[cfg(feature = "fonts")]
const ICONS: [&str; 8] = [
    "home",
    "settings",
    "wifi",
    "bluetooth",
    "palette",
    "battery_full",
    "notifications",
    "ev_station",
];

/// The text of the palette button: the palette's name.
fn palette_button_text(palette: &Palette) -> &'static str {
    match palette.name {
        "fusuma" => "Fusuma",
        "forest" => "Forest",
        "ocean" => "Ocean",
        "petrol" => "Petrol",
        "violet" => "Violet",
        "ember" => "Ember",
        "graphite" => "Graphite",
        _ => "Colours",
    }
}

/// The text of the theme dropdown's button: the palette and the mode that are on (a watch has the mode only: its
/// palette is a tile of the home screen).
fn theme_button_text(palette: &Palette, dark: bool, watch: bool) -> Buf {
    let mut b = Buf::new();
    let mode = if dark { "Dark" } else { "Light" };
    if watch {
        let _ = write!(b, "{mode}");
    } else {
        let _ = write!(b, "{} \u{b7} {mode}", palette_button_text(palette));
    }
    b
}

/// The names of a palette's row in the palette dropdown.
const PALETTE_NAMES: [&str; 7] = [
    "Fusuma", "Forest", "Ocean", "Petrol", "Violet", "Ember", "Graphite",
];

/// The pages in the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Cards, badges, an avatar, a stepper and progress: the look of the toolkit at a glance.
    Overview,
    /// Buttons of both looks, enabled and disabled.
    Buttons,
    /// Checkboxes and radio buttons.
    Selection,
    /// A scrolling list to choose from.
    List,
    /// A progress bar that runs.
    Progress,
    /// A switch and a slider.
    Controls,
    /// A text field.
    Text,
    /// The bundled fonts.
    Typography,
    /// Tabs, a menu, a toast and gestures.
    Navigation,
    /// Wrapped text in a scrolling view.
    Reading,
    /// Alerts, tags, a segmented control, chips, breadcrumbs, pagination, steps, a stat card, a snackbar.
    Components,
    /// Charts, a table, a timeline, a carousel, a rating, a spinner and key caps.
    Data,
    /// A calendar, a time picker, a select, a tree, a fold and a split view.
    Pickers,
}

impl Page {
    /// Every page, in sidebar order.
    pub const ALL: [Self; 13] = [
        Self::Overview,
        Self::Buttons,
        Self::Selection,
        Self::List,
        Self::Progress,
        Self::Controls,
        Self::Text,
        Self::Typography,
        Self::Navigation,
        Self::Reading,
        Self::Components,
        Self::Data,
        Self::Pickers,
    ];

    /// The name in the sidebar.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Buttons => "Buttons",
            Self::Selection => "Selection",
            Self::List => "List",
            Self::Progress => "Progress",
            Self::Controls => "Controls",
            Self::Text => "Text",
            Self::Typography => "Typography",
            Self::Navigation => "Navigation",
            Self::Reading => "Reading",
            Self::Components => "Components",
            Self::Data => "Data",
            Self::Pickers => "Pickers",
        }
    }
}

/// A fixed-size text buffer to format numbers into without an allocator.
struct Buf {
    bytes: [u8; 40],
    len: usize,
}

impl Buf {
    const fn new() -> Self {
        Self {
            bytes: [0; 40],
            len: 0,
        }
    }

    fn text(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for Buf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len + s.len();
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

/// The widgets of the page on show that the app talks to.
#[derive(Clone, Copy, Debug, Default)]
struct Live {
    click_me: Option<NodeId>,
    checks: [Option<NodeId>; 3],
    radios: [Option<NodeId>; 3],
    items: [Option<NodeId>; 12],
    list: Option<NodeId>,
    bar: Option<NodeId>,
    start: Option<NodeId>,
    reset: Option<NodeId>,
    switch: Option<NodeId>,
    slider: Option<NodeId>,
    field: Option<NodeId>,
    delete: Option<NodeId>,
    tabs: [Option<NodeId>; 3],
    menu_button: Option<NodeId>,
    menu: Option<NodeId>,
    menu_items: [Option<NodeId>; 3],
    toast: Option<NodeId>,
    card: Option<NodeId>,
    reading: [Option<NodeId>; 2],
}

/// The showcase app.
#[derive(Debug)]
pub struct Showcase {
    tree: Tree<NODES>,
    theme: Theme,
    dark: bool,
    /// The colours: the settings' palette at start, then whatever the palette button stepped to.
    palette: Palette,
    /// Display scale in percent; the theme and the fixed sizes follow it.
    scale: u32,
    bounds: Rect,
    page: Page,
    sidebar: [Option<NodeId>; 13],
    /// Compact layout only: the Previous button, the page name and the Next button.
    nav: [Option<NodeId>; 3],
    /// The class of screen the tree was built for.
    class: Class,
    /// The confirmation dialog, its Cancel button and its Delete button.
    dialog: [Option<NodeId>; 3],
    /// How far each edge of the screen is cut off by its shape (rounded corners), in physical pixels.
    insets: fusuma::Insets,
    /// The shape of the screen, as the board said.
    shape: fusuma::Shape,
    /// What the device is, as the platform told us (a plain screen until then).
    device: fusuma::Screen,
    /// Watch only: a page is open (otherwise the tile grid shows).
    open: bool,
    /// Watch only: the tile of each page, and the back button.
    tiles: [Option<NodeId>; 13],
    back: Option<NodeId>,
    body: Option<NodeId>,
    content: Option<NodeId>,
    theme_button: Option<NodeId>,
    palette_button: Option<NodeId>,
    /// The dropdown under the theme button: Dark and Light, and the palettes (a watch's menu has the modes only; its
    /// palette tile steps through them).
    theme_menu: Option<NodeId>,
    theme_rows: [Option<NodeId>; 2],
    palette_rows: [Option<NodeId>; 7],
    status: Option<NodeId>,
    live: Live,
    clicks: u32,
    running: bool,
    fill_ms: u32,
    volume: u16,
    /// What the text field holds, kept when the page is left.
    typed: fusuma::InputText,
    /// The tab that is chosen, and how many swipes the card has seen.
    tab: usize,
    swipes: u32,
    /// The Overview page's stepper value.
    quantity: u32,
    /// The Components page: the segment, the page number, the step and the chips.
    segment: u8,
    page_no: u32,
    step: u8,
    chips: [bool; 3],
    /// What the Data and Pickers pages keep.
    x: pages::Extra,
    /// The Overview page's view, kept in step with its part of the tree.
    pages: fusuma::Host<pages::PageMsg, 160>,
    /// The icon characters, looked up once (a lookup by name is not free on a chip).
    icons: Option<pages::Icons>,
    /// The state the pages show, besides the ones above.
    notify: bool,
    location: bool,
    size: u8,
    chosen: u8,
    wifi: bool,
    menu_open: bool,
    swiped: Option<fusuma::SwipeDir>,
    /// Whether a page that is chosen slides in (off in the tests that take pictures).
    slide: bool,
    /// A text UI: every length is in character cells (see [`Showcase::new_cells`]).
    cells: bool,
}

impl Showcase {
    /// The app for a screen of `width` x `height` pixels, on the Buttons page, dark.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let mut s = Self {
            tree: Tree::new(),
            theme: theme(true, &Palette::FUSUMA),
            scale: 100,
            dark: true,
            palette: Palette::FUSUMA,
            bounds: Rect::new(0, 0, width, height),
            page: Page::Overview,
            sidebar: [None; 13],
            nav: [None; 3],
            class: Class::of(width),
            dialog: [None; 3],
            insets: fusuma::Insets::all(0),
            shape: fusuma::Shape::Rect,
            device: fusuma::Screen::new(0, 0),
            open: false,
            tiles: [None; 13],
            back: None,
            body: None,
            content: None,
            theme_button: None,
            palette_button: None,
            theme_menu: None,
            theme_rows: [None; 2],
            palette_rows: [None; 7],
            status: None,
            live: Live::default(),
            clicks: 0,
            running: false,
            fill_ms: 0,
            volume: 500,
            typed: fusuma::InputText::new("Hello"),
            tab: 0,
            swipes: 0,
            quantity: 2,
            segment: 0,
            page_no: 3,
            step: 1,
            chips: [true, false, false],
            x: pages::Extra::START,
            pages: fusuma::Host::new(),
            icons: None,
            notify: true,
            location: false,
            size: 1,
            chosen: 0,
            wifi: true,
            menu_open: false,
            swiped: None,
            slide: true,
            cells: false,
        };
        // The tree has room for all of it; a build failure would leave a smaller screen, not a crash.
        let _ = s.build();
        s
    }

    /// The app for a text UI of `cols` x `rows` character cells: the same widgets and pages, laid out in cells
    /// (`Theme::CELLS`) and drawn with [`frame_cells`](Self::frame_cells).
    #[must_use]
    pub fn new_cells(cols: u32, rows: u32) -> Self {
        let mut s = Self::new(1, 1);
        s.cells = true;
        s.bounds = Rect::new(0, 0, cols, rows);
        s.class = Class::of(cols.saturating_mul(CELL_PX));
        s.theme = s.make_theme();
        s.tree.clear();
        s.sidebar = [None; 13];
        s.nav = [None; 3];
        s.tiles = [None; 13];
        s.back = None;
        s.content = None;
        let _ = s.build();
        s
    }

    /// Lays out in cells and draws the page, dialogs and toasts into `sink` (a text UI's frame). Call
    /// [`App::tick`] first for the time that passed.
    pub fn frame_cells<S: fusuma::CellSink>(&mut self, sink: &mut S, cells: &fusuma::CellTheme) {
        self.handle_actions();
        self.tree
            .update(&fusuma::CellMeasure, &self.theme, self.bounds);
        self.tree.paint_cells(sink, &self.theme, cells);
    }

    /// The page on show.
    #[must_use]
    pub const fn page(&self) -> Page {
        self.page
    }

    /// Whether the dark theme is on.
    #[must_use]
    pub const fn is_dark(&self) -> bool {
        self.dark
    }

    /// The theme in use.
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The widget tree, for a host that wants to look at it (a test, a debugger).
    #[must_use]
    pub const fn tree(&self) -> &Tree<NODES> {
        &self.tree
    }

    /// The bounds of a node, for a host that taps by position.
    #[must_use]
    pub fn rect_of(&self, id: NodeId) -> Option<Rect> {
        self.tree.rect(id)
    }

    /// The sidebar entry of `page`.
    #[must_use]
    pub fn sidebar_item(&self, page: Page) -> Option<NodeId> {
        self.sidebar[Page::ALL.iter().position(|&p| p == page)?]
    }

    /// The dialog, its Cancel button and its Delete button.
    #[must_use]
    pub const fn dialog_nodes(&self) -> [Option<NodeId>; 3] {
        self.dialog
    }

    /// The Previous and Next buttons of the compact layout; `None` in the wide one.
    #[must_use]
    pub const fn nav_buttons(&self) -> Option<(NodeId, NodeId)> {
        match (self.nav[0], self.nav[2]) {
            (Some(p), Some(n)) => Some((p, n)),
            _ => None,
        }
    }

    /// Whether the compact layout (a watch or a phone) is on.
    #[must_use]
    pub const fn is_compact(&self) -> bool {
        self.class.is_compact()
    }

    /// The class of screen the layout was built for.
    #[must_use]
    pub const fn class(&self) -> Class {
        self.class
    }

    /// Watch: the tile that opens `page`; `None` when the grid is not showing.
    #[must_use]
    pub fn tile(&self, page: Page) -> Option<NodeId> {
        self.tiles[Page::ALL.iter().position(|&p| p == page)?]
    }

    /// Watch: the back button of an open page.
    #[must_use]
    pub const fn back_button(&self) -> Option<NodeId> {
        self.back
    }

    /// Watch: whether a page is open (else the tile grid shows).
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// The button that opens the theme dropdown.
    #[must_use]
    pub const fn theme_button(&self) -> Option<NodeId> {
        self.theme_button
    }

    /// A row of the theme dropdown: 0 is Dark, 1 is Light.
    #[must_use]
    pub fn theme_row(&self, i: usize) -> Option<NodeId> {
        self.theme_rows.get(i).copied().flatten()
    }

    /// The button that opens the palettes (a watch's tile steps through them).
    #[must_use]
    pub const fn palette_button(&self) -> Option<NodeId> {
        self.palette_button
    }

    /// The colours in use.
    #[must_use]
    pub const fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The widgets of the page on show, by role: for tests and demos that tap them.
    #[must_use]
    pub const fn widgets(&self) -> Widgets {
        Widgets {
            click_me: self.live.click_me,
            checks: self.live.checks,
            radios: self.live.radios,
            items: self.live.items,
            list: self.live.list,
            bar: self.live.bar,
            start: self.live.start,
            reset: self.live.reset,
            delete: self.live.delete,
            switch: self.live.switch,
            slider: self.live.slider,
            field: self.live.field,
            tabs: self.live.tabs,
            menu_button: self.live.menu_button,
            menu: self.live.menu,
            menu_items: self.live.menu_items,
            toast: self.live.toast,
            card: self.live.card,
            reading: self.live.reading,
        }
    }

    /// Whether the progress bar is running.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// How many times the page's button was clicked.
    #[must_use]
    pub const fn clicks(&self) -> u32 {
        self.clicks
    }

    fn build(&mut self) -> Result<(), fusuma::BuildError> {
        self.dialog = [None; 3];
        self.build_screen()?;
        self.build_dialog()
    }

    /// The confirmation dialog: a child of the root, closed until the Delete button opens it.
    fn build_dialog(&mut self) -> Result<(), fusuma::BuildError> {
        let sp = self.theme.spacing;
        let dialog_w = self.wide(300);
        let Some(root) = self.tree.root() else {
            return Ok(());
        };
        let t = &mut self.tree;
        let d = t.dialog(
            Some(root),
            Style::new()
                .width(Length::Fixed(dialog_w))
                .padding(sp.lg)
                .gap(sp.md)
                .align(CrossAlign::Stretch),
        )?;
        t.label(Some(d), "Delete item?", TextRole::Title)?;
        t.label(Some(d), "This cannot be undone.", TextRole::Body)?;
        let row = t.row(Some(d), Style::new().gap(sp.md).align(CrossAlign::Stretch))?;
        let cancel = t.button(Some(row), "Cancel", ButtonVariant::Secondary)?;
        let ok = t.button(Some(row), "Delete", ButtonVariant::Primary)?;
        for b in [cancel, ok] {
            t.set_style(b, Style::new().width(Length::Fill(1)));
        }
        self.dialog = [Some(d), Some(cancel), Some(ok)];
        Ok(())
    }

    fn build_screen(&mut self) -> Result<(), fusuma::BuildError> {
        let side_w = self.wide(240);
        let micro = self.micro();
        let t = &mut self.tree;
        // A tiny panel has no pixels to spend on margins.
        let pad = if micro {
            self.theme.spacing.xs
        } else {
            self.theme.spacing.md
        };
        let content = Style::new()
            .padding(pad)
            .gap(self.theme.spacing.md)
            .align(CrossAlign::Stretch)
            .background(Surface::Background);
        let ins = self.insets;
        // On a screen with rounded corners the page sits inside a frame of the screen's own background, so nothing
        // lands where the corners cut it off. On a rectangle there is no frame.
        let root = if ins.max() == 0 {
            t.column(None, content)?
        } else {
            let frame = t.column(
                None,
                Style::new()
                    .align(CrossAlign::Stretch)
                    .background(Surface::Background),
            )?;
            t.spacer(Some(frame), Style::new().height(Length::Fixed(ins.top)))?;
            let mid = t.row(
                Some(frame),
                Style::new()
                    .height(Length::Fill(1))
                    .align(CrossAlign::Stretch),
            )?;
            t.spacer(Some(mid), Style::new().width(Length::Fixed(ins.left)))?;
            let page = t.column(Some(mid), content.width(Length::Fill(1)))?;
            t.spacer(Some(mid), Style::new().width(Length::Fixed(ins.right)))?;
            t.spacer(Some(frame), Style::new().height(Length::Fixed(ins.bottom)))?;
            page
        };
        let header = t.row(
            Some(root),
            Style::new()
                .gap(self.theme.spacing.md)
                .align(CrossAlign::Center),
        )?;
        let class = self.class;
        let compact = class.is_compact();
        if class == Class::Watch && self.open {
            self.back = Some(t.button(Some(header), "<", ButtonVariant::Secondary)?);
            t.label(
                Some(header),
                self.page.name(),
                if micro {
                    TextRole::Body
                } else {
                    TextRole::Title
                },
            )?;
        } else {
            t.label(
                Some(header),
                if compact {
                    "Fusuma"
                } else {
                    "Fusuma components"
                },
                TextRole::Title,
            )?;
        }
        t.spacer(Some(header), Style::new().width(Length::Fill(1)))?;
        self.palette_rows = [None; 7];
        // The chevron of a dropdown, when the icon font has one.
        #[cfg(feature = "fonts")]
        let chevron = fusuma::fonts::FontSet::default_set().icon("expand_more");
        #[cfg(not(feature = "fonts"))]
        let chevron: Option<char> = None;
        let dropdown = |t: &mut Tree<NODES>, parent, text| match chevron {
            Some(c) => t.button_with_icon(Some(parent), c, text, ButtonVariant::Secondary),
            None => t.button(Some(parent), text, ButtonVariant::Secondary),
        };
        // On a watch the header has no room for the palette: it is a tile of the home screen instead (see below).
        self.palette_button = None;
        let watch = class == Class::Watch;
        self.theme_button = None;
        self.theme_menu = None;
        self.theme_rows = [None; 2];
        let label = theme_button_text(&self.palette, self.dark, watch);
        if !micro {
            self.theme_button = Some(dropdown(t, header, label.text())?);
        }
        // One dropdown for the whole look, a menu out of the flow that opens under its button: the mode (Dark and
        // Light side by side) and, except on a watch whose menu would not fit, every palette.
        let sp = self.theme.spacing;
        if !micro {
            let menu = t.menu(Some(root), Style::new().padding(sp.xs).gap(sp.xs))?;
            let modes = t.row(Some(menu), Style::new().gap(sp.xs))?;
            for (i, name) in ["Dark", "Light"].into_iter().enumerate() {
                let id = t.list_item(Some(modes), name, (i == 0) == self.dark)?;
                t.set_style(id, Style::new().width(Length::Fill(1)));
                self.theme_rows[i] = Some(id);
            }
            if !watch {
                for (i, name) in PALETTE_NAMES.into_iter().enumerate() {
                    self.palette_rows[i] = Some(t.list_item(
                        Some(menu),
                        name,
                        Palette::ALL[i].name == self.palette.name,
                    )?);
                }
            }
            self.theme_menu = Some(menu);
        }
        let body_style = Style::new()
            .height(Length::Fill(1))
            .gap(self.theme.spacing.md)
            .align(CrossAlign::Stretch);
        if class == Class::Watch {
            let body = t.column(Some(root), body_style)?;
            self.body = Some(body);
            if self.open {
                return self.build_page();
            }
            // The home screen: big tiles, two to a row, in a window that scrolls.
            let sp = self.theme.spacing;
            let grid = t.scroll(
                Some(body),
                Style::new()
                    .width(Length::Fill(1))
                    .height(Length::Fill(1))
                    .gap(sp.md)
                    .align(CrossAlign::Stretch),
            )?;
            self.content = Some(grid);
            let tile_h = self.theme.control_height.saturating_mul(3) / 2;
            if micro {
                // A tiny panel: one tile to a row, a list the keys walk through, and no palette (the glass has none).
                for (i, page) in Page::ALL.iter().enumerate() {
                    let id = t.button(Some(grid), page.name(), ButtonVariant::Secondary)?;
                    t.set_style(
                        id,
                        Style::new()
                            .width(Length::Fill(1))
                            .height(Length::Fixed(self.theme.control_height)),
                    );
                    self.tiles[i] = Some(id);
                }
                return Ok(());
            }
            for (r, pair) in Page::ALL.chunks(2).enumerate() {
                let row = t.row(
                    Some(grid),
                    Style::new().gap(sp.md).align(CrossAlign::Stretch),
                )?;
                for (k, page) in pair.iter().enumerate() {
                    let id = t.button(Some(row), page.name(), ButtonVariant::Secondary)?;
                    t.set_style(
                        id,
                        Style::new()
                            .width(Length::Fill(1))
                            .height(Length::Fixed(tile_h)),
                    );
                    self.tiles[r * 2 + k] = Some(id);
                }
                if pair.len() == 1 {
                    // The last row has room for one more: the palette switcher.
                    let id = t.button(
                        Some(row),
                        palette_button_text(&self.palette),
                        ButtonVariant::Primary,
                    )?;
                    t.set_style(
                        id,
                        Style::new()
                            .width(Length::Fill(1))
                            .height(Length::Fixed(tile_h)),
                    );
                    self.palette_button = Some(id);
                }
            }
            if Page::ALL.len().is_multiple_of(2) {
                // An even number of pages leaves no room in the last row: the palette switcher gets one of its own.
                let row = t.row(
                    Some(grid),
                    Style::new().gap(sp.md).align(CrossAlign::Stretch),
                )?;
                let id = t.button(
                    Some(row),
                    palette_button_text(&self.palette),
                    ButtonVariant::Primary,
                )?;
                t.set_style(
                    id,
                    Style::new()
                        .width(Length::Fill(1))
                        .height(Length::Fixed(tile_h)),
                );
                self.palette_button = Some(id);
                // Half a row, like the tiles above it.
                t.spacer(Some(row), Style::new().width(Length::Fill(1)))?;
            }
            return Ok(());
        }
        if compact {
            let body = t.column(Some(root), body_style)?;
            self.body = Some(body);
            let nav = t.row(
                Some(body),
                Style::new()
                    .gap(self.theme.spacing.sm)
                    .align(CrossAlign::Center),
            )?;
            self.nav[0] = Some(t.button(Some(nav), "<", ButtonVariant::Secondary)?);
            self.nav[1] = Some(t.label(Some(nav), self.page.name(), TextRole::Body)?);
            t.spacer(Some(nav), Style::new().width(Length::Fill(1)))?;
            self.nav[2] = Some(t.button(Some(nav), ">", ButtonVariant::Secondary)?);
            self.status = Some(t.label(Some(root), "Tap something", TextRole::Caption)?);
            return self.build_page();
        }
        let body = t.row(Some(root), body_style)?;
        self.body = Some(body);
        let side = t.scroll(
            Some(body),
            Style::new()
                .width(Length::Fixed(side_w))
                .height(Length::Fill(1))
                .padding(self.theme.spacing.sm)
                .gap(self.theme.spacing.xs)
                .align(CrossAlign::Stretch)
                .background(Surface::Surface)
                .round(Round::Medium),
        )?;
        for (slot, page) in self.sidebar.iter_mut().zip(Page::ALL) {
            *slot = Some(t.list_item(Some(side), page.name(), page == self.page)?);
        }
        self.status = Some(t.label(Some(root), "Tap something", TextRole::Caption)?);
        self.build_page()
    }

    /// Replaces the content column with the one of the current page.
    fn build_page(&mut self) -> Result<(), fusuma::BuildError> {
        if let Some(old) = self.content.take() {
            self.tree.remove(old);
        }
        self.live = Live::default();
        let sp = self.theme.spacing;
        let micro = self.micro();
        let page = self.page;
        let body = self.body;
        let permille = self.permille();
        let scale = self.scale;
        let cells = self.cells;
        let class = self.class;
        let compact = class.is_compact();
        // A watch shows a few things at a time: the header names the page, and the extras are left out.
        let lite = class == Class::Watch;
        let t = &mut self.tree;
        let style = Style::new()
            .width(Length::Fill(1))
            .height(Length::Fill(1))
            .padding(if micro { sp.xs } else { sp.md })
            .gap(sp.md)
            .align(CrossAlign::Stretch)
            .background(Surface::Surface)
            .round(Round::Medium);
        // A small screen shows a window onto the page, which scrolls: the pages are taller than a watch.
        let c = if compact || matches!(page, Page::Components | Page::Data | Page::Pickers) {
            t.scroll(body, style)?
        } else {
            t.column(body, style)?
        };
        self.content = Some(c);
        // Every page is a view of the app's state, built into the container above (see `sync_pages`).
        let _ = (
            t, sp, body, permille, scale, cells, class, compact, lite, page,
        );
        self.live = Live::default();
        self.pages.forget();
        self.sync_pages();
        Ok(())
    }

    /// How a page is laid out on this screen.
    fn page_look(&self) -> pages::Look {
        let sp = self.theme.spacing;
        let cells = self.cells;
        let scale = self.scale;
        let wide = |px: u32| {
            if cells {
                px / CELL_PX
            } else {
                scaled(px, scale)
            }
        };
        let tall = |px: u32| if cells { px / 20 } else { scaled(px, scale) };
        let lite = self.class == Class::Watch;
        pages::Look {
            compact: self.class.is_compact(),
            lite,
            micro: self.micro(),
            xs: sp.xs,
            sm: sp.sm,
            md: sp.md,
            lg: sp.lg,
            list_w: wide(360),
            list_h: tall(if lite { 150 } else { 260 }),
            avatar: if cells { 3 } else { scaled(44, scale) },
            reading_h: tall(if lite { 150 } else { 120 }),
        }
    }

    /// The icons the pages use, by name, when the build has the font.
    fn page_icons(&self) -> pages::Icons {
        #[cfg(feature = "fonts")]
        {
            let set = fusuma::fonts::FontSet::default_set();
            let mut row = [None; 9];
            for (slot, name) in row.iter_mut().zip(ICONS) {
                *slot = set.icon(name);
            }
            pages::Icons {
                home: set.icon("home"),
                search: set.icon("search"),
                settings: set.icon("settings"),
                more: set.icon("more_vert"),
                share: set.icon("share"),
                edit: set.icon("edit"),
                delete: set.icon("delete"),
                row,
            }
        }
        #[cfg(not(feature = "fonts"))]
        {
            pages::Icons::default()
        }
    }

    /// Makes the page on show match the state: built the first time, then only what differs is touched.
    fn sync_pages(&mut self) {
        let Some(c) = self.content else { return };
        let look = self.page_look();
        let icons = match self.icons {
            Some(i) => i,
            None => {
                let i = self.page_icons();
                self.icons = Some(i);
                i
            }
        };
        let typed = self.typed;
        let state = pages::State {
            clicks: self.clicks,
            notify: self.notify,
            location: self.location,
            size: self.size,
            chosen: self.chosen,
            wifi: self.wifi,
            volume: self.volume,
            permille: self.permille(),
            running: self.running,
            typed: typed.as_str(),
            tab: u8::try_from(self.tab).unwrap_or(0),
            menu_open: self.menu_open,
            swipes: self.swipes,
            swiped: self.swiped,
            quantity: self.quantity,
            segment: self.segment,
            page_no: self.page_no,
            step: self.step,
            chips: self.chips,
            x: self.x,
            icons,
        };
        // The tree has room for the page (the whole app is checked to fit), so a failure leaves a smaller page.
        let (tree, host) = (&mut self.tree, &mut self.pages);
        let _ = match self.page {
            Page::Overview => host.sync_children(tree, c, &pages::overview(look, state)),
            Page::Buttons => host.sync_children(tree, c, &pages::buttons(look, state)),
            Page::Selection => host.sync_children(tree, c, &pages::selection(look, state)),
            Page::List => host.sync_children(tree, c, &pages::list(look, state)),
            Page::Progress => host.sync_children(tree, c, &pages::progress_page(look, state)),
            Page::Controls => host.sync_children(tree, c, &pages::controls(look, state)),
            Page::Text => host.sync_children(tree, c, &pages::text_page(look, state)),
            Page::Typography => host.sync_children(tree, c, &pages::typography(look, state)),
            Page::Navigation => host.sync_children(tree, c, &pages::navigation(look, state)),
            Page::Reading => host.sync_children(tree, c, &pages::reading(look, state)),
            Page::Components => host.sync_children(tree, c, &pages::components(look, state)),
            Page::Data => host.sync_children(tree, c, &pages::data(look, state)),
            Page::Pickers => host.sync_children(tree, c, &pages::pickers(look, state)),
        };
        self.refresh_live();
    }

    /// Finds the widgets of the page by the names the app gave them.
    fn refresh_live(&mut self) {
        use pages::wid;
        let t = &self.tree;
        let id = |name: fusuma::Id| t.find(name);
        let at = |base: fusuma::Id, i: usize| t.find(fusuma::Id::new(base.0 + i as u16));
        self.live = Live {
            click_me: id(wid::CLICK_ME),
            checks: [id(wid::CHECK0), id(wid::CHECK1), id(wid::CHECK2)],
            radios: [at(wid::RADIO0, 0), at(wid::RADIO0, 1), at(wid::RADIO0, 2)],
            items: core::array::from_fn(|i| at(wid::ITEM0, i)),
            list: id(wid::LIST),
            bar: id(wid::BAR),
            start: id(wid::START),
            reset: id(wid::RESET),
            switch: id(wid::SWITCH),
            slider: id(wid::SLIDER),
            field: id(wid::FIELD),
            delete: id(wid::DELETE),
            tabs: [at(wid::TAB0, 0), at(wid::TAB0, 1), at(wid::TAB0, 2)],
            menu_button: id(wid::MENU_BUTTON),
            menu: id(wid::MENU),
            menu_items: [
                at(wid::MENU_ITEM0, 0),
                at(wid::MENU_ITEM0, 1),
                at(wid::MENU_ITEM0, 2),
            ],
            toast: id(wid::TOAST),
            card: id(wid::CARD),
            reading: [id(wid::READING), id(wid::READING_TEXT)],
        };
    }

    /// What a message from a page does: the one place the pages' state changes. `say` is what the status line says.
    fn page_message(&mut self, m: pages::PageMsg, say: &mut Buf) {
        use pages::PageMsg as P;
        let on_off = |on: bool| if on { "on" } else { "off" };
        match m {
            P::Click => {
                self.clicks += 1;
                let _ = write!(say, "Clicked {} times", self.clicks);
            }
            P::AskDelete => {
                if let Some(d) = self.dialog[0] {
                    self.tree.open_dialog(d);
                }
            }
            P::Notify(on) => {
                self.notify = on;
                let _ = write!(say, "Notifications {}", on_off(on));
            }
            P::Location(on) => {
                self.location = on;
                let _ = write!(say, "Location {}", on_off(on));
            }
            P::Size(i) => {
                self.size = i;
                let _ = write!(say, "Size: {}", pages::RADIO_NAMES[usize::from(i).min(2)]);
            }
            P::Pick(i) => {
                self.chosen = i;
                let _ = write!(say, "Chosen: {}", pages::ITEM_NAMES[usize::from(i).min(11)]);
            }
            P::Wifi(on) => {
                self.wifi = on;
                let _ = write!(say, "Wi-Fi {}", on_off(on));
            }
            P::Volume(v) => {
                self.volume = v;
                let _ = write!(say, "Volume: {}%", v / 10);
            }
            P::StartStop => {
                self.running = !self.running;
                if self.running && self.fill_ms >= FILL_MS {
                    self.fill_ms = 0;
                }
            }
            P::Reset => {
                self.running = false;
                self.fill_ms = 0;
            }
            P::Typed => {
                if let Some(f) = self.tree.find(pages::wid::FIELD) {
                    self.typed = fusuma::InputText::new(self.tree.text(f).unwrap_or(""));
                }
            }
            P::Submit => {
                let _ = write!(say, "Submitted: {}", self.typed.as_str());
            }
            P::Tab(i) => {
                self.tab = usize::from(i).min(2);
                let _ = write!(say, "{} tab", pages::TAB_NAMES[self.tab]);
            }
            P::MenuOpen => self.menu_open = true,
            P::MenuChoice(i) => {
                self.menu_open = false;
                let name = pages::MENU_NAMES[usize::from(i).min(2)];
                let _ = write!(say, "{name}");
                let mut m = Buf::new();
                let _ = write!(m, "{name} chosen");
                self.show_toast(m.text());
            }
            P::MenuDismiss => self.menu_open = false,
            P::Segment(i) => {
                self.segment = i.min(2);
                let _ = write!(say, "{}", pages::SEGMENT_NAMES[usize::from(self.segment)]);
            }
            P::PageTo(p) => {
                self.page_no = p.clamp(1, pages::PAGES);
                let _ = write!(say, "Page {}", self.page_no);
            }
            P::Chip(i) => {
                let i = usize::from(i).min(2);
                self.chips[i] ^= true;
                let _ = write!(say, "{} {}", pages::CHIP_NAMES[i], on_off(self.chips[i]));
            }
            P::StepTo(i) => {
                self.step = i.min(2);
                let _ = write!(say, "Step {}", self.step + 1);
            }
            P::Crumb(i) => {
                let _ = write!(say, "Back to {}", pages::CRUMB_NAMES[usize::from(i).min(2)]);
            }
            P::Undo => {
                let _ = write!(say, "Undone");
            }
            P::Split(p) => {
                self.x.split = p.clamp(50, 950);
                let _ = write!(say, "Split {}%", self.x.split / 10);
            }
            P::Day(d) => {
                self.x.day = d;
                let _ = write!(
                    say,
                    "{} {} {}",
                    d,
                    fusuma::MONTHS[usize::from(self.x.month - 1)],
                    self.x.year
                );
            }
            P::MonthBy(by) => {
                (self.x.year, self.x.month) = fusuma::shift_month(self.x.year, self.x.month, by);
                self.x.day = self
                    .x
                    .day
                    .min(fusuma::days_in_month(self.x.year, self.x.month));
                let _ = write!(
                    say,
                    "{} {}",
                    fusuma::MONTHS[usize::from(self.x.month - 1)],
                    self.x.year
                );
            }
            P::TimeStep(part, by) => {
                (self.x.hour, self.x.minute) =
                    fusuma::step_time(self.x.hour, self.x.minute, part, by);
                let _ = write!(say, "{:02}:{:02}", self.x.hour, self.x.minute);
            }
            P::SelectToggle => self.x.size_open = !self.x.size_open,
            P::SelectClose => self.x.size_open = false,
            P::SelectPick(i) => {
                self.x.size = Some(i.min(2));
                self.x.size_open = false;
                let _ = write!(say, "Size {}", pages::SIZES[i.min(2)]);
            }
            P::Rate(n) => {
                self.x.rating = if self.x.rating == n { 0 } else { n };
                let _ = write!(say, "{} of 5", self.x.rating);
            }
            P::Slide(i) => {
                self.x.slide = i.min(pages::SLIDES.len() - 1);
                let _ = write!(say, "Slide {}", self.x.slide + 1);
            }
            P::Fold => {
                self.x.fold = !self.x.fold;
                let _ = write!(say, "{}", if self.x.fold { "Opened" } else { "Closed" });
            }
            P::TreeToggle => {
                self.x.tree_open = !self.x.tree_open;
                if !self.x.tree_open && self.x.tree_row > 0 {
                    self.x.tree_row = 0;
                }
            }
            P::TreeSelect(i) => {
                self.x.tree_row = i;
                let _ = write!(
                    say,
                    "{}",
                    ["src", "lib.rs", "main.rs"][usize::from(i.min(2))]
                );
            }
            P::Swiped(dir) => {
                self.swipes += 1;
                self.swiped = Some(dir);
                let _ = write!(say, "Swiped {dir:?}");
            }
            P::Held => {
                let _ = write!(say, "Long press");
                self.show_toast("Held for half a second");
            }
            P::Fewer => {
                self.quantity = self.quantity.saturating_sub(1);
                let _ = write!(say, "Seats: {}", self.quantity);
            }
            P::More => {
                self.quantity = (self.quantity + 1).min(99);
                let _ = write!(say, "Seats: {}", self.quantity);
            }
        }
        self.sync_pages();
    }

    /// Shows `text` in the Navigation page's toast.
    fn show_toast(&mut self, text: &str) {
        if let Some(t) = self.live.toast {
            self.tree.show_toast(t, text, 1500);
        }
    }

    /// Builds the whole tree again, for a change of screen: the pages' state lives in the app, not in the tree.
    fn rebuild(&mut self) {
        self.tree.clear();
        self.pages.forget();
        self.sidebar = [None; 13];
        self.nav = [None; 3];
        self.tiles = [None; 13];
        self.back = None;
        self.content = None;
        let _ = self.build();
    }

    /// Whether this is a tiny board (a 128 px panel, a Micro form factor): a list and the keys, no palette switcher.
    #[must_use]
    pub fn micro(&self) -> bool {
        // A grid of text cells is counted in cells, not pixels: 40 of them is not a 40 px panel.
        if self.cells {
            return false;
        }
        fusuma::Screen {
            width: self.bounds.w,
            height: self.bounds.h,
            ..self.device
        }
        .with_scale(self.scale)
        .form_factor()
            == fusuma::FormFactor::Micro
    }

    /// What the platform said the device is (a plain screen until it says).
    #[must_use]
    pub const fn device(&self) -> fusuma::Screen {
        self.device
    }

    /// A point on the page that is not on the menu or its button, for a test that dismisses by pressing outside.
    #[cfg(test)]
    fn tab_outside_point(&self) -> fusuma::render::Point {
        fusuma::render::Point::new(self.bounds.w as i32 - 3, self.bounds.h as i32 - 3)
    }

    /// Starts in a given look, for a host that takes it from its address or its settings: `page`, dark or light and a
    /// palette by name (what is not known is left alone), pages not sliding in, and the theme dropdown open.
    pub fn set_start(
        &mut self,
        page: Option<&str>,
        dark: Option<bool>,
        palette: Option<&str>,
        still: bool,
        open_menu: bool,
    ) {
        if still {
            self.slide = false;
        }
        if let Some(p) = palette.and_then(Palette::named) {
            self.set_palette(*p);
        }
        if let Some(d) = dark {
            self.set_dark(d);
        }
        if let Some(p) =
            page.and_then(|n| Page::ALL.iter().find(|p| p.name().eq_ignore_ascii_case(n)))
        {
            self.choose_page(*p);
        }
        if open_menu && let (Some(m), Some(b)) = (self.theme_menu, self.theme_button) {
            self.tree.open_menu(m, b);
        }
    }

    /// Everything is drawn again at the next frame (a new surface, a new theme).
    pub fn invalidate(&mut self) {
        self.tree.invalidate();
    }

    /// Shows `page` (the layout that has a sidebar or a Previous and Next row; on a watch's tile grid it opens the
    /// page). For a host that picks a page by name.
    pub fn choose_page(&mut self, page: Page) {
        if self.class == Class::Watch {
            self.page = page;
            self.open = true;
            self.rebuild();
        } else {
            self.set_page(page);
        }
    }

    /// A width of `px` pixels at the screen's scale, or the columns a text UI gives it.
    fn wide(&self, px: u32) -> u32 {
        if self.cells {
            px / CELL_PX
        } else {
            scaled(px, self.scale)
        }
    }

    fn permille(&self) -> u16 {
        (self.fill_ms.min(FILL_MS) * 1000 / FILL_MS) as u16
    }

    fn say(&mut self, text: &str) {
        if let Some(s) = self.status {
            self.tree.set_text(s, text);
        }
    }

    fn set_page(&mut self, page: Page) {
        if page != self.page {
            self.page = page;
            if let Some(l) = self.nav[1] {
                self.tree.set_text(l, page.name());
            }
            // The sidebar shows the page that is on.
            for (slot, p) in self.sidebar.iter().zip(Page::ALL) {
                if let Some(id) = *slot {
                    self.tree.set_checked(id, p == page);
                }
            }
            let _ = self.build_page();
            // The new page comes in from the right.
            if let Some(c) = self.content.filter(|_| self.slide) {
                let distance = self.wide(48);
                self.tree.slide_in(c, fusuma::Edge::Right, distance);
            }
        }
    }

    /// The theme for the current colours, scale and class: a watch redraws slowly, so it gets half the motion.
    fn make_theme(&self) -> Theme {
        let budget = if self.device.is_reflective() {
            fusuma::MotionBudget::Off
        } else if self.class == Class::Watch {
            fusuma::MotionBudget::Reduced
        } else {
            fusuma::MotionBudget::Full
        };
        if self.cells {
            return Theme::CELLS.with_colors(self.palette.colors(self.dark));
        }
        theme(self.dark, &self.palette)
            .scaled(self.scale)
            .with_motion(budget)
    }

    /// The dropdown button says which palette and mode are on.
    fn refresh_theme_button(&mut self) {
        if let Some(b) = self.theme_button {
            let label = theme_button_text(&self.palette, self.dark, self.class == Class::Watch);
            self.tree.set_text(b, label.text());
        }
    }

    fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
        self.theme = self.make_theme();
        if let Some(b) = self.palette_button {
            self.tree.set_text(b, palette_button_text(&palette));
        }
        self.refresh_theme_button();
        for (row, p) in self.palette_rows.iter().zip(Palette::ALL) {
            if let Some(r) = *row {
                self.tree.set_checked(r, p.name == palette.name);
            }
        }
        self.tree.invalidate();
    }

    fn set_dark(&mut self, dark: bool) {
        self.dark = dark;
        self.theme = self.make_theme();
        for (i, row) in self.theme_rows.iter().enumerate() {
            if let Some(r) = *row {
                self.tree.set_checked(r, (i == 0) == dark);
            }
        }
        self.refresh_theme_button();
        self.tree.invalidate();
    }

    /// Reacts to what the widgets reported since the last call.
    fn handle_actions(&mut self) {
        while let Some(a) = self.tree.pop_action() {
            let id = Some(a.source);
            let mut b = Buf::new();
            // A message from a page's view (see `pages`): the state changes in `page_message` and the view follows.
            if let Some(m) = self.pages.message(&self.tree, a) {
                self.page_message(m, &mut b);
                if !b.text().is_empty() {
                    self.say(b.text());
                }
                continue;
            }
            match a.kind {
                ActionKind::Dismissed => {
                    // A menu closes itself when it is dismissed (the page's view hears about it); only the dialog's
                    // dismissal is ours to answer.
                    if let Some(d) = self.dialog[0].filter(|&d| Some(d) == id) {
                        self.tree.close_dialog(d);
                        let _ = write!(b, "Cancelled");
                    }
                }
                ActionKind::Clicked => {
                    if id.is_some() && (id == self.dialog[1] || id == self.dialog[2]) {
                        if let Some(d) = self.dialog[0] {
                            self.tree.close_dialog(d);
                        }
                        let _ = write!(
                            b,
                            "{}",
                            if id == self.dialog[2] {
                                "Deleted"
                            } else {
                                "Cancelled"
                            }
                        );
                    } else if let Some(i) = self.tiles.iter().position(|&t| t.is_some() && t == id)
                    {
                        self.open = true;
                        self.page = Page::ALL[i];
                        self.rebuild();
                    } else if id.is_some() && id == self.back {
                        self.open = false;
                        self.rebuild();
                    } else if id.is_some() && (id == self.nav[0] || id == self.nav[2]) {
                        let n = Page::ALL.len();
                        let at = Page::ALL.iter().position(|&p| p == self.page).unwrap_or(0);
                        let to = if id == self.nav[2] {
                            (at + 1) % n
                        } else {
                            (at + n - 1) % n
                        };
                        self.set_page(Page::ALL[to]);
                        let _ = write!(b, "Page: {}", Page::ALL[to].name());
                    } else if id.is_some() && id == self.theme_button {
                        if let (Some(m), Some(a)) = (self.theme_menu, self.theme_button) {
                            self.tree.open_menu(m, a);
                        }
                    } else if let Some(i) =
                        self.theme_rows.iter().position(|&r| r.is_some() && r == id)
                    {
                        self.set_dark(i == 0);
                        let _ = write!(b, "Theme: {}", if i == 0 { "Dark" } else { "Light" });
                    } else if let Some(i) = self
                        .palette_rows
                        .iter()
                        .position(|&r| r.is_some() && r == id)
                    {
                        self.set_palette(Palette::ALL[i]);
                        let _ = write!(b, "Colours: {}", self.palette.name);
                    } else if id.is_some() && id == self.palette_button {
                        // The watch's tile steps through the palettes.
                        self.set_palette(*self.palette.next());
                        let _ = write!(b, "Colours: {}", self.palette.name);
                    }
                }
                ActionKind::Selected => {
                    if let Some(i) = self.sidebar.iter().position(|&s| s == id) {
                        self.set_page(Page::ALL[i]);
                        let _ = write!(b, "Page: {}", Page::ALL[i].name());
                    }
                }
                _ => {}
            }
            if !b.text().is_empty() {
                let line = b;
                self.say(line.text());
            }
        }
    }

    /// A pointer event (a finger or a mouse) in screen pixels.
    pub fn pointer(&mut self, event: Pointer) {
        self.tree.pointer(event);
        self.handle_actions();
    }

    /// A key.
    pub fn key(&mut self, key: Key) {
        self.tree.key(key);
        self.handle_actions();
    }

    /// One pass of the page's view over its nodes, with nothing changed: what a benchmark times. Not part of the app.
    #[doc(hidden)]
    pub fn view_pass(&mut self) {
        self.sync_pages();
    }

    /// One pass after a value the page shows changed (a count, a quantity). Not part of the app.
    #[doc(hidden)]
    pub fn view_pass_changed(&mut self) {
        self.clicks = self.clicks.wrapping_add(1);
        self.quantity = (self.quantity + 1) % 100;
        self.sync_pages();
    }

    /// How many nodes the tree holds now. Not part of the app.
    #[doc(hidden)]
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.tree.len()
    }

    /// Whether the Navigation page's toast is on the screen.
    #[must_use]
    pub fn toast_shown(&self) -> bool {
        self.live.toast.is_some_and(|t| self.tree.is_toast_shown(t))
    }

    /// Milliseconds until time has to pass for something that is not moving (a toast that hides, a long press).
    #[must_use]
    pub fn deadline(&self) -> Option<u32> {
        self.tree.deadline()
    }

    /// Time passes by `dt_ms`: animations run and the progress bar fills while it is running. Returns
    /// whether anything wants a frame.
    pub fn tick(&mut self, dt_ms: u32) -> bool {
        if self.running {
            self.fill_ms = self.fill_ms.saturating_add(dt_ms);
            if self.fill_ms >= FILL_MS {
                self.fill_ms = FILL_MS;
                self.running = false;
            }
            // The bar, its label and the button's text follow the state (only the progress page has them).
            if self.page == Page::Progress {
                self.sync_pages();
            }
        }
        let moving = self.tree.advance(&self.theme, dt_ms);
        // What time itself caused (a long press that came due, a slide that finished) is answered now, not at the next
        // input.
        self.handle_actions();
        moving || self.tree.needs_frame()
    }

    /// Lays out the widgets without drawing, so a pointer event right after a change (a new page) finds
    /// them where they will be drawn. [`frame`](Self::frame) does this too.
    pub fn layout<M: fusuma::render::Measure>(&mut self, renderer: &M) {
        self.tree.update(renderer, &self.theme, self.bounds);
    }

    /// Lays out and draws what changed since the last frame, onto `renderer` (whose surface is the whole
    /// screen). Returns the rectangle that was redrawn, or `None` when nothing had changed: the host
    /// only has to send that part to the display.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn frame<R: Renderer>(&mut self, renderer: &mut R) -> Result<Option<Rect>, R::Error> {
        self.tree.update(renderer, &self.theme, self.bounds);
        // Draws only what changed; a list that only scrolled has its pixels moved instead of redrawn.
        self.tree.paint_frame(renderer, &self.theme, self.bounds)
    }
}

impl App for Showcase {
    fn deadline(&self) -> Option<u32> {
        Showcase::deadline(self)
    }

    fn frame_rects(&self) -> &[Rect] {
        self.tree.frame_rects()
    }

    fn pointer(&mut self, event: Pointer) {
        Self::pointer(self, event);
    }

    fn key(&mut self, key: Key) {
        Self::key(self, key);
    }

    fn tick(&mut self, dt_ms: u32) -> bool {
        Self::tick(self, dt_ms)
    }

    fn text(&mut self, text: &str) {
        self.tree.type_text(text);
        self.handle_actions();
    }

    fn edit(&mut self, edit: fusuma::Edit) {
        self.tree.edit(edit);
        self.handle_actions();
    }

    fn preedit(&mut self, text: &str) {
        self.tree.set_preedit(text);
    }

    fn wants_text(&self) -> bool {
        self.tree.wants_text()
    }

    fn text_cursor(&self) -> Option<Rect> {
        self.tree.text_cursor()
    }

    fn selected_text(&self) -> Option<&str> {
        self.tree.selected_text()
    }

    fn palette(&mut self, palette: Palette) {
        self.set_palette(palette);
    }

    fn screen(&mut self, screen: fusuma::Screen) {
        // What the device is: the layout class is the device's (a small board is the watch layout at any scale), and a
        // glass that reflects light has no motion.
        self.device = screen;
        // Black ink on white paper where the glass has too few levels for colour (and no dark mode: ink is dark).
        let palette = screen.palette(self.palette);
        if palette != self.palette {
            self.palette = palette;
            self.dark = false;
        }
        let class = fusuma::Screen {
            width: self.bounds.w,
            height: self.bounds.h,
            ..screen
        }
        .with_scale(self.scale)
        .class();
        if class != self.class {
            self.class = class;
            self.open = false;
        }
        self.theme = self.make_theme();
        self.rebuild();
    }

    fn shape(&mut self, shape: fusuma::Shape) {
        let (w, h) = (self.bounds.w, self.bounds.h);
        let insets = shape.safe_insets(w, h);
        if insets != self.insets {
            self.insets = insets;
            self.shape = shape;
            self.rebuild();
        } else {
            self.shape = shape;
        }
    }

    fn resized(&mut self, width: u32, height: u32, scale_percent: u32) {
        let bounds = Rect::new(0, 0, width, height);
        let scale = scale_percent.clamp(50, 800);
        let class = if self.cells {
            Class::of(width.saturating_mul(CELL_PX))
        } else {
            fusuma::Screen {
                width,
                height: self.bounds.h,
                ..self.device
            }
            .with_scale(scale)
            .class()
        };
        if scale != self.scale || class != self.class {
            // Spacing and sizes are in the styles the tree was built with, so a new scale is a new tree.
            self.scale = scale;
            self.class = class;
            self.open = false;
            self.insets = fusuma::Screen::new(width, height)
                .with_shape(self.shape)
                .insets();
            self.theme = self.make_theme();
            self.bounds = bounds;
            self.tree.clear();
            self.pages.forget();
            self.sidebar = [None; 13];
            self.nav = [None; 3];
            self.tiles = [None; 13];
            self.back = None;
            self.content = None;
            let _ = self.build();
        } else if bounds != self.bounds {
            self.bounds = bounds;
            self.tree.invalidate();
        }
    }

    fn frame<R: Renderer>(&mut self, renderer: &mut R) -> Result<Option<Rect>, R::Error> {
        Self::frame(self, renderer)
    }
}

/// The widgets of the page on show, by role. A field is `None` when another page is on show.
#[derive(Clone, Copy, Debug)]
pub struct Widgets {
    /// The Buttons page's primary button.
    pub click_me: Option<NodeId>,
    /// The Selection page's checkboxes.
    pub checks: [Option<NodeId>; 3],
    /// The Selection page's radio buttons.
    pub radios: [Option<NodeId>; 3],
    /// The List page's rows.
    pub items: [Option<NodeId>; 12],
    /// The List page's scrolling window.
    pub list: Option<NodeId>,
    /// The Progress page's bar.
    pub bar: Option<NodeId>,
    /// The Progress page's start/pause button.
    pub start: Option<NodeId>,
    /// The Progress page's reset button.
    pub reset: Option<NodeId>,
    /// The Buttons page's Delete button, which opens the dialog.
    pub delete: Option<NodeId>,
    /// The Controls page's switch.
    pub switch: Option<NodeId>,
    /// The Controls page's slider.
    pub slider: Option<NodeId>,
    /// The Navigation page's tabs.
    pub tabs: [Option<NodeId>; 3],
    /// The Navigation page's button that opens the menu.
    pub menu_button: Option<NodeId>,
    /// The Navigation page's menu and its rows.
    pub menu: Option<NodeId>,
    /// The rows of the menu.
    pub menu_items: [Option<NodeId>; 3],
    /// The Navigation page's toast.
    pub toast: Option<NodeId>,
    /// The Navigation page's card that listens for a swipe and a long press.
    pub card: Option<NodeId>,
    /// The Reading page's text view and its paragraph.
    pub reading: [Option<NodeId>; 2],
    /// The Text page's text field.
    pub field: Option<NodeId>,
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma::render::{Point, Probe};

    fn app() -> (Showcase, fusuma::renderers::vello_cpu::VelloCpu) {
        let mut s = Showcase::new(1024, 600);
        s.slide = false;
        s.set_page(Page::Buttons);
        let mut r = fusuma::renderers::vello_cpu::new(1024, 600).unwrap();
        assert!(s.frame(&mut r).unwrap().is_some());
        (s, r)
    }

    fn tap(s: &mut Showcase, id: NodeId) {
        let measure = fusuma::renderers::vello_cpu::new(8, 8).unwrap();
        s.layout(&measure);
        let r = s.rect_of(id).unwrap();
        let p = Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            s.pointer(e);
        }
    }

    fn go(s: &mut Showcase, page: Page) {
        let id = s.sidebar_item(page).unwrap();
        // The sidebar scrolls: bring the entry into view first.
        let _ = s.tree.focus(id);
        tap(s, id);
        assert_eq!(s.page(), page);
    }

    #[test]
    fn the_first_frame_draws_everything_and_the_second_nothing() {
        let (mut s, mut r) = app();
        assert_eq!(s.frame(&mut r).unwrap(), None);
        assert_eq!(s.page(), Page::Buttons);
        assert!(s.is_dark());
        let bg = s.theme().colors.background;
        assert_eq!(r.pixel(0, 0).unwrap(), bg);
    }

    #[test]
    fn the_whole_app_fits_in_the_tree_on_every_page() {
        let (mut s, mut r) = app();
        for p in Page::ALL {
            go(&mut s, p);
            s.frame(&mut r).unwrap();
            assert!(
                s.tree().len() < NODES,
                "{p:?}: {} of {NODES}",
                s.tree().len()
            );
        }
    }

    #[test]
    fn clicking_counts_and_does_not_redraw_the_header_or_the_sidebar() {
        let (mut s, mut r) = app();
        let w = s.widgets().click_me.unwrap();
        tap(&mut s, w);
        tap(&mut s, w);
        assert_eq!(s.clicks(), 2);
        let region = s.frame(&mut r).unwrap().unwrap();
        // Damage is one bounding rectangle, so it spans from the button to the status line at the
        // bottom (and the status line sits under the sidebar, so it is as wide as the screen); the header is outside.
        assert!(region.y >= 100, "{region:?}");
        assert_eq!(s.frame(&mut r).unwrap(), None, "and then nothing is left");
    }

    #[test]
    fn choosing_a_page_swaps_the_content_and_says_so() {
        let (mut s, _) = app();
        go(&mut s, Page::Selection);
        assert!(s.widgets().radios[0].is_some() && s.widgets().click_me.is_none());
        go(&mut s, Page::Progress);
        assert!(s.widgets().bar.is_some() && s.widgets().radios[0].is_none());
        let status = s.tree().text(s.status.unwrap()).unwrap();
        assert_eq!(status, "Page: Progress");
    }

    #[test]
    fn radios_and_the_list_report_what_was_chosen() {
        let (mut s, _) = app();
        go(&mut s, Page::Selection);
        let w = s.widgets();
        tap(&mut s, w.radios[2].unwrap());
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::RADIO_LABEL).unwrap()),
            Some("Size: Large")
        );
        assert!(
            s.tree().is_checked(w.radios[2].unwrap()) && !s.tree().is_checked(w.radios[1].unwrap())
        );
        tap(&mut s, w.checks[1].unwrap());
        assert_eq!(s.tree().text(s.status.unwrap()), Some("Location on"));
        tap(&mut s, w.checks[2].unwrap());
        assert!(
            !s.tree().is_checked(w.checks[2].unwrap()),
            "the disabled one stays off"
        );
        go(&mut s, Page::List);
        let w = s.widgets();
        tap(&mut s, w.items[3].unwrap());
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::LIST_LABEL).unwrap()),
            Some("Chosen: Dune")
        );
    }

    #[test]
    fn the_progress_bar_runs_pauses_resets_and_stops_at_the_end() {
        let (mut s, mut r) = app();
        go(&mut s, Page::Progress);
        let w = s.widgets();
        assert_eq!(s.tree().progress_value(w.bar.unwrap()), Some(0));
        tap(&mut s, w.start.unwrap());
        assert!(s.is_running());
        s.tick(4000);
        assert_eq!(s.tree().progress_value(w.bar.unwrap()), Some(500));
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::BAR_LABEL).unwrap()),
            Some("50%")
        );
        tap(&mut s, w.start.unwrap());
        s.tick(1000);
        assert_eq!(s.tree().progress_value(w.bar.unwrap()), Some(500), "paused");
        tap(&mut s, w.start.unwrap());
        s.tick(60_000);
        assert_eq!(s.tree().progress_value(w.bar.unwrap()), Some(1000));
        assert!(!s.is_running(), "stops at the end");
        assert_eq!(s.tree().text(w.start.unwrap()), Some("Start"));
        s.frame(&mut r).unwrap();
        tap(&mut s, w.reset.unwrap());
        assert_eq!(s.tree().progress_value(w.bar.unwrap()), Some(0));
        tap(&mut s, w.start.unwrap());
        s.tick(1);
        assert!(s.is_running(), "a finished bar starts again from zero");
    }

    #[test]
    fn progress_survives_a_page_change() {
        let (mut s, _) = app();
        go(&mut s, Page::Progress);
        let target = s.widgets().start.unwrap();
        tap(&mut s, target);
        s.tick(2000);
        go(&mut s, Page::Buttons);
        s.tick(2000);
        go(&mut s, Page::Progress);
        assert_eq!(s.tree().progress_value(s.widgets().bar.unwrap()), Some(500));
        assert_eq!(s.tree().text(s.widgets().start.unwrap()), Some("Pause"));
    }

    #[test]
    fn the_theme_button_switches_and_repaints_everything() {
        let (mut s, mut r) = app();
        let dark_bg = s.theme().colors.background;
        let target = s.theme_button().unwrap();
        tap(&mut s, target);
        assert!(s.is_dark(), "opening the dropdown changes nothing");
        assert!(s.tree().is_menu_open(s.theme_menu.unwrap()));
        let light = s.theme_rows[1].unwrap();
        tap(&mut s, light);
        assert!(!s.is_dark());
        assert!(
            !s.tree().is_menu_open(s.theme_menu.unwrap()),
            "choosing closes it"
        );
        let region = s.frame(&mut r).unwrap().unwrap();
        assert_eq!(region, Rect::new(0, 0, 1024, 600));
        let light_bg = s.theme().colors.background;
        assert_ne!(dark_bg, light_bg);
        assert_eq!(r.pixel(0, 0).unwrap(), light_bg);
        assert_eq!(
            s.tree().text(s.theme_button().unwrap()),
            Some("Fusuma \u{b7} Light")
        );
        assert!(
            s.tree().is_checked(s.theme_rows[1].unwrap())
                && !s.tree().is_checked(s.theme_rows[0].unwrap())
        );
    }

    #[test]
    fn the_palette_button_steps_through_every_palette_and_keeps_dark_or_light() {
        let (mut s, mut r) = app();
        assert_eq!(
            s.tree().text(s.theme_button().unwrap()),
            Some("Fusuma \u{b7} Dark")
        );
        for want in [
            "forest", "ocean", "petrol", "violet", "ember", "graphite", "fusuma",
        ] {
            next_palette(&mut s);
            assert_eq!(s.palette().name, want);
            assert!(s.is_dark(), "the palette does not switch dark and light");
            let p = Palette::named(want).unwrap();
            assert_eq!(s.theme().colors, p.dark);
            let region = s.frame(&mut r).unwrap().unwrap();
            assert_eq!(
                region,
                Rect::new(0, 0, 1024, 600),
                "{want}: everything repaints"
            );
            assert_eq!(r.pixel(0, 0).unwrap(), p.dark.background, "{want}");
        }
        next_palette(&mut s);
        toggle_theme(&mut s);
        assert_eq!(s.theme().colors, Palette::FOREST.light, "light forest");
        assert_eq!(
            s.tree().text(s.theme_button().unwrap()),
            Some("Forest \u{b7} Light")
        );
    }

    #[test]
    fn on_a_watch_the_palette_switcher_is_a_tile_of_the_home_screen() {
        let (mut s, mut r) = watch();
        // Eleven tiles are more than the grid shows at once: scroll to the last row.
        let grid = s.content.unwrap();
        s.tree.scroll_to(grid, 10_000);
        s.frame(&mut r).unwrap();
        let tile = s.palette_button().unwrap();
        let last = s.tile(*Page::ALL.last().unwrap()).unwrap();
        let (a, b) = (s.rect_of(tile).unwrap(), s.rect_of(last).unwrap());
        assert_eq!((a.h, a.w), (b.h, b.w), "as big as a page tile");
        // An odd number of pages leaves the last row half empty: the palette tile fills it, beside the last page.
        assert_eq!(a.y, b.y, "{a:?} {b:?}");
        assert!(a.x > b.x);
        tap(&mut s, tile);
        assert_eq!(s.palette().name, "forest");
        assert_eq!(s.tree().text(s.palette_button().unwrap()), Some("Forest"));
        assert!(!s.is_open(), "it switches colours, it opens no page");
        open(&mut s, Page::Buttons);
        assert_eq!(s.palette_button(), None, "not on a page");
        close(&mut s);
        assert_eq!(s.palette().name, "forest", "kept across pages");
    }

    #[test]
    fn the_settings_palette_is_the_one_drawn_from_the_start() {
        let (mut s, mut r) = app();
        let green = Palette::choose(Some("petrol"), Some("forest")).unwrap();
        App::palette(&mut s, green);
        s.frame(&mut r).unwrap();
        assert_eq!(r.pixel(0, 0).unwrap(), Palette::PETROL.dark.background);
        assert_eq!(s.theme().colors.primary, Palette::FOREST.dark.primary);
        assert_eq!(
            s.tree().text(s.theme_button().unwrap()),
            Some("Petrol \u{b7} Dark")
        );
    }

    #[test]
    fn the_keyboard_drives_the_whole_app() {
        let (mut s, _) = app();
        s.key(Key::Next);
        assert_eq!(
            s.tree().focused(),
            s.theme_button(),
            "the header comes first"
        );
        s.key(Key::Next);
        assert_eq!(s.tree().focused(), s.sidebar_item(Page::Overview));
        s.key(Key::Down);
        s.key(Key::Down);
        s.key(Key::Activate);
        assert_eq!(s.page(), Page::Selection);
    }

    #[test]
    fn every_page_draws_in_both_themes_and_the_png_is_written_when_asked() {
        for dark in [true, false] {
            for p in Page::ALL {
                let (mut s, mut r) = app();
                if !dark {
                    toggle_theme(&mut s);
                }
                go(&mut s, p);
                s.frame(&mut r).unwrap();
                assert_eq!(r.pixel(1023, 599).unwrap(), s.theme().colors.background);
                if let Ok(dir) = std::env::var("FUSUMA_SHOWCASE_PNG") {
                    let name = std::format!(
                        "{dir}/showcase-{}-{}.png",
                        if dark { "dark" } else { "light" },
                        p.name()
                    );
                    std::fs::write(name, fusuma::renderers::vello_cpu::png(&r).unwrap()).unwrap();
                }
            }
        }
    }

    #[test]
    fn the_controls_page_has_a_working_switch_and_slider() {
        let (mut s, mut r) = app();
        go(&mut s, Page::Controls);
        let w = s.widgets();
        let (sw, sl) = (w.switch.unwrap(), w.slider.unwrap());
        assert!(s.tree().is_checked(sw));
        tap(&mut s, sw);
        assert!(!s.tree().is_checked(sw));
        assert_eq!(s.tree().text(s.status.unwrap()), Some("Wi-Fi off"));
        s.frame(&mut r).unwrap();
        let rect = s.rect_of(sl).unwrap();
        let y = rect.y + (rect.h / 2) as i32;
        s.pointer(Pointer::Down(Point::new(rect.x + rect.w as i32 - 2, y)));
        s.pointer(Pointer::Up(Point::new(rect.x + rect.w as i32 - 2, y)));
        assert_eq!(s.tree().slider_value(sl), Some(1000));
        assert_eq!(
            s.tree()
                .text(s.tree.find(pages::wid::SLIDER_LABEL).unwrap()),
            Some("Volume: 100%")
        );
        go(&mut s, Page::Buttons);
        go(&mut s, Page::Controls);
        assert_eq!(
            s.tree().slider_value(s.widgets().slider.unwrap()),
            Some(1000),
            "the value is kept"
        );
        s.key(Key::Next);
        s.tree.focus(s.widgets().slider.unwrap());
        s.key(Key::Left);
        assert_eq!(
            s.tree().slider_value(s.widgets().slider.unwrap()),
            Some(950)
        );
    }

    #[test]
    fn the_list_scrolls_by_wheel_and_by_drag_and_still_chooses() {
        let (mut s, mut r) = app();
        go(&mut s, Page::List);
        s.frame(&mut r).unwrap();
        let w = s.widgets();
        let list = w.list.unwrap();
        let view = s.rect_of(list).unwrap();
        assert!(
            s.tree().scroll_max(list).unwrap() > 0,
            "twelve rows do not fit 260 px"
        );
        s.pointer(Pointer::Wheel(Point::new(view.x + 20, view.y + 20), 150));
        assert_eq!(s.tree().scroll_offset(list), Some(150));
        s.frame(&mut r).unwrap();
        // A row that was out of sight is now at hand.
        let row = w.items[6].unwrap();
        tap(&mut s, row);
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::LIST_LABEL).unwrap()),
            Some("Chosen: Glacier")
        );
        // Dragging scrolls back without choosing anything.
        let start = Point::new(view.x + 30, view.y + 30);
        s.pointer(Pointer::Down(start));
        s.pointer(Pointer::Move(Point::new(start.x, start.y + 200)));
        s.pointer(Pointer::Up(Point::new(start.x, start.y + 200)));
        s.pointer(Pointer::Leave);
        assert_eq!(s.tree().scroll_offset(list), Some(0));
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::LIST_LABEL).unwrap()),
            Some("Chosen: Glacier")
        );
    }

    #[test]
    fn a_resize_lays_the_app_out_again_in_the_new_size() {
        let (mut s, mut r) = app();
        let before = s.rect_of(s.theme_button().unwrap()).unwrap();
        App::resized(&mut s, 800, 480, 100);
        let mut small = fusuma::renderers::vello_cpu::new(800, 480).unwrap();
        let region = s.frame(&mut small).unwrap().unwrap();
        assert_eq!(
            region,
            Rect::new(0, 0, 800, 480),
            "everything is drawn again"
        );
        let after = s.rect_of(s.theme_button().unwrap()).unwrap();
        assert!(
            after.right() <= 800 && after.right() < before.right(),
            "the header button moved in"
        );
        // The same size again is nothing to do.
        App::resized(&mut s, 800, 480, 100);
        assert_eq!(s.frame(&mut small).unwrap(), None);
        let _ = &mut r;
    }

    #[test]
    fn a_2x_screen_gets_a_2x_ui_with_bigger_text_and_the_same_state() {
        let (mut s, _) = app();
        let w = s.widgets().click_me.unwrap();
        tap(&mut s, w);
        go(&mut s, Page::Controls);
        let (base_h, base_text) = (s.theme().control_height, s.theme().text.body);
        App::resized(&mut s, 2048, 1200, 200);
        assert_eq!(s.theme().control_height, base_h * 2);
        assert_eq!(s.theme().text.body, base_text * 2);
        assert_eq!(s.page(), Page::Controls, "the page on show is kept");
        assert_eq!(s.clicks(), 1, "so is the state");
        let mut big = fusuma::renderers::vello_cpu::new(2048, 1200).unwrap();
        s.frame(&mut big).unwrap();
        let side = s.rect_of(s.sidebar_item(Page::Buttons).unwrap()).unwrap();
        assert!(side.w > 2 * 200, "the sidebar is twice as wide: {side:?}");
        assert!(side.h >= 2 * 44 - 2, "and a row twice as tall: {side:?}");
        // Back to 1x restores the original sizes.
        App::resized(&mut s, 1024, 600, 100);
        assert_eq!(s.theme().control_height, base_h);
    }

    #[test]
    fn the_same_scale_again_does_not_rebuild_and_the_light_theme_keeps_the_scale() {
        let (mut s, mut r) = app();
        App::resized(&mut s, 1024, 600, 150);
        s.frame(&mut r).unwrap();
        let t = s.theme_button().unwrap();
        App::resized(&mut s, 1024, 600, 150);
        assert_eq!(s.theme_button(), Some(t), "no rebuild for the same scale");
        toggle_theme(&mut s);
        assert!(!s.is_dark());
        assert_eq!(
            s.theme().control_height,
            84,
            "switching the theme keeps the 150% scale (56 x 1.5)"
        );
    }

    #[test]
    fn the_text_page_edits_echoes_and_keeps_the_text_when_the_page_is_left() {
        let (mut s, mut r) = app();
        go(&mut s, Page::Text);
        s.frame(&mut r).unwrap();
        let f = s.widgets().field.unwrap();
        assert_eq!(s.tree().text(f), Some("Hello"));
        tap(&mut s, f);
        assert!(App::wants_text(&s), "the field has the focus");
        App::text(&mut s, ", world");
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::ECHO).unwrap()),
            Some("You typed: Hello, world")
        );
        App::edit(&mut s, fusuma::Edit::Backspace);
        assert_eq!(s.tree().text(f), Some("Hello, worl"));
        App::edit(&mut s, fusuma::Edit::SelectAll);
        assert_eq!(App::selected_text(&s), Some("Hello, worl"));
        s.frame(&mut r).unwrap();
        assert!(
            App::text_cursor(&s).is_some(),
            "the platform can place an input method"
        );
        s.key(Key::Activate);
        assert_eq!(
            s.tree().text(s.status.unwrap()),
            Some("Submitted: Hello, worl")
        );
        go(&mut s, Page::Buttons);
        assert!(!App::wants_text(&s));
        go(&mut s, Page::Text);
        assert_eq!(
            s.tree().text(s.widgets().field.unwrap()),
            Some("Hello, worl"),
            "kept"
        );
    }

    fn phone() -> (Showcase, fusuma::renderers::vello_cpu::VelloCpu) {
        let mut s = Showcase::new(600, 800);
        s.slide = false;
        s.set_page(Page::Buttons);
        let mut r = fusuma::renderers::vello_cpu::new(600, 800).unwrap();
        s.frame(&mut r).unwrap();
        (s, r)
    }

    #[test]
    fn a_phone_gets_no_sidebar_and_a_previous_and_next_row() {
        let (s, _) = phone();
        assert!(s.is_compact() && s.sidebar_item(Page::Buttons).is_none());
        let (prev, next) = s.nav_buttons().unwrap();
        assert_ne!(prev, next);
        let (wide, _) = app();
        assert!(!wide.is_compact() && wide.nav_buttons().is_none());
    }

    #[test]
    fn next_and_previous_walk_the_pages_and_wrap_round() {
        let (mut s, mut r) = phone();
        let (prev, next) = s.nav_buttons().unwrap();
        for want in Page::ALL
            .iter()
            .skip_while(|p| **p != Page::Buttons)
            .skip(1)
        {
            tap(&mut s, next);
            assert_eq!(s.page(), *want);
            s.frame(&mut r).unwrap();
        }
        tap(&mut s, next);
        assert_eq!(s.page(), Page::Overview, "after the last comes the first");
        tap(&mut s, prev);
        assert_eq!(
            s.page(),
            *Page::ALL.last().unwrap(),
            "and before the first the last"
        );
        let want = std::format!("Page: {}", Page::ALL.last().unwrap().name());
        assert_eq!(s.tree().text(s.status.unwrap()), Some(want.as_str()));
    }

    #[test]
    fn every_page_fits_the_phone_screen_in_both_themes() {
        for dark in [true, false] {
            let (mut s, mut r) = phone();
            if !dark {
                let b = s.theme_button().unwrap();
                tap(&mut s, b);
            }
            let (_, next) = s.nav_buttons().unwrap();
            for _ in Page::ALL {
                // The page has slid in by now.
                App::tick(&mut s, 1000);
                s.frame(&mut r).unwrap();
                let tree = s.tree();
                let root = tree.rect(tree.root().unwrap()).unwrap();
                assert_eq!(root, Rect::new(0, 0, 600, 800));
                for id in s
                    .widgets()
                    .checks
                    .iter()
                    .chain(&s.widgets().radios)
                    .flatten()
                {
                    let rc = s.rect_of(*id).unwrap();
                    // Taller than the screen is fine (the page scrolls); wider is not.
                    assert!(rc.right() <= 600, "{:?}: {rc:?}", s.page());
                }
                if let Some(rc) = s.widgets().slider.and_then(|i| s.rect_of(i)) {
                    assert!(rc.right() <= 600, "{rc:?}");
                }
                if let Some(rc) = s.widgets().field.and_then(|i| s.rect_of(i)) {
                    assert!(rc.right() <= 600, "the text field fits: {rc:?}");
                }
                if let Some(rc) = s.widgets().list.and_then(|i| s.rect_of(i)) {
                    assert!(rc.right() <= 600 && rc.bottom() <= 800, "{rc:?}");
                }
                tap(&mut s, next);
            }
        }
    }

    #[test]
    fn the_layout_follows_the_width_in_both_directions_and_keeps_the_page_and_the_theme() {
        let (mut s, mut r) = app();
        go(&mut s, Page::Controls);
        toggle_theme(&mut s);
        App::resized(&mut s, 600, 800, 100);
        assert!(s.is_compact());
        assert_eq!((s.page(), s.is_dark()), (Page::Controls, false));
        let mut small = fusuma::renderers::vello_cpu::new(600, 800).unwrap();
        s.frame(&mut small).unwrap();
        assert_eq!(
            s.tree().text(s.theme_button().unwrap()),
            Some("Fusuma \u{b7} Light")
        );
        App::resized(&mut s, 1024, 600, 100);
        assert!(!s.is_compact() && s.sidebar_item(Page::Controls).is_some());
        assert_eq!(
            s.tree().text(s.theme_button().unwrap()),
            Some("Fusuma \u{b7} Light")
        );
        // A 2x screen of 1280 physical pixels is 640 logical: not compact; 1200 is.
        App::resized(&mut s, 1200, 800, 200);
        assert!(s.is_compact());
        App::resized(&mut s, 1280, 800, 200);
        assert!(!s.is_compact());
        let _ = &mut r;
    }

    #[test]
    fn the_phone_pictures_are_written_when_asked() {
        let Ok(dir) = std::env::var("FUSUMA_SHOWCASE_PNG") else {
            return;
        };
        let (mut s, mut r) = phone();
        let (_, next) = s.nav_buttons().unwrap();
        for p in Page::ALL {
            s.frame(&mut r).unwrap();
            std::fs::write(
                std::format!("{dir}/phone-{}.png", p.name()),
                fusuma::renderers::vello_cpu::png(&r).unwrap(),
            )
            .unwrap();
            tap(&mut s, next);
        }
    }

    fn watch() -> (Showcase, fusuma::renderers::vello_cpu::VelloCpu) {
        let mut s = Showcase::new(410, 502);
        s.slide = false;
        let mut r = fusuma::renderers::vello_cpu::new(410, 502).unwrap();
        s.frame(&mut r).unwrap();
        (s, r)
    }

    #[test]
    fn a_tiny_panel_is_a_list_with_no_palette_and_its_pages_stack_what_sat_side_by_side() {
        let micro = |w, h| {
            let mut s = Showcase::new(w, h);
            s.slide = false;
            let mut r = fusuma::renderers::vello_cpu::new(w, h).unwrap();
            s.frame(&mut r).unwrap();
            (s, r)
        };
        let (mut s, mut r) = micro(128, 296);
        assert!(s.micro());
        assert_eq!(
            s.theme_button(),
            None,
            "no palette switcher on a panel with no colours"
        );
        assert!(s.palette_button.is_none() && s.theme_menu.is_none());
        // One tile to a row, all the same width, one under the other.
        let rects: std::vec::Vec<_> = Page::ALL
            .iter()
            .filter_map(|p| s.tile(*p))
            .filter_map(|id| s.rect_of(id))
            .collect();
        assert!(rects.len() >= 4, "{rects:?}");
        assert!(
            rects
                .windows(2)
                .all(|w| w[0].x == w[1].x && w[0].w == w[1].w && w[1].y > w[0].y)
        );
        assert!(rects[0].right() <= 128, "inside the 128 px: {:?}", rects[0]);
        // A page: the two buttons are one under the other, both inside the screen.
        let id = s.tile(Page::Buttons).unwrap();
        s.tree.focus(id);
        tap(&mut s, id);
        s.frame(&mut r).unwrap();
        let (a, b) = (
            s.rect_of(s.tree().find(pages::wid::CLICK_ME).unwrap())
                .unwrap(),
            s.rect_of(s.tree().find(pages::wid::DELETE).unwrap())
                .unwrap(),
        );
        assert!(b.y >= a.bottom() && a.x == b.x, "{a:?} {b:?}");
        assert!(a.right() <= 128 && b.right() <= 128, "{a:?} {b:?}");
        // A declared Panel (a touch board, not a watch) with the same pixels is not a micro board.
        let mut p = Showcase::new(128, 296);
        App::screen(
            &mut p,
            fusuma::Screen::new(128, 296).with_form_factor(fusuma::FormFactor::Panel),
        );
        assert!(!p.micro() && p.class == Class::Watch);
        // And the watch and the phone are not.
        assert!(!Showcase::new(410, 502).micro() && !Showcase::new(800, 480).micro());
    }

    fn open(s: &mut Showcase, p: Page) {
        let id = s.tile(p).unwrap();
        // A tile below the window is scrolled to first, as a keyboard user would by tabbing to it.
        s.tree.focus(id);
        tap(s, id);
    }

    fn close(s: &mut Showcase) {
        let id = s.back_button().unwrap();
        tap(s, id);
    }

    /// Opens the theme dropdown and picks the other mode.
    fn toggle_theme(s: &mut Showcase) {
        let id = s.theme_button().unwrap();
        tap(s, id);
        let row = s.theme_rows[usize::from(s.is_dark())].unwrap();
        tap(s, row);
    }

    /// Picks the palette after the current one: from the theme dropdown, or by the watch's tile.
    fn next_palette(s: &mut Showcase) {
        if let Some(tile) = s.palette_button() {
            tap(s, tile);
            return;
        }
        let id = s.theme_button().unwrap();
        tap(s, id);
        let at = Palette::ALL
            .iter()
            .position(|p| p.name == Showcase::palette(s).name)
            .unwrap();
        let row = s.palette_rows[(at + 1) % Palette::ALL.len()].unwrap();
        tap(s, row);
    }

    #[test]
    fn the_class_follows_the_logical_width() {
        for (w, c) in [
            (0, Class::Watch),
            (410, Class::Watch),
            (479, Class::Watch),
            (480, Class::Phone),
            (639, Class::Phone),
            (640, Class::Tablet),
            (1024, Class::Tablet),
            (1099, Class::Tablet),
            (1100, Class::Desktop),
            (u32::MAX, Class::Desktop),
        ] {
            assert_eq!(Class::of(w), c, "{w}");
        }
        assert!(Class::Watch.is_compact() && Class::Phone.is_compact());
        assert!(!Class::Tablet.is_compact() && !Class::Desktop.is_compact());
    }

    #[test]
    fn a_watch_starts_on_a_grid_of_tiles_and_a_tile_opens_its_page_with_a_back_button() {
        let (mut s, mut r) = watch();
        assert_eq!(s.class(), Class::Watch);
        assert!(!s.is_open() && s.back_button().is_none() && s.nav_buttons().is_none());
        for p in Page::ALL {
            assert!(s.tile(p).is_some(), "{p:?}");
        }
        open(&mut s, Page::Controls);
        assert!(s.is_open() && s.page() == Page::Controls);
        assert!(s.tile(Page::Controls).is_none(), "the grid is gone");
        s.frame(&mut r).unwrap();
        assert!(s.widgets().switch.is_some() && s.widgets().slider.is_some());
        let back = s.back_button().unwrap();
        tap(&mut s, back);
        assert!(!s.is_open() && s.tile(Page::Controls).is_some());
    }

    #[test]
    fn the_watch_shows_fewer_components_than_the_tablet() {
        let (mut s, _) = watch();
        open(&mut s, Page::Selection);
        let w = s.widgets();
        assert!(w.checks[2].is_none(), "the third checkbox is left out");
        let small = s.tree().len();
        let (mut big, _) = app();
        go(&mut big, Page::Selection);
        assert!(
            small + 6 < big.tree().len(),
            "{small} against {}",
            big.tree().len()
        );
        let (mut s, _) = watch();
        open(&mut s, Page::Buttons);
        assert!(
            s.tree().len() - 6 - 3 < 15, // the closed dialog's six and the theme menu's three are not the page
            "the Buttons page is a button and a counter: {}",
            s.tree().len()
        );
    }

    #[test]
    fn state_survives_opening_and_closing_pages_on_the_watch() {
        let (mut s, mut r) = watch();
        open(&mut s, Page::Buttons);
        s.frame(&mut r).unwrap();
        let b = s.widgets().click_me.unwrap();
        tap(&mut s, b);
        tap(&mut s, b);
        close(&mut s);
        open(&mut s, Page::Buttons);
        assert_eq!(s.clicks(), 2);
        assert!(
            s.tree()
                .text(s.tree.find(pages::wid::COUNTER).unwrap())
                .unwrap()
                .contains('2')
        );
        close(&mut s);
        open(&mut s, Page::Controls);
        s.frame(&mut r).unwrap();
        let sl = s.rect_of(s.widgets().slider.unwrap()).unwrap();
        let y = sl.y + (sl.h / 2) as i32;
        s.pointer(Pointer::Down(Point::new(sl.right() - 2, y)));
        s.pointer(Pointer::Up(Point::new(sl.right() - 2, y)));
        close(&mut s);
        open(&mut s, Page::Controls);
        assert_eq!(
            s.tree().slider_value(s.widgets().slider.unwrap()),
            Some(1000)
        );
    }

    #[test]
    fn every_watch_page_fits_the_screen_and_a_change_of_size_changes_the_layout() {
        for dark in [true, false] {
            let (mut s, mut r) = watch();
            if !dark {
                toggle_theme(&mut s);
            }
            for p in Page::ALL {
                open(&mut s, p);
                s.frame(&mut r).unwrap();
                let w = s.widgets();
                let ids = w
                    .checks
                    .iter()
                    .chain(&w.radios)
                    .chain(&w.items)
                    .chain([
                        &w.click_me,
                        &w.bar,
                        &w.start,
                        &w.reset,
                        &w.switch,
                        &w.slider,
                        &w.field,
                        &w.list,
                    ])
                    .flatten();
                for id in ids {
                    let rect = s.rect_of(*id).unwrap();
                    assert!(
                        rect.right() <= 410,
                        "{p:?}: {rect:?} is wider than the watch"
                    );
                }

                close(&mut s);
                s.frame(&mut r).unwrap();
            }
        }
        let (mut s, _) = watch();
        open(&mut s, Page::List);
        App::resized(&mut s, 1024, 600, 100);
        assert_eq!((s.class(), s.is_open()), (Class::Tablet, false));
        assert!(s.sidebar_item(Page::List).is_some());
        App::resized(&mut s, 300, 300, 100);
        assert_eq!(s.class(), Class::Watch);
        assert!(s.tile(Page::List).is_some());
    }

    #[test]
    fn the_watch_pictures_are_written_when_asked() {
        let Ok(dir) = std::env::var("FUSUMA_SHOWCASE_PNG") else {
            return;
        };
        let (mut s, mut r) = watch();
        std::fs::write(
            std::format!("{dir}/watch-Home.png"),
            fusuma::renderers::vello_cpu::png(&r).unwrap(),
        )
        .unwrap();
        for p in Page::ALL {
            open(&mut s, p);
            s.frame(&mut r).unwrap();
            std::fs::write(
                std::format!("{dir}/watch-{}.png", p.name()),
                fusuma::renderers::vello_cpu::png(&r).unwrap(),
            )
            .unwrap();
            close(&mut s);
            s.frame(&mut r).unwrap();
        }
    }

    /// Whether the point (x, y) is on the visible part of a `w` x `h` screen with rounded corners of radius `r`.
    fn visible(x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
        let (cx, cy) = (x.clamp(r, w - r), y.clamp(r, h - r));
        let (dx, dy) = (i64::from(x - cx), i64::from(y - cy));
        x >= 0 && y >= 0 && x < w && y < h && dx * dx + dy * dy <= i64::from(r) * i64::from(r)
    }

    #[test]
    fn on_a_rounded_screen_every_widget_corner_is_on_the_visible_part() {
        for r in [40u32, 60, 90] {
            let mut s = Showcase::new(410, 502);
            App::shape(&mut s, fusuma::Shape::Rounded(r));
            let mut screen = fusuma::renderers::vello_cpu::new(410, 502).unwrap();
            s.frame(&mut screen).unwrap();
            let mut ids = std::vec![s.theme_button().unwrap()];
            ids.extend(Page::ALL.iter().filter_map(|p| s.tile(*p)));
            let window = s.rect_of(s.content.unwrap()).unwrap();
            for id in ids {
                // What is scrolled out of its window is clipped there; judge the part that shows.
                let rc = s.rect_of(id).unwrap();
                let rc = if id == s.theme_button().unwrap() {
                    rc
                } else {
                    rc.intersect(&window)
                };
                if rc.w == 0 || rc.h == 0 {
                    continue;
                }
                for (x, y) in [
                    (rc.x, rc.y),
                    (rc.right() - 1, rc.y),
                    (rc.x, rc.bottom() - 1),
                    (rc.right() - 1, rc.bottom() - 1),
                ] {
                    assert!(
                        visible(x, y, 410, 502, r as i32),
                        "radius {r}: corner ({x}, {y}) of {rc:?} is cut off"
                    );
                }
            }
        }
    }

    #[test]
    fn a_reflective_glass_stops_the_motion_and_a_micro_board_gets_the_small_layout() {
        let mut s = Showcase::new(296, 128);
        let before = s.theme.motion;
        assert!(before.normal_ms > 0, "an ordinary screen moves");
        App::screen(
            &mut s,
            fusuma::Screen::new(296, 128)
                .with_form_factor(fusuma::FormFactor::Micro)
                .with_glass(fusuma::DisplayKind::Reflective, 1, 300),
        );
        assert_eq!(s.theme.motion.normal_ms, 0, "e-ink does not animate");
        assert_eq!(s.theme.motion.fast_ms, 0);
        assert_eq!(s.class, Class::Watch);
        // A resize afterwards keeps what the device is.
        App::resized(&mut s, 296, 128, 100);
        assert!(s.device().is_reflective());
        assert_eq!(s.theme.motion.normal_ms, 0);
        // A phone told its glass is 68 mm wide gets the phone layout though 360 logical px is under the watch limit.
        let mut p = Showcase::new(1080, 2400);
        App::screen(
            &mut p,
            fusuma::Screen::new(1080, 2400).with_size_mm(68, 150),
        );
        App::resized(&mut p, 1080, 2400, 300);
        assert_eq!(p.class, Class::Phone);
    }

    #[test]
    fn a_rectangular_screen_is_laid_out_exactly_as_before() {
        let (mut plain, _) = watch();
        let before = plain.rect_of(plain.theme_button().unwrap()).unwrap();
        App::shape(&mut plain, fusuma::Shape::Rect);
        assert_eq!(plain.rect_of(plain.theme_button().unwrap()), Some(before));
        let mut round = Showcase::new(410, 502);
        App::shape(&mut round, fusuma::Shape::Rounded(60));
        let mut r = fusuma::renderers::vello_cpu::new(410, 502).unwrap();
        round.frame(&mut r).unwrap();
        let moved = round.rect_of(round.theme_button().unwrap()).unwrap();
        assert!(
            moved.x < before.x && moved.y > before.y,
            "{moved:?} against {before:?}"
        );
        assert!(
            moved.right() < before.right(),
            "the button moved in from the right edge"
        );
    }

    #[test]
    fn the_shape_survives_a_resize_and_a_page_open_and_works_on_a_circle() {
        let mut s = Showcase::new(466, 466);
        App::shape(&mut s, fusuma::Shape::Circle);
        let mut r = fusuma::renderers::vello_cpu::new(466, 466).unwrap();
        s.frame(&mut r).unwrap();
        let tile = s.tile(Page::Buttons).unwrap();
        let rc = s.rect_of(tile).unwrap();
        assert!(visible(rc.x, rc.y, 466, 466, 233), "{rc:?} on a circle");
        open(&mut s, Page::Buttons);
        App::resized(&mut s, 466, 466, 100);
        s.frame(&mut r).unwrap();
        let b = s.rect_of(s.theme_button().unwrap()).unwrap();
        assert!(
            visible(b.right() - 1, b.y, 466, 466, 233),
            "{b:?} still inside after a resize"
        );
    }

    #[test]
    fn the_rounded_watch_pictures_are_written_when_asked() {
        let Ok(dir) = std::env::var("FUSUMA_SHOWCASE_PNG") else {
            return;
        };
        let mut s = Showcase::new(410, 502);
        App::shape(&mut s, fusuma::Shape::Rounded(70));
        let mut r = fusuma::renderers::vello_cpu::new(410, 502).unwrap();
        s.frame(&mut r).unwrap();
        // The glass: what is outside the rounded corners is black, as on the real screen.
        for y in 0..502 {
            for x in 0..410 {
                if !visible(x, y, 410, 502, 70) {
                    r.fill_rect(Rect::new(x, y, 1, 1), 0, fusuma::render::Color::BLACK)
                        .unwrap();
                }
            }
        }
        std::fs::write(
            std::format!("{dir}/watch-rounded-home.png"),
            fusuma::renderers::vello_cpu::png(&r).unwrap(),
        )
        .unwrap();
    }

    fn tap_id(s: &mut Showcase, id: NodeId) {
        tap(s, id);
    }

    #[test]
    fn the_delete_button_opens_a_dialog_that_cancel_dismiss_and_delete_close() {
        let (mut s, mut r) = app();
        let [d, cancel, ok] = s.dialog_nodes().map(Option::unwrap);
        assert!(!s.tree().is_dialog_open(d));
        let del = s.widgets().delete.unwrap();
        tap_id(&mut s, del);
        assert!(s.tree().is_dialog_open(d));
        s.frame(&mut r).unwrap();
        // The page behind does not answer while it is open.
        let click = s.widgets().click_me.unwrap();
        tap_id(&mut s, click);
        assert_eq!(s.clicks(), 0, "the press behind the dialog did not count");
        assert!(
            !s.tree().is_dialog_open(d),
            "a press outside asked to close it, and the app did"
        );
        assert_eq!(s.tree().text(s.status.unwrap()), Some("Cancelled"));
        // Reopen, then Cancel.
        tap_id(&mut s, del);
        s.frame(&mut r).unwrap();
        tap_id(&mut s, cancel);
        assert!(!s.tree().is_dialog_open(d));
        assert_eq!(s.tree().text(s.status.unwrap()), Some("Cancelled"));
        // Reopen, then Delete.
        tap_id(&mut s, del);
        s.frame(&mut r).unwrap();
        tap_id(&mut s, ok);
        assert!(!s.tree().is_dialog_open(d));
        assert_eq!(s.tree().text(s.status.unwrap()), Some("Deleted"));
        // Reopen, then Escape.
        tap_id(&mut s, del);
        s.frame(&mut r).unwrap();
        s.key(Key::Cancel);
        assert!(!s.tree().is_dialog_open(d), "Escape dismisses it");
    }

    #[test]
    fn the_dialog_fits_every_screen_class_and_is_clear_of_a_rounded_corner() {
        for (w, h, shape) in [
            (410, 502, fusuma::Shape::Rounded(70)),
            (600, 800, fusuma::Shape::Rect),
            (1024, 600, fusuma::Shape::Rect),
            (466, 466, fusuma::Shape::Circle),
        ] {
            let mut s = Showcase::new(w, h);
            App::shape(&mut s, shape);
            let mut r = fusuma::renderers::vello_cpu::new(w, h).unwrap();
            s.frame(&mut r).unwrap();
            if let Some(tile) = s.tile(Page::Buttons) {
                tap_id(&mut s, tile);
            } else {
                s.set_page(Page::Buttons);
            }
            s.frame(&mut r).unwrap();
            let del = s.widgets().delete.unwrap();
            tap_id(&mut s, del);
            s.frame(&mut r).unwrap();
            let rc = s.rect_of(s.dialog_nodes()[0].unwrap()).unwrap();
            assert!(
                rc.w > 0 && rc.h > 0 && rc.right() <= w as i32 && rc.bottom() <= h as i32,
                "{w}x{h}: {rc:?}"
            );
            if let fusuma::Shape::Circle = shape {
                assert!(
                    visible(rc.x, rc.y, w as i32, h as i32, (w / 2) as i32),
                    "{rc:?} corner on a circle"
                );
            }
        }
    }

    #[test]
    fn the_dialog_pictures_are_written_when_asked() {
        let Ok(dir) = std::env::var("FUSUMA_SHOWCASE_PNG") else {
            return;
        };
        for (name, w, h, shape) in [
            ("tablet", 1024, 600, fusuma::Shape::Rect),
            ("watch", 410, 502, fusuma::Shape::Rounded(70)),
        ] {
            let mut s = Showcase::new(w, h);
            App::shape(&mut s, shape);
            let mut r = fusuma::renderers::vello_cpu::new(w, h).unwrap();
            s.frame(&mut r).unwrap();
            if let Some(tile) = s.tile(Page::Buttons) {
                tap_id(&mut s, tile);
            } else {
                s.set_page(Page::Buttons);
            }
            s.frame(&mut r).unwrap();
            let del = s.widgets().delete.unwrap();
            tap_id(&mut s, del);
            s.frame(&mut r).unwrap();
            std::fs::write(
                std::format!("{dir}/dialog-{name}.png"),
                fusuma::renderers::vello_cpu::png(&r).unwrap(),
            )
            .unwrap();
        }
    }

    /// Lets the page that slid in settle, and lays it out.
    fn settle(s: &mut Showcase) {
        App::tick(s, 2000);
        let measure = fusuma::renderers::vello_cpu::new(8, 8).unwrap();
        s.layout(&measure);
    }

    #[test]
    fn the_new_page_slides_in_from_the_right_and_settles_in_its_place() {
        let (mut s, mut r) = app();
        s.slide = true;
        go(&mut s, Page::Navigation);
        s.frame(&mut r).unwrap();
        let c = s.content.unwrap();
        let (dx, _) = s.tree().slide_offset(c).unwrap();
        assert!(dx > 0, "it starts to the right: {dx}");
        let mut last = dx;
        while App::tick(&mut s, 16) {
            s.frame(&mut r).unwrap();
            let now = s.tree().slide_offset(c).unwrap().0;
            assert!(now <= last, "it only comes closer");
            last = now;
        }
        assert_eq!(s.tree().slide_offset(c), Some((0, 0)));
    }

    #[test]
    fn the_tabs_choose_one_page_of_the_navigation_page_and_say_so() {
        let (mut s, _) = app();
        go(&mut s, Page::Navigation);
        settle(&mut s);
        let tabs = s.widgets().tabs.map(Option::unwrap);
        assert!(s.tree().is_checked(tabs[0]));
        tap(&mut s, tabs[2]);
        assert!(s.tree().is_checked(tabs[2]) && !s.tree().is_checked(tabs[0]));
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::TAB_LABEL).unwrap()),
            Some("Settings tab")
        );
        // The choice is kept when the page is left and come back to.
        go(&mut s, Page::Buttons);
        go(&mut s, Page::Navigation);
        settle(&mut s);
        assert!(s.tree().is_checked(s.widgets().tabs[2].unwrap()));
    }

    #[test]
    fn the_menu_opens_below_its_button_a_row_shows_a_toast_and_the_dialog_is_not_dismissed() {
        let (mut s, _) = app();
        go(&mut s, Page::Navigation);
        settle(&mut s);
        let w = s.widgets();
        let (button, menu, toast) = (w.menu_button.unwrap(), w.menu.unwrap(), w.toast.unwrap());
        assert!(!s.tree().is_menu_open(menu));
        tap(&mut s, button);
        assert!(s.tree().is_menu_open(menu));
        settle(&mut s);
        let (m, b) = (s.rect_of(menu).unwrap(), s.rect_of(button).unwrap());
        assert!(m.y >= b.bottom(), "below the button: {m:?} {b:?}");
        let row = s.widgets().menu_items[1].unwrap();
        tap(&mut s, row);
        assert!(!s.tree().is_menu_open(menu), "choosing closes it");
        assert!(
            s.tree().is_toast_shown(toast),
            "and a toast says what was chosen"
        );
        assert_eq!(s.tree().text(s.status.unwrap()), Some("Edit"));
        // Dismissing the menu is not a dismissal of the confirmation dialog.
        tap(&mut s, button);
        let outside = s.tab_outside_point();
        for e in [Pointer::Down(outside), Pointer::Up(outside), Pointer::Leave] {
            s.pointer(e);
        }
        assert!(!s.tree().is_menu_open(menu));
        assert_ne!(s.tree().text(s.status.unwrap()), Some("Cancelled"));
        assert!(!s.tree().is_dialog_open(s.dialog[0].unwrap()));
    }

    #[test]
    fn a_swipe_and_a_long_press_on_the_card_are_counted_and_shown() {
        let (mut s, _) = app();
        go(&mut s, Page::Navigation);
        settle(&mut s);
        let card = s.widgets().card.unwrap();
        let r = s.rect_of(card).unwrap();
        let p = Point::new(r.x + 20, r.y + (r.h / 2) as i32);
        s.pointer(Pointer::Down(p));
        s.pointer(Pointer::Move(Point::new(p.x + 60, p.y)));
        s.pointer(Pointer::Up(Point::new(p.x + 60, p.y)));
        s.pointer(Pointer::Leave);
        assert_eq!(
            s.tree().text(s.tree.find(pages::wid::CARD_LABEL).unwrap()),
            Some("Swiped Right (1)")
        );
        s.pointer(Pointer::Down(p));
        App::tick(&mut s, 600);
        s.pointer(Pointer::Up(p));
        s.pointer(Pointer::Leave);
        assert!(s.tree().is_toast_shown(s.widgets().toast.unwrap()));
    }

    #[test]
    fn the_components_page_answers_every_part_and_says_what_happened() {
        let (mut s, mut r) = app();
        go(&mut s, Page::Components);
        settle(&mut s);
        s.frame(&mut r).unwrap();
        let by = |s: &Showcase, name: &str| {
            s.tree()
                .semantics()
                .find(|n| n.name == name)
                .map(|n| n.id)
                .unwrap_or_else(|| panic!("no '{name}' on the page"))
        };
        // The page scrolls: a part below the window is focused (which scrolls to it) before it is pressed.
        let press = |s: &mut Showcase, r: &mut _, id| {
            s.tree.focus(id);
            s.frame(r).unwrap();
            tap(s, id);
        };
        let status = |s: &Showcase| {
            s.status
                .and_then(|id| s.tree().text(id))
                .map(std::string::ToString::to_string)
        };
        // The segmented control.
        let week = by(&s, "Week");
        press(&mut s, &mut r, week);
        assert_eq!(s.segment, 1);
        assert_eq!(status(&s).as_deref(), Some("Week"));
        // A chip toggles, and several can be on.
        let bt = by(&s, "Bluetooth");
        press(&mut s, &mut r, bt);
        assert_eq!(s.chips, [true, true, false]);
        assert_eq!(status(&s).as_deref(), Some("Bluetooth on"));
        let wifi = by(&s, "Wi-Fi");
        press(&mut s, &mut r, wifi);
        assert_eq!(s.chips, [false, true, false]);
        // Pagination: page 3 is current, the next arrow goes to 4, a number goes there.
        assert_eq!(s.page_no, 3);
        let next = by(&s, ">");
        press(&mut s, &mut r, next);
        assert_eq!(s.page_no, 4);
        assert_eq!(status(&s).as_deref(), Some("Page 4"));
        let last = by(&s, "24");
        press(&mut s, &mut r, last);
        assert_eq!(s.page_no, 24);
        s.frame(&mut r).unwrap();
        assert!(s.tree().is_disabled(by(&s, ">")), "no page after the last");
        // The steps move on, and come round to the first again.
        let more = by(&s, "Next step");
        press(&mut s, &mut r, more);
        assert_eq!((s.step, status(&s).as_deref()), (2, Some("Step 3")));
        press(&mut s, &mut r, more);
        assert_eq!(s.step, 0);
        // The snackbar and a breadcrumb.
        let undo = by(&s, "Undo");
        press(&mut s, &mut r, undo);
        assert_eq!(status(&s).as_deref(), Some("Undone"));
        let home = by(&s, "Home");
        press(&mut s, &mut r, home);
        assert_eq!(status(&s).as_deref(), Some("Back to Home"));
    }

    #[test]
    fn the_data_and_pickers_pages_answer_every_part_and_keep_their_state() {
        let (mut s, mut r) = app();
        let by = |s: &Showcase, name: &str| {
            s.tree()
                .semantics()
                .filter(|n| n.name == name)
                .last()
                .map(|n| n.id)
                .unwrap_or_else(|| panic!("no '{name}' on the page"))
        };
        let press = |s: &mut Showcase, r: &mut _, name: &str| {
            let id = by(s, name);
            s.tree.focus(id);
            s.frame(r).unwrap();
            tap(s, id);
        };
        let status = |s: &Showcase| {
            s.status
                .and_then(|id| s.tree().text(id))
                .map(std::string::ToString::to_string)
        };
        // Data: a star, a carousel arrow.
        go(&mut s, Page::Data);
        settle(&mut s);
        s.frame(&mut r).unwrap();
        let stars: std::vec::Vec<_> = s
            .tree()
            .semantics()
            .filter(|n| n.name == "*")
            .map(|n| n.id)
            .collect();
        assert_eq!(stars.len(), 5);
        s.tree.focus(stars[4]);
        s.frame(&mut r).unwrap();
        tap(&mut s, stars[4]);
        assert_eq!(s.x.rating, 5);
        assert_eq!(status(&s).as_deref(), Some("5 of 5"));
        press(&mut s, &mut r, ">");
        assert_eq!(s.x.slide, 1);
        assert!(s.tree().semantics().any(|n| n.name == "Pick a theme"));
        // Pickers: the calendar, the time, the select, the tree, the fold.
        go(&mut s, Page::Pickers);
        settle(&mut s);
        s.frame(&mut r).unwrap();
        press(&mut s, &mut r, "20");
        assert_eq!(s.x.day, 20);
        assert_eq!(status(&s).as_deref(), Some("20 November 2026"));
        press(&mut s, &mut r, ">");
        assert_eq!((s.x.year, s.x.month), (2026, 12));
        press(&mut s, &mut r, ">");
        press(&mut s, &mut r, "<");
        assert_eq!((s.x.year, s.x.month), (2026, 12));
        for _ in 0..12 {
            press(&mut s, &mut r, ">");
        }
        assert_eq!((s.x.year, s.x.month), (2027, 12), "a year on");
        // 31 December selected is held to the month's length when the month changes.
        s.x.day = 31;
        s.sync_pages();
        press(&mut s, &mut r, "<");
        assert_eq!((s.x.month, s.x.day), (11, 30));
        let plus = s
            .tree()
            .semantics()
            .filter(|n| n.name == "+")
            .map(|n| n.id)
            .collect::<std::vec::Vec<_>>();
        s.tree.focus(plus[0]);
        s.frame(&mut r).unwrap();
        tap(&mut s, plus[0]);
        assert_eq!((s.x.hour, s.x.minute), (10, 30));
        assert_eq!(status(&s).as_deref(), Some("10:30"));
        // The select opens, and picking closes it and shows the choice.
        press(&mut s, &mut r, "Medium");
        assert!(s.x.size_open);
        press(&mut s, &mut r, "Large");
        assert_eq!((s.x.size, s.x.size_open), (Some(2), false));
        // The tree: folding hides the children and selects the folder.
        let before = s.tree().semantics().filter(|n| n.name == "lib.rs").count();
        assert_eq!(before, 1);
        press(&mut s, &mut r, "v");
        assert!(!s.x.tree_open);
        assert_eq!(
            s.tree().semantics().filter(|n| n.name == "lib.rs").count(),
            0
        );
        // The fold opens.
        press(&mut s, &mut r, "More");
        assert!(s.x.fold);
        assert!(
            s.tree()
                .semantics()
                .any(|n| n.name == "Hidden until opened")
        );
    }

    #[test]
    fn the_reading_page_wraps_its_text_and_scrolls_it() {
        let (mut s, mut r) = app();
        go(&mut s, Page::Reading);
        settle(&mut s);
        s.frame(&mut r).unwrap();
        let [view, para] = s.widgets().reading.map(Option::unwrap);
        let (v, p) = (s.rect_of(view).unwrap(), s.rect_of(para).unwrap());
        assert!(p.h > v.h, "more lines than fit: {p:?} in {v:?}");
        assert!(s.tree().scroll_max(view).unwrap() > 0);
        // On a phone the same text is narrower, so it is taller.
        let (mut ph, mut pr) = phone();
        let (_, next) = ph.nav_buttons().unwrap();
        for _ in Page::ALL
            .iter()
            .skip_while(|p| **p != Page::Buttons)
            .skip(1)
            .take_while(|p| **p != Page::Components)
        {
            tap(&mut ph, next);
        }
        settle(&mut ph);
        ph.frame(&mut pr).unwrap();
        assert_eq!(ph.page(), Page::Reading);
    }

    /// The text of a frame of the text UI, a row to a line.
    fn cells_text(s: &mut Showcase, cols: u16, rows: u16) -> std::string::String {
        let mut g = fusuma::cells::Grid::new(
            cols,
            rows,
            std::vec![fusuma::cells::Cell::BLANK; usize::from(cols) * usize::from(rows)],
        )
        .unwrap();
        s.frame_cells(&mut g, &fusuma::CellTheme::UNICODE);
        let mut out = std::string::String::new();
        for y in 0..rows {
            let line: std::string::String = g
                .row(y)
                .unwrap()
                .iter()
                .filter(|c| !c.is_tail())
                .map(|c| c.ch)
                .collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    fn tap_cells(s: &mut Showcase, id: NodeId) {
        s.layout(&fusuma::CellMeasure);
        let r = s.rect_of(id).unwrap();
        let p = Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            s.pointer(e);
        }
    }

    #[test]
    fn the_text_ui_draws_every_page_at_a_few_sizes() {
        for (cols, rows) in [(80u16, 24u16), (100, 30), (50, 24)] {
            let mut s = Showcase::new_cells(u32::from(cols), u32::from(rows));
            let want_class = Class::of(u32::from(cols) * CELL_PX);
            assert_eq!(s.class(), want_class, "{cols}x{rows}");
            for p in Page::ALL {
                if want_class == Class::Watch {
                    // The tile grid: open the page by its tile.
                    s.open = false;
                    s.rebuild();
                    s.layout(&fusuma::CellMeasure);
                    if let Some(t) = s.tile(p) {
                        let _ = s.tree.focus(t);
                        tap_cells(&mut s, t);
                    }
                } else {
                    s.set_page(p);
                }
                App::tick(&mut s, 1000);
                let text = cells_text(&mut s, cols, rows);
                if want_class == Class::Watch {
                    assert!(text.contains(p.name()), "{cols}x{rows} {p:?}:\n{text}");
                } else {
                    assert!(text.contains("Fusuma"), "{cols}x{rows} {p:?}:\n{text}");
                }
                if !want_class.is_compact() {
                    assert!(
                        text.contains(p.name()),
                        "{cols}x{rows}: the page's name\n{text}"
                    );
                }
                if p == Page::Selection && !want_class.is_compact() {
                    assert!(
                        text.contains("[x] Notifications") && text.contains("(•) Medium"),
                        "{text}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_text_ui_takes_taps_in_cells() {
        let mut s = Showcase::new_cells(100, 30);
        s.set_page(Page::Buttons);
        let click = s.widgets().click_me.unwrap();
        tap_cells(&mut s, click);
        assert_eq!(s.clicks(), 1);
        let text = cells_text(&mut s, 100, 30);
        assert!(text.contains("Clicked 1 times"), "{text}");
    }

    #[test]
    fn the_dropdown_pictures_are_written_when_asked() {
        let Ok(dir) = std::env::var("FUSUMA_SHOWCASE_PNG") else {
            return;
        };
        let (mut s, mut r) = app();
        let b = s.theme_button().unwrap();
        tap(&mut s, b);
        App::tick(&mut s, 1000);
        s.frame(&mut r).unwrap();
        std::fs::write(
            std::format!("{dir}/dropdown.png"),
            fusuma::renderers::vello_cpu::png(&r).unwrap(),
        )
        .unwrap();
    }

    /// The pictures the web build must draw: the same start as its address asks for (`set_start`), on Vello CPU at
    /// 1024 x 600. `scripts/web_shots.nu` compares a browser's screenshots with them.
    #[test]
    fn the_web_reference_pictures_are_written_when_asked() {
        let Ok(dir) = std::env::var("FUSUMA_WEB_REFERENCE") else {
            return;
        };
        for (dark, mode) in [(true, "dark"), (false, "light")] {
            for p in Page::ALL {
                let mut s = Showcase::new(1024, 600);
                s.set_start(Some(p.name()), Some(dark), None, true, false);
                let mut r = fusuma::renderers::vello_cpu::new(1024, 600).unwrap();
                App::tick(&mut s, 2000);
                s.frame(&mut r).unwrap();
                std::fs::write(
                    std::format!("{dir}/{mode}-{}.png", p.name()),
                    fusuma::renderers::vello_cpu::png(&r).unwrap(),
                )
                .unwrap();
            }
        }
        // The dropdown open, over the first page.
        let mut s = Showcase::new(1024, 600);
        s.set_start(None, None, None, true, true);
        let mut r = fusuma::renderers::vello_cpu::new(1024, 600).unwrap();
        App::tick(&mut s, 2000);
        s.frame(&mut r).unwrap();
        std::fs::write(
            std::format!("{dir}/dark-menu.png"),
            fusuma::renderers::vello_cpu::png(&r).unwrap(),
        )
        .unwrap();
    }
}
