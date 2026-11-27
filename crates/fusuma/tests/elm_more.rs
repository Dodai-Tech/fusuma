#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The second set of components made of views (top bar, toolbar, form field, key and value, icon badge, navigation bar and
//! rail, toggle, chip and button groups, split button, floating action button, search bar, avatar group): what they build,
//! which message each part sends, that a changed count or label reaches the tree, and that a pass with nothing changed costs
//! no allocation.

use allocation_counter::measure;
use fusuma::{
    Host, View, avatar_group, button, button_group, chip_group, fab, form_field, icon_badge,
    key_value, marks, navigation_bar, navigation_rail, search_bar, split_button, text, text_field,
    toggle_group, toolbar, top_bar,
};
use fusuma_render::{Point, Rect};
use fusuma_render_integer::{Embedded, Frame};
use fusuma_ui::{NodeId, Pointer, Style, Theme, Tree};

const W: u16 = 400;
const H: u16 = 900;
const BOUNDS: Rect = Rect::new(0, 0, W as u32, H as u32);
const THEME: Theme = Theme::DARK;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Msg {
    Nav(usize),
    Rail(usize),
    Toggle(usize),
    Chip(usize),
    Act(usize),
    Main,
    More,
    Add,
    Search(String),
    Submit,
    Menu,
}

struct State {
    nav: [bool; 3],
    rail: [bool; 2],
    toggles: [bool; 3],
    chips: [bool; 4],
    count: u16,
    error: bool,
    people: usize,
    query: String,
}

const NAV: [&str; 3] = ["Home", "Cars", "Me"];
const RAIL: [&str; 2] = ["Inbox", "Sent"];
const TOGGLES: [&str; 3] = ["Bold", "Italic", "Underline"];
const CHIPS: [&str; 4] = ["Wi-Fi", "Bluetooth", "NFC", "GPS"];
const ACTIONS: [&str; 3] = ["Cut", "Copy", "Paste"];
const PEOPLE: [&str; 5] = ["Ana Maria", "Bob Stone", "Cleo Fox", "Dan Hill", "Eve Moon"];

fn view(s: &State) -> impl View<Msg> + use<'_> {
    fusuma::column((
        fusuma::column((
            top_bar("Garage", (button("Edit").on_press(Msg::Menu),)),
            toolbar((button("A"), button("B"))),
            form_field(
                "Name",
                text_field("x"),
                "As on the card",
                if s.error { "Required" } else { "" },
            ),
            key_value("Range", "412 km"),
            icon_badge('i', s.count),
        ))
        .gap(4),
        fusuma::column((
            navigation_bar(&NAV, marks(&s.nav), Msg::Nav),
            navigation_rail(&RAIL, marks(&s.rail), Msg::Rail),
            toggle_group(&TOGGLES, marks(&s.toggles), Msg::Toggle),
            chip_group(&CHIPS, marks(&s.chips), Msg::Chip),
            button_group(&ACTIONS, Msg::Act),
        ))
        .gap(4),
        fusuma::column((
            split_button("Charge", Msg::Main, Msg::More),
            fab('+', "Add").on_press(Msg::Add),
            search_bar(' ', &s.query, |q| Msg::Search(q.to_owned()), Msg::Submit),
            avatar_group(&PEOPLE[..s.people], 28, 3),
            text("end"),
        ))
        .gap(4),
    ))
    .gap(4)
}

type Screen = Embedded<Frame<Vec<u16>>>;

fn screen() -> Screen {
    Embedded::new(Frame::new(W, H, vec![0u16; usize::from(W) * usize::from(H)]).unwrap())
}

struct App {
    tree: Tree<256>,
    host: Host<Msg, 64>,
    state: State,
}

impl App {
    fn new() -> Self {
        let mut a = Self {
            tree: Tree::new(),
            host: Host::new(),
            state: State {
                nav: [true, false, false],
                rail: [true, false],
                toggles: [false; 3],
                chips: [false; 4],
                count: 3,
                error: false,
                people: 5,
                query: String::new(),
            },
        };
        a.sync();
        a
    }

    fn sync(&mut self) {
        if self.tree.root_id().is_none() {
            self.tree.column(None, Style::new()).unwrap();
        }
        let root = self.tree.root_id();
        self.host
            .sync(&mut self.tree, root, &view(&self.state))
            .unwrap();
    }

    fn frame(&mut self, r: &mut Screen) {
        self.tree.update(r, &THEME, BOUNDS);
        self.tree.paint_frame(r, &THEME, BOUNDS).unwrap();
    }

    fn find(&self, name: &str) -> NodeId {
        self.tree
            .semantics()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no '{name}'"))
            .id
    }

