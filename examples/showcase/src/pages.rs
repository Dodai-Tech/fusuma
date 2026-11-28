//! The showcase's pages as views: each is a function of the app's state, built from the same widgets as before, and the
//! messages they send are the one place the app's state changes (`Showcase::page_message`).

use fusuma::Tone;
use fusuma::{
    Children, accordion, alert, area_chart, avatar, badge, banner, breadcrumbs, button, card,
    checkbox, chip, column, divider, each, gauge, gestures, icon, item, menu, named, pagination,
    paragraph, progress, radio, row, scroll, section, segmented, slider, snackbar, stat_card,
    stepper, steps, switch, tab, tab_bar, tag, text, text_field, text_fmt, toast, wrap,
};
use fusuma::{CrossAlign, Id, Length, Round, Surface, Swipe, SwipeDir};
use fusuma::{
    avatar_group, button_group, chip_group, fab, form_field, icon_badge, key_value, marks,
    navigation_bar, search_bar, split_button, toggle_group, toolbar, top_bar,
};
use fusuma::{
    bar_chart, calendar, carousel, kbd, line_chart, rating, select, show, spinner, split_view,
    table, time_picker, timeline, tree_item,
};

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
    /// The split view's bar moved to this many thousandths.
    Split(u16),
    /// A day of the calendar.
    Day(u8),
    /// The calendar's arrows: a month back or on.
    MonthBy(i32),
    /// A step of the time picker.
    TimeStep(fusuma::TimePart, i32),
    /// The select's button, an option of it, or a press outside its menu.
    SelectToggle,
    /// An option of the select.
    SelectPick(usize),
    /// A press outside the select's menu.
    SelectClose,
    /// A star of the rating.
    Rate(u8),
    /// A place of the navigation bar or rail.
    BarNav(u8),
    /// A part of the toggle group.
    BarToggle(u8),
    /// A chip of the chip group.
    BarChip(u8),
    /// A part of the button group.
    BarAct(u8),
    /// The split button's main part, and its arrow.
    BarMain,
    /// The split button's arrow.
    BarMore,
    /// The floating action button: one more person in the group.
    BarAdd,
    /// The search bar's text changed, or Enter was pressed in it.
    BarSearch,
    /// Enter in the search bar.
    BarSubmit,
    /// A page of the carousel.
    Slide(usize),
    /// The accordion's heading.
    Fold,
    /// A tree row's triangle (the folder) or the row itself.
    TreeToggle,
    /// A row of the tree.
    TreeSelect(u8),
}

/// What the Data and Pickers pages keep: the split, the date and time, the select, the rating, the carousel, the accordion and
/// the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extra {
    pub split: u16,
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub size: Option<usize>,
    pub size_open: bool,
    pub rating: u8,
    pub slide: usize,
    pub fold: bool,
    pub tree_open: bool,
    pub tree_row: u8,
    pub bar_nav: u8,
    pub toggles: [bool; 3],
    pub filters: [bool; 4],
    pub people: usize,
}

impl Extra {
    pub const START: Self = Self {
        split: 400,
        year: 2026,
        month: 11,
        day: 26,
        hour: 9,
        minute: 30,
        size: Some(1),
        size_open: false,
        rating: 3,
        slide: 0,
        fold: false,
        tree_open: true,
        tree_row: 1,
        bar_nav: 0,
        toggles: [false, true, false],
        filters: [true, false, false, true],
        people: 5,
    };
}

