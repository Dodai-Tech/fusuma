//! The showcase's pages as views: each is a function of the app's state, built from the same widgets as before, and the
//! messages they send are the one place the app's state changes (`Showcase::page_message`).

use fusuma::Tone;
use fusuma::{
    Children, alert, avatar, badge, banner, breadcrumbs, button, card, checkbox, chip, column,
    divider, each, gauge, gestures, icon, item, menu, named, pagination, paragraph, progress,
    radio, row, scroll, section, segmented, slider, snackbar, stat_card, stepper, steps, switch,
    tab, tab_bar, tag, text, text_field, text_fmt, toast, wrap,
};
use fusuma::{CrossAlign, Id, Length, Swipe, SwipeDir};

/// The names the app gives the widgets the tests and the rest of the app look for.
pub mod wid {
    use fusuma::Id;

    pub const COUNTER: Id = Id::new(1);
    pub const CLICK_ME: Id = Id::new(2);
    pub const DELETE: Id = Id::new(3);
    pub const CHECK0: Id = Id::new(4);
    pub const CHECK1: Id = Id::new(5);
    pub const CHECK2: Id = Id::new(6);
    pub const RADIO0: Id = Id::new(7);
    pub const RADIO_LABEL: Id = Id::new(10);
    pub const LIST: Id = Id::new(11);
    pub const LIST_LABEL: Id = Id::new(12);
    pub const SWITCH: Id = Id::new(13);
    pub const SLIDER: Id = Id::new(14);
    pub const SLIDER_LABEL: Id = Id::new(15);
    pub const BAR: Id = Id::new(16);
    pub const BAR_LABEL: Id = Id::new(17);
    pub const START: Id = Id::new(18);
    pub const RESET: Id = Id::new(19);
    pub const FIELD: Id = Id::new(20);
    pub const ECHO: Id = Id::new(21);
    pub const TAB0: Id = Id::new(22);
    pub const TAB_LABEL: Id = Id::new(25);
    pub const MENU_BUTTON: Id = Id::new(26);
    pub const MENU: Id = Id::new(27);
    pub const MENU_ITEM0: Id = Id::new(28);
    pub const TOAST: Id = Id::new(31);
    pub const CARD: Id = Id::new(32);
    pub const CARD_LABEL: Id = Id::new(33);
    pub const READING: Id = Id::new(34);
    pub const READING_TEXT: Id = Id::new(35);
    /// The first of the twelve rows of the list.
    pub const ITEM0: Id = Id::new(40);
}

/// What the pages send.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageMsg {
    /// The primary button of the Buttons page.
    Click,
    /// The Delete button: ask for confirmation.
    AskDelete,
    /// The Notifications checkbox.
    Notify(bool),
    /// The Location checkbox.
    Location(bool),
    /// A radio button of the size group.
    Size(u8),
    /// A row of the list.
    Pick(u8),
    /// The Wi-Fi switch.
    Wifi(bool),
    /// The volume slider, in thousandths.
    Volume(u16),
    /// Start or pause the progress bar.
    StartStop,
    /// Reset the progress bar.
    Reset,
    /// The text field changed (the app reads the text from the field).
    Typed,
    /// Enter was pressed in the text field.
    Submit,
    /// A tab was chosen.
    Tab(u8),
    /// The Menu button.
    MenuOpen,
    /// A row of the menu was pressed.
    MenuChoice(u8),
    /// The menu was dismissed.
    MenuDismiss,
    /// The card was swiped.
    Swiped(SwipeDir),
    /// The card was held.
    Held,
    /// Fewer seats.
    Fewer,
    /// More seats.
    More,
    /// A segment of the segmented control.
    Segment(u8),
    /// A page of the pagination.
    PageTo(u32),
    /// A chip.
    Chip(u8),
    /// A step is chosen (the Components page moves on to it).
    StepTo(u8),
    /// A part of the breadcrumbs.
    Crumb(u8),
    /// The snackbar's action.
    Undo,
}

/// How a page is laid out on the screen it is on.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// A narrow screen stacks things.
    pub compact: bool,
    /// A watch leaves out the title and the extras.
    pub lite: bool,
    /// A tiny panel (128 px wide): what sits side by side is stacked, and the words are short.
    pub micro: bool,
    /// The spacings.
    pub xs: u32,
    /// Small.
    pub sm: u32,
    /// Medium.
    pub md: u32,
    /// Large.
    pub lg: u32,
    /// The width of the list on a wide screen.
    pub list_w: u32,
    /// The height of the list.
    pub list_h: u32,
    /// The side of the avatar.
    pub avatar: u32,
    /// The height of the reading view.
    pub reading_h: u32,
}

