//! A showcase of the Fusuma components: one small app that uses every widget there is.
//!
//! It is the same program everywhere. It owns a [`Tree`] and reacts to [`Pointer`] and [`Key`] events and to
//! the passing of time; a host (a test, a window, a board) feeds those in and calls [`Showcase::frame`] to
//! have what changed drawn on any [`Renderer`]. `no_std`, no allocator.
#![no_std]

use core::fmt::{self, Write};

use fusuma::render::{Rect, Renderer};
use fusuma::{
    ActionKind, App, ButtonVariant, CrossAlign, Key, Length, NodeId, Pointer, Round, Style,
    Surface, TextRole, Theme, Tree,
};

/// Nodes the tree can hold; the busiest page uses about 40.
pub const NODES: usize = 64;

/// How long the progress bar takes to fill, in milliseconds.
const FILL_MS: u32 = 8000;

/// The theme the showcase uses: Fusuma's neutral default, sized for a 7" 1024 x 600 touch screen (bigger
/// controls and spacing than the phone-sized defaults).
#[must_use]
pub const fn theme(dark: bool) -> Theme {
    let base = if dark { Theme::DARK } else { Theme::LIGHT };
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
            title: 20,
        },
        control_height: 56,
        indicator: 28,
        track_height: 14,
        ..base
    }
}

/// The pages in the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Buttons of both looks, enabled and disabled.
    Buttons,
    /// Checkboxes and radio buttons.
    Selection,
    /// A list to choose from.
    List,
    /// A progress bar that runs.
    Progress,
    /// A switch and a slider.
    Controls,
}

impl Page {
    /// Every page, in sidebar order.
    pub const ALL: [Self; 5] = [
        Self::Buttons,
        Self::Selection,
        Self::List,
        Self::Progress,
        Self::Controls,
    ];

    /// The name in the sidebar.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Buttons => "Buttons",
            Self::Selection => "Selection",
            Self::List => "List",
            Self::Progress => "Progress",
            Self::Controls => "Controls",
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
    counter: Option<NodeId>,
    click_me: Option<NodeId>,
    checks: [Option<NodeId>; 3],
    radios: [Option<NodeId>; 3],
    radio_label: Option<NodeId>,
    items: [Option<NodeId>; 6],
    list_label: Option<NodeId>,
    bar: Option<NodeId>,
    bar_label: Option<NodeId>,
    start: Option<NodeId>,
    reset: Option<NodeId>,
    switch: Option<NodeId>,
    slider: Option<NodeId>,
    slider_label: Option<NodeId>,
}

const RADIO_NAMES: [&str; 3] = ["Small", "Medium", "Large"];
const ITEM_NAMES: [&str; 6] = ["Aurora", "Birch", "Cedar", "Dune", "Ember", "Fjord"];

/// The showcase app.
#[derive(Debug)]
pub struct Showcase {
    tree: Tree<NODES>,
    theme: Theme,
    dark: bool,
    bounds: Rect,
    page: Page,
    sidebar: [Option<NodeId>; 5],
    body: Option<NodeId>,
    content: Option<NodeId>,
    theme_button: Option<NodeId>,
    status: Option<NodeId>,
    live: Live,
    clicks: u32,
    running: bool,
    fill_ms: u32,
    volume: u16,
}