/// The names on the Bars page.
pub const BAR_NAV: [&str; 3] = ["Home", "Cars", "Me"];
pub const BAR_TOGGLES: [&str; 3] = ["Bold", "Italic", "Underline"];
pub const BAR_CHIPS: [&str; 4] = ["Wi-Fi", "Bluetooth", "NFC", "GPS"];
pub const BAR_ACTIONS: [&str; 3] = ["Cut", "Copy", "Paste"];
pub const PEOPLE: [&str; 5] = ["Ana Maria", "Bob Stone", "Cleo Fox", "Dan Hill", "Eve Moon"];
/// The sizes the select offers.
pub const SIZES: [&str; 3] = ["Small", "Medium", "Large"];
/// A week of readings for the charts.
pub const READINGS: [i32; 12] = [4, 7, 5, 9, 12, 8, 6, 10, 14, 11, 9, 13];
/// The rows of the table.
pub const TABLE_HEAD: [&str; 3] = ["Item", "Qty", "Price"];
const ROW1: [&str; 3] = ["Bolt", "10", "0.50"];
const ROW2: [&str; 3] = ["Nut", "25", "0.20"];
const ROW3: [&str; 3] = ["Washer", "7", "0.10"];
pub const TABLE_ROWS: [&[&str]; 3] = [&ROW1, &ROW2, &ROW3];
/// The steps of the order timeline.
pub const TIMELINE: [(&str, &str); 3] =
    [("Ordered", "Mon"), ("Shipped", "Tue"), ("Delivered", "Thu")];
/// The carousel's pages.
pub const SLIDES: [&str; 3] = ["Welcome", "Pick a theme", "All set"];

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
    pub x: Extra,
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
    /// The row of icons on the Foundations page.
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

/// A filled square with its name under it: a colour or a corner of the theme, as it is drawn.
fn swatch<'a>(
    look: Look,
    name: &'a str,
    fill: Surface,
    round: Round,
) -> impl fusuma::View<PageMsg> + 'a {
    let side = if look.lite { 28 } else { 44 };
    column((
        row(())
            .background(fill)
            .round(round)
            .width(Length::Fixed(side))
            .height(Length::Fixed(side)),
        text(name).caption(),
    ))
    .gap(look.xs)
    .align(CrossAlign::Center)
}

/// A bar as wide as a spacing step, with its name and size under it.
fn gap_bar<'a>(look: Look, name: &'a str, px: u32) -> impl fusuma::View<PageMsg> + 'a {
    column((
        row(())
            .background(Surface::Primary)
            .round(Round::Small)
            .width(Length::Fixed(px.max(2)))
            .height(Length::Fixed(12)),
        text_fmt(format_args!("{name} {px}")).caption(),
    ))
    .gap(look.xs)
    .align(CrossAlign::Start)
}