    fn names(&self) -> Vec<String> {
        self.tree
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }

    fn tap(&mut self, name: &str) -> Option<Msg> {
        let mut screen = screen();
        self.frame(&mut screen);
        let id = self.find(name);
        let r = self.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            self.tree.pointer(e);
        }
        let mut got = None;
        while let Some(a) = self.tree.pop_action() {
            if let Some(m) = self.host.message(&self.tree, a) {
                match &m {
                    Msg::Nav(i) => self.state.nav = [0, 1, 2].map(|k| k == *i),
                    Msg::Rail(i) => self.state.rail = [0, 1].map(|k| k == *i),
                    Msg::Toggle(i) => self.state.toggles[*i] ^= true,
                    Msg::Chip(i) => self.state.chips[*i] ^= true,
                    _ => {}
                }
                got = Some(m);
            }
        }
        if got.is_some() {
            self.sync();
        }
        got
    }
}

#[test]
fn every_part_sends_its_own_message() {
    let mut a = App::new();
    assert_eq!(a.tap("Cars"), Some(Msg::Nav(1)));
    assert_eq!(a.tap("Sent"), Some(Msg::Rail(1)));
    assert_eq!(a.tap("Italic"), Some(Msg::Toggle(1)));
    assert_eq!(a.tap("Copy"), Some(Msg::Act(1)));
    assert_eq!(a.tap("Charge"), Some(Msg::Main));
    assert_eq!(a.tap("v"), Some(Msg::More));
    assert_eq!(a.tap("Add"), Some(Msg::Add));
    assert_eq!(a.tap("Edit"), Some(Msg::Menu));
}

#[test]
fn a_navigation_bar_and_a_rail_have_one_place_and_a_toggle_group_any_number() {
    let mut a = App::new();
    a.tap("Me");
    assert_eq!(a.state.nav, [false, false, true]);
    a.tap("Cars");
    assert_eq!(a.state.nav, [false, true, false], "one at a time");
    a.tap("Bold");
    a.tap("Underline");
    assert_eq!(a.state.toggles, [true, false, true], "any number");
    a.tap("Bold");
    assert_eq!(a.state.toggles, [false, false, true]);
    // The chips are on or off each, and the marks reach the tree.
    a.tap("NFC");
    a.tap("GPS");
    let primary = |a: &App, n: &str| a.tree.is_primary(a.find(n));
    assert!(primary(&a, "NFC") && primary(&a, "GPS") && !primary(&a, "Wi-Fi"));
    // The toggles are the same, and the actions are never on.
    assert!(primary(&a, "Underline") && !primary(&a, "Bold"));
    a.tap("Cut");
    assert!(!primary(&a, "Cut"));
}

#[test]
fn the_form_field_swaps_a_hint_for_an_error_and_keeps_its_shape() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    assert!(a.names().iter().any(|n| n == "As on the card"));
    let before = a.tree.rect(a.find("As on the card")).unwrap();
    a.state.error = true;
    a.sync();
    a.frame(&mut r);
    let names = a.names();
    assert!(names.iter().any(|n| n == "Required"), "{names:?}");
    assert!(!names.iter().any(|n| n == "As on the card"));
    assert_eq!(
        a.tree.rect(a.find("Required")).unwrap().y,
        before.y,
        "the line below did not move"
    );
}

#[test]
fn the_icon_badge_shows_the_count_and_caps_it() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    assert!(a.names().iter().any(|n| n == "3"));
    for (count, shown) in [(12, "12"), (99, "99"), (100, "99+"), (4000, "99+")] {
        a.state.count = count;
        a.sync();
        a.frame(&mut r);
        assert!(a.names().iter().any(|n| n == shown), "{count} -> {shown}");
    }
    // Zero keeps the icon and drops the number.
    a.state.count = 0;
    a.sync();
    a.frame(&mut r);
    assert!(!a.names().iter().any(|n| n == "99+" || n == "12"));
}

#[test]
fn the_avatar_group_shows_initials_and_a_count_of_the_rest() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    let n = a.names();
    for want in ["AM", "BS", "CF", "+2"] {
        assert!(n.iter().any(|x| x == want), "{want} in {n:?}");
    }
    assert!(!n.iter().any(|x| x == "DH"), "only three are shown");
    // Fewer people than the most: no count.
    a.state.people = 2;
    a.sync();
    a.frame(&mut r);
    let n = a.names();
    assert!(n.iter().any(|x| x == "AM") && n.iter().any(|x| x == "BS"));
}

#[test]
fn a_pass_with_nothing_changed_allocates_nothing() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    a.sync();
    let used = measure(|| {
        a.sync();
    });
    assert_eq!(used.count_total, 0, "{used:?}");
}