/// The segments, the chips, the breadcrumbs and the steps of the Components page, and how many pages it pretends to have.
pub const SEGMENT_NAMES: [&str; 3] = ["Day", "Week", "Month"];
pub const CHIP_NAMES: [&str; 3] = ["Wi-Fi", "Bluetooth", "GPS"];
pub const CRUMB_NAMES: [&str; 3] = ["Home", "Settings", "Display"];
pub const STEP_NAMES: [&str; 3] = ["Account", "Details", "Done"];
pub const PAGES: u32 = 24;

/// What the pages show: the app's state, borrowed.
#[derive(Clone, Copy, Debug)]
pub struct State<'a> {
    pub clicks: u32,
    pub notify: bool,
    pub location: bool,
    pub size: u8,
    pub chosen: u8,
    pub wifi: bool,
    pub volume: u16,
    pub permille: u16,
    pub running: bool,
    pub typed: &'a str,
    pub tab: u8,
    pub menu_open: bool,
    pub swipes: u32,
    pub swiped: Option<SwipeDir>,
    pub quantity: u32,
    pub segment: u8,
    pub page_no: u32,
    pub step: u8,
    pub chips: [bool; 3],
    pub icons: Icons,
}

/// The icon characters the pages use, when the build has the font (a `char` is `None` where it does not).
#[derive(Clone, Copy, Debug, Default)]
pub struct Icons {
    pub home: Option<char>,
    pub search: Option<char>,
    pub settings: Option<char>,
    pub more: Option<char>,
    pub share: Option<char>,
    pub edit: Option<char>,
    pub delete: Option<char>,
    /// The Typography page's row.
    pub row: [Option<char>; 9],
}

pub const RADIO_NAMES: [&str; 3] = ["Small", "Medium", "Large"];
pub const TAB_NAMES: [&str; 3] = ["Home", "Search", "Settings"];
pub const MENU_NAMES: [&str; 3] = ["Share", "Edit", "Delete"];
pub const ITEM_NAMES: [&str; 12] = [
    "Aurora", "Birch", "Cedar", "Dune", "Ember", "Fjord", "Glacier", "Harbor", "Island", "Juniper",
    "Kelp", "Lagoon",
];
/// The rows of the list, each with the key it is found by.
const ROWS: [(u32, &str); 12] = [
    (0, "Aurora"),
    (1, "Birch"),
    (2, "Cedar"),
    (3, "Dune"),
    (4, "Ember"),
    (5, "Fjord"),
    (6, "Glacier"),
    (7, "Harbor"),
    (8, "Island"),
    (9, "Juniper"),
    (10, "Kelp"),
    (11, "Lagoon"),
];
/// What the Reading page shows: long enough to wrap onto many lines and scroll.
pub const READING: &str = "Fusuma draws the same widgets on a watch, a phone and a desktop. Text wraps to the room it has: on a watch it takes many short lines, on a desktop a few long ones, and a text view scrolls when there are more lines than fit. A word that is too long for a line is broken where it no longer fits, and a newline always starts a new line.\nDrag this text, turn the wheel, or move the focus.\nThe lines are worked out with the renderer's own measuring, so the same words break at the same places on the integer renderer and on Vello. Nothing here is a web page: it is a retained tree of widgets, drawn by a renderer that fits in a few kilobytes.";

fn name_of(base: Id, i: usize) -> Id {
    Id::new(base.0 + u16::try_from(i).unwrap_or(0))
}