impl Showcase {
    /// The app for a screen of `width` x `height` pixels, on the Buttons page, dark.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let mut s = Self {
            tree: Tree::new(),
            theme: theme(true),
            dark: true,
            bounds: Rect::new(0, 0, width, height),
            page: Page::Buttons,
            sidebar: [None; 5],
            body: None,
            content: None,
            theme_button: None,
            status: None,
            live: Live::default(),
            clicks: 0,
            running: false,
            fill_ms: 0,
            volume: 500,
        };
        // The tree has room for all of it; a build failure would leave a smaller screen, not a crash.
        let _ = s.build();
        s
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

    /// The button that switches the theme.
    #[must_use]
    pub const fn theme_button(&self) -> Option<NodeId> {
        self.theme_button
    }

    /// The widgets of the page on show, by role: for tests and demos that tap them.
    #[must_use]
    pub const fn widgets(&self) -> Widgets {
        Widgets {
            click_me: self.live.click_me,
            checks: self.live.checks,
            radios: self.live.radios,
            items: self.live.items,
            bar: self.live.bar,
            start: self.live.start,
            reset: self.live.reset,
            switch: self.live.switch,
            slider: self.live.slider,
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
        let t = &mut self.tree;
        let root = t.column(
            None,
            Style::new()
                .padding(self.theme.spacing.md)
                .gap(self.theme.spacing.md)
                .align(CrossAlign::Stretch)
                .background(Surface::Background),
        )?;
        let header = t.row(
            Some(root),
            Style::new()
                .gap(self.theme.spacing.md)
                .align(CrossAlign::Center),
        )?;
        t.label(Some(header), "Fusuma components", TextRole::Title)?;
        t.spacer(Some(header), Style::new().width(Length::Fill(1)))?;
        self.theme_button =
            Some(t.button(Some(header), "Light theme", ButtonVariant::Secondary)?);
        let body = t.row(
            Some(root),
            Style::new()
                .height(Length::Fill(1))
                .gap(self.theme.spacing.md)
                .align(CrossAlign::Stretch),
        )?;
        self.body = Some(body);
        let side = t.column(
            Some(body),
            Style::new()
                .width(Length::Fixed(240))
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
        let body = self.body;
        let permille = self.permille();
        let t = &mut self.tree;
        let c = t.column(
            body,
            Style::new()
                .width(Length::Fill(1))
                .padding(sp.md)
                .gap(sp.md)
                .align(CrossAlign::Stretch)
                .background(Surface::Surface)
                .round(Round::Medium),
        )?;
        self.content = Some(c);
        let live = &mut self.live;
        match self.page {
            Page::Buttons => {
                t.label(Some(c), "Buttons", TextRole::Title)?;
                let row = t.row(Some(c), Style::new().gap(sp.md))?;
                live.click_me = Some(t.button(Some(row), "Click me", ButtonVariant::Primary)?);
                t.button(Some(row), "Secondary", ButtonVariant::Secondary)?;
                let off = t.button(Some(row), "Disabled", ButtonVariant::Primary)?;
                t.set_enabled(off, false);
                let mut b = Buf::new();
                let _ = write!(b, "Clicked {} times", self.clicks);
                live.counter = Some(t.label(Some(c), b.text(), TextRole::Body)?);
                t.label(
                    Some(c),
                    "Hover or press to see the fade. Tab and Enter work too.",
                    TextRole::Caption,
                )?;
            }
            Page::Selection => {
                t.label(Some(c), "Selection", TextRole::Title)?;
                let cols = t.row(Some(c), Style::new().gap(sp.lg))?;
                let checks = t.column(Some(cols), Style::new().gap(sp.xs))?;
                for (slot, (name, on)) in live.checks.iter_mut().zip([
                    ("Notifications", true),
                    ("Location", false),
                    ("Analytics", false),
                ]) {
                    *slot = Some(t.checkbox(Some(checks), name, on)?);
                }
                if let Some(last) = live.checks[2] {
                    t.set_enabled(last, false);
                }
                let radios = t.column(Some(cols), Style::new().gap(sp.xs))?;
                for (i, slot) in live.radios.iter_mut().enumerate() {
                    *slot = Some(t.radio(Some(radios), RADIO_NAMES[i], i == 1)?);
                }
                live.radio_label = Some(t.label(Some(c), "Size: Medium", TextRole::Body)?);
            }
            Page::List => {
                t.label(Some(c), "List", TextRole::Title)?;
                let list = t.column(
                    Some(c),
                    Style::new()
                        .width(Length::Fixed(360))
                        .gap(sp.xs)
                        .align(CrossAlign::Stretch),
                )?;
                for (i, slot) in live.items.iter_mut().enumerate() {
                    *slot = Some(t.list_item(Some(list), ITEM_NAMES[i], i == 0)?);
                }
                live.list_label = Some(t.label(Some(c), "Chosen: Aurora", TextRole::Body)?);
            }
            Page::Controls => {
                t.label(Some(c), "Controls", TextRole::Title)?;
                live.switch = Some(t.switch(Some(c), "Wi-Fi", true)?);
                let mut b = Buf::new();
                let _ = write!(b, "Volume: {}%", self.volume / 10);
                live.slider_label = Some(t.label(Some(c), b.text(), TextRole::Body)?);
                let slider = t.slider(Some(c), self.volume)?;
                live.slider = Some(slider);
                t.label(
                    Some(c),
                    "Drag the slider, or focus it and use Left and Right.",
                    TextRole::Caption,
                )?;
            }
            Page::Progress => {
                t.label(Some(c), "Progress", TextRole::Title)?;
                live.bar = Some(t.progress_bar(Some(c), permille)?);
                let mut b = Buf::new();
                let _ = write!(b, "{}%", permille / 10);
                live.bar_label = Some(t.label(Some(c), b.text(), TextRole::Body)?);
                let row = t.row(Some(c), Style::new().gap(sp.md))?;
                live.start = Some(t.button(
                    Some(row),
                    if self.running { "Pause" } else { "Start" },
                    ButtonVariant::Primary,
                )?);
                live.reset = Some(t.button(Some(row), "Reset", ButtonVariant::Secondary)?);
            }
        }
        Ok(())
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
            let _ = self.build_page();
        }
    }

    fn set_dark(&mut self, dark: bool) {
        self.dark = dark;
        self.theme = theme(dark);
        if let Some(b) = self.theme_button {
            self.tree
                .set_text(b, if dark { "Light theme" } else { "Dark theme" });
        }
        self.tree.invalidate();
    }

    /// Reacts to what the widgets reported since the last call.
    fn handle_actions(&mut self) {
        while let Some(a) = self.tree.pop_action() {
            let id = Some(a.source);
            let mut b = Buf::new();
            match a.kind {
                ActionKind::Clicked => {
                    if id == self.theme_button {
                        self.set_dark(!self.dark);
                    } else if id == self.live.click_me {
                        self.clicks += 1;
                        let _ = write!(b, "Clicked {} times", self.clicks);
                        if let Some(l) = self.live.counter {
                            self.tree.set_text(l, b.text());
                        }
                    } else if id == self.live.start {
                        self.running = !self.running;
                        if self.running && self.fill_ms >= FILL_MS {
                            self.fill_ms = 0;
                        }
                        if let Some(s) = self.live.start {
                            self.tree
                                .set_text(s, if self.running { "Pause" } else { "Start" });
                        }
                    } else if id == self.live.reset {
                        self.running = false;
                        self.fill_ms = 0;
                        self.show_progress();
                        if let Some(s) = self.live.start {
                            self.tree.set_text(s, "Start");
                        }
                    }
                }
                ActionKind::Changed(v) => {
                    if id == self.live.slider {
                        self.volume = v;
                        let _ = write!(b, "Volume: {}%", v / 10);
                        if let Some(l) = self.live.slider_label {
                            self.tree.set_text(l, b.text());
                        }
                    }
                }
                ActionKind::Toggled(on) => {
                    let name = if id == self.live.switch {
                        Some("Wi-Fi")
                    } else {
                        self.live
                            .checks
                            .iter()
                            .position(|&c| c == id)
                            .and_then(|i| {
                                ["Notifications", "Location", "Analytics"].get(i).copied()
                            })
                    };
                    if let Some(name) = name {
                        let _ = write!(b, "{name} {}", if on { "on" } else { "off" });
                    }
                }
                ActionKind::Selected => {
                    if let Some(i) = self.sidebar.iter().position(|&s| s == id) {
                        self.set_page(Page::ALL[i]);
                        let _ = write!(b, "Page: {}", Page::ALL[i].name());
                    } else if let Some(i) = self.live.radios.iter().position(|&r| r == id) {
                        let _ = write!(b, "Size: {}", RADIO_NAMES[i]);
                        if let Some(l) = self.live.radio_label {
                            self.tree.set_text(l, b.text());
                        }
                    } else if let Some(i) = self.live.items.iter().position(|&r| r == id) {
                        let _ = write!(b, "Chosen: {}", ITEM_NAMES[i]);
                        if let Some(l) = self.live.list_label {
                            self.tree.set_text(l, b.text());
                        }
                    }
                }
            }
            if !b.text().is_empty() {
                let line = b;
                self.say(line.text());
            }
        }
    }

    fn show_progress(&mut self) {
        let permille = self.permille();
        if let Some(bar) = self.live.bar {
            self.tree.set_progress(bar, permille);
        }
        if let Some(l) = self.live.bar_label {
            let mut b = Buf::new();
            let _ = write!(b, "{}%", permille / 10);
            self.tree.set_text(l, b.text());
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

    /// Time passes by `dt_ms`: animations run and the progress bar fills while it is running. Returns
    /// whether anything wants a frame.
    pub fn tick(&mut self, dt_ms: u32) -> bool {
        if self.running {
            self.fill_ms = self.fill_ms.saturating_add(dt_ms);
            if self.fill_ms >= FILL_MS {
                self.fill_ms = FILL_MS;
                self.running = false;
                if let Some(s) = self.live.start {
                    self.tree.set_text(s, "Start");
                }
            }
            self.show_progress();
        }
        let moving = self.tree.advance(&self.theme, dt_ms);
        moving || self.tree.needs_frame()
    }

    /// Lays out the widgets without drawing, so a pointer event right after a change (a new page) finds
    /// them where they will be drawn. [`frame`](Self::frame) does this too.
    pub fn layout<R: Renderer>(&mut self, renderer: &R) {
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
        let Some(region) = self.tree.take_damage_in(self.bounds) else {
            return Ok(None);
        };
        renderer.save()?;
        let mut result = renderer.clip(region);
        if result.is_ok() {
            result = renderer.clear(self.theme.colors.background);
        }
        if result.is_ok() {
            result = self.tree.paint_region(renderer, &self.theme, region);
        }
        let restored = renderer.restore();
        result?;
        restored?;
        Ok(Some(region))
    }
}

impl App for Showcase {
    fn pointer(&mut self, event: Pointer) {
        Self::pointer(self, event);
    }

    fn key(&mut self, key: Key) {
        Self::key(self, key);
    }

    fn tick(&mut self, dt_ms: u32) -> bool {
        Self::tick(self, dt_ms)
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
    pub items: [Option<NodeId>; 6],
    /// The Progress page's bar.
    pub bar: Option<NodeId>,
    /// The Progress page's start/pause button.
    pub start: Option<NodeId>,
    /// The Progress page's reset button.
    pub reset: Option<NodeId>,
    /// The Controls page's switch.
    pub switch: Option<NodeId>,
    /// The Controls page's slider.
    pub slider: Option<NodeId>,
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma::render::{Point, Probe};

    fn app() -> (Showcase, fusuma::renderers::vello_cpu::VelloCpu) {
        let mut s = Showcase::new(1024, 600);
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
            s.tree().text(s.live.radio_label.unwrap()),
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
            s.tree().text(s.live.list_label.unwrap()),
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
        assert_eq!(s.tree().text(s.live.bar_label.unwrap()), Some("50%"));
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
        assert!(!s.is_dark());
        let region = s.frame(&mut r).unwrap().unwrap();
        assert_eq!(region, Rect::new(0, 0, 1024, 600));
        let light_bg = s.theme().colors.background;
        assert_ne!(dark_bg, light_bg);
        assert_eq!(r.pixel(0, 0).unwrap(), light_bg);
        assert_eq!(s.tree().text(s.theme_button().unwrap()), Some("Dark theme"));
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
        assert_eq!(s.tree().focused(), s.sidebar_item(Page::Buttons));
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
                    let target = s.theme_button().unwrap();
                    tap(&mut s, target);
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
            s.tree().text(s.live.slider_label.unwrap()),
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
}