/// The Foundations page: what every widget is made from, drawn from the theme in force (the colours, the corners and the
/// type), so it changes with the design and the palette.
pub fn foundations<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let colours = card((
        section("Colour"),
        wrap((
            swatch(look, "Page", Surface::Background, Round::Small),
            swatch(look, "Card", Surface::Surface, Round::Small),
            swatch(look, "Raised", Surface::Raised, Round::Small),
            swatch(look, "Border", Surface::Border, Round::Small),
            swatch(look, "Primary", Surface::Primary, Round::Small),
        ))
        .width(Length::Fill(1))
        .gap(look.md),
        (!look.micro).then(|| section("Tones")),
        (!look.micro).then(|| {
            wrap((
                swatch(look, "Info", Surface::Tone(Tone::Info), Round::Small),
                swatch(look, "Success", Surface::Tone(Tone::Success), Round::Small),
                swatch(look, "Warning", Surface::Tone(Tone::Warning), Round::Small),
                swatch(look, "Danger", Surface::Tone(Tone::Danger), Round::Small),
            ))
            .width(Length::Fill(1))
            .gap(look.md)
        }),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let shape = card((
        section("Corners"),
        wrap((
            swatch(look, "None", Surface::Border, Round::None),
            swatch(look, "Small", Surface::Border, Round::Small),
            swatch(look, "Medium", Surface::Border, Round::Medium),
            swatch(look, "Large", Surface::Border, Round::Large),
            swatch(look, "Full", Surface::Border, Round::Full),
        ))
        .width(Length::Fill(1))
        .gap(look.md),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let type_scale = card((
        section("Type"),
        text("Title").title(),
        text("Body: the quick brown fox"),
        text("Caption: the quiet text under things").caption(),
        text("Code: let x = 42;").code(),
    ))
    .padding(look.md)
    .gap(look.xs)
    .width(Length::Fill(1));
    let space = card((
        section("Spacing"),
        row((
            gap_bar(look, "xs", look.xs),
            gap_bar(look, "sm", look.sm),
            gap_bar(look, "md", look.md),
            gap_bar(look, "lg", look.lg),
        ))
        .gap(look.md)
        .align(CrossAlign::End),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    // The icons the build has, in order, as a list with a key each.
    let mut icons = [(0u32, ' '); 9];
    let mut n = 0;
    for (k, c) in s.icons.row.iter().enumerate() {
        if let Some(c) = *c {
            icons[n] = (u32::try_from(k).unwrap_or(0), c);
            n += 1;
        }
    }
    let icon_card = (n > 0).then(|| {
        card((
            section("Icons"),
            IconRow(icons, n, look.md),
            text("Font icons: one colour, any size, drawn like text.").caption(),
        ))
        .padding(look.md)
        .gap(look.sm)
        .width(Length::Fill(1))
    });
    (
        (!look.lite).then(|| text("Foundations").title()),
        (!look.lite).then(|| text("What every widget is made from.").caption()),
        colours,
        (!look.lite).then_some(shape),
        (!look.lite).then_some(space),
        (!look.lite).then_some(type_scale),
        icon_card,
    )
}

/// The Examples page: one account screen put together from the widgets the other pages show one by one.
pub fn overview<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let profile = card((
        row((
            avatar("AM", look.avatar),
            column((text("Ana Maria"), text("Designer, Porto").caption())),
        ))
        .gap(look.md)
        .align(CrossAlign::Center),
        divider(),
        row((
            badge("Pro", true),
            badge("Team", false),
            badge("Online", false),
        ))
        .gap(look.sm),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let plan = card((
        section("Plan"),
        row((
            text("Seats"),
            stepper(seats_text(s.quantity).as_str())
                .on_dec(PageMsg::Fewer)
                .on_inc(PageMsg::More),
        ))
        .gap(look.md)
        .align(CrossAlign::Center),
        (!look.lite).then(divider),
        (!look.lite).then(|| section("Storage")),
        (!look.lite).then(|| progress(640)),
        (!look.lite).then(|| text("6.4 of 10 GB used").caption()),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let body = (profile, plan);
    let cards = if look.compact {
        column(body).gap(look.md)
    } else {
        row(body).gap(look.md).align(CrossAlign::Stretch)
    };
    (
        (!look.lite).then(|| text("Examples").title()),
        (!look.lite)
            .then(|| text("An account screen made from the widgets on the other pages.").caption()),
        cards,
        (!look.lite).then(|| stat_card("Steps today", "8,412", "+12%", Tone::Success)),
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

/// The Data page: numbers and how they change.
pub fn data<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let (w, h) = if look.micro {
        (96, 48)
    } else if look.lite {
        (180, 70)
    } else {
        (260, 90)
    };
    (
        (!look.lite).then(|| text("Data").title()),
        stat_card("Steps today", "8,412", "+12%", Tone::Success),
        column((
            line_chart(&READINGS).size(w, h),
            area_chart(&READINGS).size(w, h),
            bar_chart(&READINGS).range(&READINGS, 0, 16).size(w, h),
        ))
        .gap(look.sm),
        table(&TABLE_HEAD, &TABLE_ROWS),
        timeline(&TIMELINE, 1),
        carousel(
            s.x.slide,
            SLIDES.len(),
            PageMsg::Slide,
            text(SLIDES[s.x.slide.min(SLIDES.len() - 1)]),
        ),
        row((rating(s.x.rating, PageMsg::Rate), spinner().size(24)))
            .gap(look.md)
            .align(CrossAlign::Center),
        row((text("Press").caption(), kbd("Ctrl"), kbd("K")))
            .gap(look.sm)
            .align(CrossAlign::Center),
    )
}

/// The Pickers page: a date, a time, a choice, a tree, a fold and a split view.
pub fn pickers<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let x = s.x;
    (
        (!look.lite).then(|| text("Pickers").title()),
        calendar(x.year, x.month, Some(x.day), PageMsg::Day, PageMsg::MonthBy),
        time_picker(x.hour, x.minute, PageMsg::TimeStep),
        select(
            &SIZES,
            x.size,
            x.size_open,
            PageMsg::SelectToggle,
            PageMsg::SelectPick,
            PageMsg::SelectClose,
        )
        .placeholder("Size"),
        column((
            tree_item(
                "src",
                0,
                Some(x.tree_open),
                x.tree_row == 0,
                PageMsg::TreeToggle,
                PageMsg::TreeSelect(0),
            ),
            show(
                x.tree_open,
                column((
                    tree_item(
                        "lib.rs",
                        1,
                        None,
                        x.tree_row == 1,
                        PageMsg::TreeToggle,
                        PageMsg::TreeSelect(1),
                    ),
                    tree_item(
                        "main.rs",
                        1,
                        None,
                        x.tree_row == 2,
                        PageMsg::TreeToggle,
                        PageMsg::TreeSelect(2),
                    ),
                )),
            ),
        )),
        accordion(
            "More",
            x.fold,
            PageMsg::Fold,
            (text("Hidden until opened"),),
        ),
        split_view(
            x.split,
            PageMsg::Split,
            (text("Left").caption(),),
            (text("Right").caption(),),
        ),
    )
}

/// The Bars page: the bars, groups and fields that frame a screen.
pub fn bars<'a>(look: Look, s: State<'a>) -> impl Children<PageMsg> + 'a {
    let x = s.x;
    let nav = 1u32 << x.bar_nav.min(2);
    // A heading and the parts of the page, in groups: a tuple holds 12 at most.
    (
        (!look.lite).then(|| text("Bars").title()),
        top_bar("Garage", (button("Edit"),)),
        toolbar((
            toggle_group(&BAR_TOGGLES, marks(&x.toggles), |i| {
                PageMsg::BarToggle(i as u8)
            }),
            button_group(&BAR_ACTIONS, |i| PageMsg::BarAct(i as u8)),
        )),
        search_bar(
            s.icons.search.unwrap_or(' '),
            s.typed,
            |_| PageMsg::BarSearch,
            PageMsg::BarSubmit,
        ),
        form_field("Name", text_field("Ana Maria"), "As on the card", ""),
        key_value("Range", "412 km"),
        chip_group(&BAR_CHIPS, marks(&x.filters), |i| PageMsg::BarChip(i as u8)),
        row((
            split_button("Charge", PageMsg::BarMain, PageMsg::BarMore),
            fab(s.icons.edit.unwrap_or('+'), "Add").on_press(PageMsg::BarAdd),
            icon_badge(s.icons.home.unwrap_or('i'), 3),
        ))
        .gap(look.md),
        avatar_group(&PEOPLE[..x.people], look.avatar.max(24), 3),
        navigation_bar(&BAR_NAV, nav, |i| PageMsg::BarNav(i as u8)),
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
        (!look.lite).then(|| section("Linear")),
        named(wid::BAR, progress(s.permille)),
        named(
            wid::BAR_LABEL,
            text_fmt(format_args!("{}%", s.permille / 10)),
        ),
        (!look.lite).then(|| section("Tall, the same value")),
        (!look.lite).then(|| progress(s.permille).tall(80)),
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
pub fn typography<'a>(look: Look, _s: State<'a>) -> impl Children<PageMsg> + 'a {
    (
        (!look.lite).then(|| text("Typography").title()),
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
struct IconRow([(u32, char); 9], usize, u32);

impl fusuma::ViewMarker for IconRow {}

impl fusuma::View<PageMsg> for IconRow {
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