/// The Overview page.
pub fn overview<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let seats = |s: State<'_>| s.quantity;
    let who = card((
        row((
            avatar("AM", look.avatar),
            column((text("Ana Maria"), text("Designer").caption())),
        ))
        .gap(look.md)
        .align(CrossAlign::Center),
        divider(),
        row((badge("Pro", true), badge("Team", false))).gap(look.sm),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let seats_card = card((
        section("Seats"),
        stepper(seats_text(seats(s)).as_str())
            .on_dec(PageMsg::Fewer)
            .on_inc(PageMsg::More),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let usage = (!look.lite).then(|| {
        card((section("Storage"), progress(640), text("6.4 of 10 GB")))
            .padding(look.md)
            .gap(look.sm)
            .width(Length::Fill(1))
    });
    let body = (who, seats_card, usage);
    let cards = if look.compact {
        column(body).gap(look.md)
    } else {
        row(body).gap(look.md)
    };
    (
        (!look.lite).then(|| text("Overview").title()),
        cards,
        (!look.lite).then(|| section("Status")),
        (!look.lite).then(|| {
            row((
                badge("Online", true),
                badge("Synced", false),
                badge("3 new", false),
                badge("v0.2", false),
            ))
            .gap(look.sm)
        }),
    )
}

/// A number as text, held on the stack until the view that shows it is made.
fn seats_text(n: u32) -> fusuma::SmallText {
    let mut t = fusuma::SmallText::EMPTY;
    let _ = core::fmt::Write::write_fmt(&mut t, format_args!("{n}"));
    t
}

/// `children` side by side, wrapping onto the next line where the screen is too narrow (a tiny panel stacks them).
fn side_by_side<'a, C: Children<PageMsg> + 'a>(
    look: Look,
    children: C,
) -> impl fusuma::View<PageMsg> + 'a {
    wrap(children).width(Length::Fill(1)).gap(look.sm)
}

/// The Components page: messages, a segmented control, chips, breadcrumbs, pagination and steps. A tiny panel gets the
/// few that fit.
pub fn components<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let mut shown = fusuma::SmallText::EMPTY;
    let _ = core::fmt::Write::write_fmt(&mut shown, format_args!("{}", s.page_no));
    let chips = side_by_side(
        look,
        (
            chip(CHIP_NAMES[0], s.chips[0]).on_press(PageMsg::Chip(0)),
            chip(CHIP_NAMES[1], s.chips[1]).on_press(PageMsg::Chip(1)),
            chip(CHIP_NAMES[2], s.chips[2]).on_press(PageMsg::Chip(2)),
        ),
    );
    (
        (!look.lite).then(|| text("Components").title()),
        column((
            alert(Tone::Warning, "Low battery", "12 percent left"),
            banner(Tone::Success, "Saved"),
            row((tag(Tone::Info, "New"), tag(Tone::Danger, "Offline"))).gap(look.sm),
        ))
        .gap(look.sm)
        .align(CrossAlign::Stretch),
        row((
            gauge(
                u16::try_from(s.page_no * 1000 / PAGES).unwrap_or(1000),
                shown.as_str(),
            )
            .size(if look.micro { 48 } else { 72 }),
            column((
                text("Gauge").caption(),
                text("The page number below").caption(),
            ))
            .gap(2),
        ))
        .gap(look.md)
        .align(CrossAlign::Center),
        segmented(&SEGMENT_NAMES, usize::from(s.segment), |i| {
            PageMsg::Segment(i as u8)
        }),
        chips,
        (!look.lite).then(|| breadcrumbs(&CRUMB_NAMES, |i| PageMsg::Crumb(i as u8))),
        (!look.lite).then(|| pagination(s.page_no, PAGES, PageMsg::PageTo)),
        steps(&STEP_NAMES, usize::from(s.step)),
        button("Next step").on_press(PageMsg::StepTo((s.step + 1) % 3)),
        (!look.lite).then(|| stat_card("Steps today", "8,412", "+12%", Tone::Success)),
        snackbar("Message deleted", "Undo", PageMsg::Undo),
    )
}

/// The Buttons page.
pub fn buttons<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    (
        (!look.lite).then(|| text("Buttons").title()),
        side_by_side(
            look,
            (
                named(
                    wid::CLICK_ME,
                    button("Click me").primary().on_press(PageMsg::Click),
                ),
                named(
                    wid::DELETE,
                    button("Delete...").on_press(PageMsg::AskDelete),
                ),
                (!look.lite).then(|| button("Disabled").primary().enabled(false)),
            ),
        ),
        named(
            wid::COUNTER,
            if look.micro {
                text_fmt(format_args!("Clicked {}", s.clicks))
            } else {
                text_fmt(format_args!("Clicked {} times", s.clicks))
            },
        ),
        (!look.lite)
            .then(|| text("Hover or press to see the fade. Tab and Enter work too.").caption()),
    )
}

/// The Selection page.
pub fn selection<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let checks = column((
        named(
            wid::CHECK0,
            checkbox("Notifications", s.notify).on_toggle(PageMsg::Notify),
        ),
        named(
            wid::CHECK1,
            checkbox("Location", s.location).on_toggle(PageMsg::Location),
        ),
        (!look.lite).then(|| named(wid::CHECK2, checkbox("Analytics", false).enabled(false))),
    ))
    .gap(look.xs);
    let radios = column((
        named(
            wid::RADIO0,
            radio(RADIO_NAMES[0], s.size == 0).on_select(PageMsg::Size(0)),
        ),
        named(
            name_of(wid::RADIO0, 1),
            radio(RADIO_NAMES[1], s.size == 1).on_select(PageMsg::Size(1)),
        ),
        named(
            name_of(wid::RADIO0, 2),
            radio(RADIO_NAMES[2], s.size == 2).on_select(PageMsg::Size(2)),
        ),
    ))
    .gap(look.xs);
    let cols = if look.compact {
        column((checks, radios)).gap(look.md)
    } else {
        row((checks, radios)).gap(look.lg)
    };
    (
        (!look.lite).then(|| text("Selection").title()),
        cols,
        named(
            wid::RADIO_LABEL,
            text_fmt(format_args!(
                "Size: {}",
                RADIO_NAMES[usize::from(s.size).min(2)]
            )),
        ),
    )
}

/// The List page.
pub fn list<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    (
        (!look.lite).then(|| text("List").title()),
        named(
            wid::LIST,
            scroll((each(
                &ROWS,
                |r| r.0,
                move |r| {
                    let i = r.0 as usize;
                    named(
                        name_of(wid::ITEM0, i),
                        item(r.1, usize::from(s.chosen) == i).on_select(PageMsg::Pick(r.0 as u8)),
                    )
                },
            ),))
            .width(if look.compact {
                Length::Fill(1)
            } else {
                Length::Fixed(look.list_w)
            })
            .height(Length::Fixed(look.list_h))
            .gap(look.xs)
            .align(CrossAlign::Stretch),
        ),
        named(
            wid::LIST_LABEL,
            text_fmt(format_args!(
                "Chosen: {}",
                ITEM_NAMES[usize::from(s.chosen).min(11)]
            )),
        ),
    )
}

/// The Controls page.
pub fn controls<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    (
        (!look.lite).then(|| text("Controls").title()),
        named(
            wid::SWITCH,
            switch("Wi-Fi", s.wifi).on_toggle(PageMsg::Wifi),
        ),
        named(
            wid::SLIDER_LABEL,
            text_fmt(format_args!("Volume: {}%", s.volume / 10)),
        ),
        named(wid::SLIDER, slider(s.volume).on_change(PageMsg::Volume)),
        (!look.lite)
            .then(|| text("Drag the slider, or focus it and use Left and Right.").caption()),
    )
}

/// The Progress page.
pub fn progress_page<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    (
        (!look.lite).then(|| text("Progress").title()),
        named(wid::BAR, progress(s.permille)),
        named(
            wid::BAR_LABEL,
            text_fmt(format_args!("{}%", s.permille / 10)),
        ),
        side_by_side(
            look,
            (
                named(
                    wid::START,
                    button(if s.running { "Pause" } else { "Start" })
                        .primary()
                        .on_press(PageMsg::StartStop),
                ),
                named(wid::RESET, button("Reset").on_press(PageMsg::Reset)),
            ),
        ),
    )
}

/// The Text page.
pub fn text_page<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    (
        (!look.lite).then(|| text("Text").title()),
        named(
            wid::FIELD,
            text_field(s.typed)
                .on_input(|_| PageMsg::Typed)
                .on_submit(PageMsg::Submit),
        ),
        named(wid::ECHO, text_fmt(format_args!("You typed: {}", s.typed))),
        (!look.lite).then(|| {
            text("Type, select with Shift+arrows, copy and paste, or use an input method. Enter submits.")
                .caption()
        }),
    )
}

/// The Typography page.
pub fn typography<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    // The icons the build has, in order, as a list with a key each.
    let mut icons = [(0u32, ' '); 9];
    let mut n = 0;
    for (k, c) in s.icons.row.iter().enumerate() {
        if let Some(c) = *c {
            icons[n] = (u32::try_from(k).unwrap_or(0), c);
            n += 1;
        }
    }
    let _ = &icons;
    (
        (!look.lite).then(|| text("Typography").title()),
        TypographyIcons(icons, n, look.sm),
        (!look.lite).then(|| text("Inter Bold, for titles and emphasis").title()),
        text("Inter Regular: the quick brown fox jumps over the lazy dog."),
        text("Romanian: ăâîșț ĂÂÎȘȚ, and more: éüñß €"),
        text("let x = 42; // JetBrains Mono, fixed width").code(),
        (!look.lite).then(|| text("With no font bundled, text is the bitmap font.").caption()),
    )
}

/// The Navigation page.
pub fn navigation<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let tab_of = move |i: usize, ic: Option<char>| {
        let t = tab(TAB_NAMES[i], usize::from(s.tab) == i)
            .on_select(PageMsg::Tab(u8::try_from(i).unwrap_or(0)));
        named(
            name_of(wid::TAB0, i),
            match ic {
                Some(c) => t.icon(c),
                None => t,
            },
        )
    };
    let menu_row = move |i: usize, ic: Option<char>| {
        let it =
            item(MENU_NAMES[i], false).on_select(PageMsg::MenuChoice(u8::try_from(i).unwrap_or(0)));
        named(
            name_of(wid::MENU_ITEM0, i),
            match ic {
                Some(c) => it.icon(c),
                None => it,
            },
        )
    };
    let menu_button = {
        let b = button("Menu").on_press(PageMsg::MenuOpen);
        named(
            wid::MENU_BUTTON,
            match s.icons.more {
                Some(c) => b.icon(c),
                None => b,
            },
        )
    };
    (
        (!look.lite).then(|| text("Navigation").title()),
        tab_bar((
            tab_of(0, s.icons.home),
            tab_of(1, s.icons.search),
            tab_of(2, s.icons.settings),
        )),
        named(
            wid::TAB_LABEL,
            text_fmt(format_args!("{} tab", TAB_NAMES[usize::from(s.tab).min(2)])),
        ),
        row((menu_button,)).gap(look.md),
        named(
            wid::MENU,
            menu(
                s.menu_open,
                wid::MENU_BUTTON,
                (
                    menu_row(0, s.icons.share),
                    menu_row(1, s.icons.edit),
                    menu_row(2, s.icons.delete),
                ),
            )
            .padding(look.xs)
            .gap(look.xs)
            .align(CrossAlign::Stretch)
            .on_dismiss(PageMsg::MenuDismiss),
        ),
        named(
            wid::CARD,
            gestures(
                row((named(
                    wid::CARD_LABEL,
                    match s.swiped {
                        Some(d) => text_fmt(format_args!("Swiped {d:?} ({})", s.swipes)),
                        None => text_fmt(format_args!("Swipe or hold this card ({})", s.swipes)),
                    },
                ),))
                .padding(look.md)
                .background(fusuma::Surface::Raised)
                .round(fusuma::Round::Medium),
            )
            .on_long_press(PageMsg::Held)
            .on_swipe(Swipe::Horizontal, PageMsg::Swiped),
        ),
        named(wid::TOAST, toast()),
    )
}

/// The Reading page.
pub fn reading<'a>(look: Look, _: State<'a>) -> impl Children<PageMsg> + 'a {
    (
        (!look.lite).then(|| text("Reading").title()),
        named(
            wid::READING,
            scroll((named(wid::READING_TEXT, paragraph(READING)),))
                .height(Length::Fixed(look.reading_h))
                .align(CrossAlign::Stretch),
        ),
    )
}

/// The row of icons of the Typography page: as many as the font has.
#[derive(Clone, Copy, Debug)]
struct TypographyIcons([(u32, char); 9], usize, u32);

impl fusuma::ViewMarker for TypographyIcons {}

impl fusuma::View<PageMsg> for TypographyIcons {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut fusuma::Cx<'_, PageMsg, N, K>,
        parent: Option<fusuma::NodeId>,
    ) -> Result<fusuma::NodeId, fusuma::BuildError> {
        row((each(&self.0[..self.1], |i| i.0, |i| icon(i.1)),))
            .gap(self.2)
            .build(cx, parent)
    }

    fn rebuild<const N: usize, const K: usize>(
        &self,
        cx: &mut fusuma::Cx<'_, PageMsg, N, K>,
        node: fusuma::NodeId,
    ) {
        row((each(&self.0[..self.1], |i| i.0, |i| icon(i.1)),))
            .gap(self.2)
            .rebuild(cx, node);
    }
}
