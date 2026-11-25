#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]
//! A small Elm program (a counter with a checkbox and a hint) run through the view layer: what it builds, how little it
//! changes, what its messages are, and what it costs in allocations.

use allocation_counter::measure;
use fusuma_render::{Color, Point, Probe, Rect, Renderer};
use fusuma_render_embedded::{Embedded, Frame};
use fusuma_ui::{ActionKind, ButtonVariant, CrossAlign, Pointer, Style, TextRole, Theme, Tree};
use fusuma_view::{Host, View, button, card, checkbox, column, named, row, show, text, text_fmt};

const W: u16 = 240;
const H: u16 = 200;
const BOUNDS: Rect = Rect::new(0, 0, W as u32, H as u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Msg {
    Inc,
    Dec,
    Verbose(bool),
}

#[derive(Default)]
struct State {
    n: i32,
    verbose: bool,
}

fn view(s: &State) -> impl View<Msg> + use<'_> {
    column((
        text("Counter").title(),
        card((
            text_fmt(format_args!("Count: {}", s.n)),
            row((
                button("-").on_press(Msg::Dec),
                button("+").primary().on_press(Msg::Inc),
            ))
            .gap(6),
            checkbox("Verbose", s.verbose).on_toggle(Msg::Verbose),
            show(s.verbose, text("Tap + or - to change it").caption()),
        ))
        .padding(8)
        .gap(6),
        named(fusuma_ui::Id::new(9), text("footer").caption()),
    ))
    .padding(8)
    .gap(6)
    .align(CrossAlign::Start)
}

fn update(s: &mut State, m: Msg) {
    match m {
        Msg::Inc => s.n += 1,
        Msg::Dec => s.n -= 1,
        Msg::Verbose(on) => s.verbose = on,
    }
}

type Screen = Embedded<Frame<Vec<u16>>>;

fn screen() -> Screen {
    Embedded::new(Frame::new(W, H, vec![0u16; usize::from(W) * usize::from(H)]).unwrap())
}

const THEME: Theme = Theme::DARK;

struct App {
    tree: Tree<32>,
    host: Host<Msg, 16>,
    state: State,
}

impl App {
    fn new() -> Self {
        let mut a = Self {
            tree: Tree::new(),
            host: Host::new(),
            state: State::default(),
        };
        a.sync();
        a
    }

    fn sync(&mut self) {
        // The host's place: the tree's root is a plain column the view goes into.
        if self.tree.root_id().is_none() {
            self.tree.column(None, Style::new()).unwrap();
        }
        let root = self.tree.root_id();
        self.host
            .sync(&mut self.tree, root, &view(&self.state))
            .unwrap();
    }

    /// The loop of an Elm program: actions become messages, messages change the state, the view is made again.
    fn pump(&mut self) -> usize {
        let mut handled = 0;
        while let Some(a) = self.tree.pop_action() {
            if let Some(m) = self.host.message(&self.tree, a) {
                update(&mut self.state, m);
                handled += 1;
            }
        }
        if handled > 0 {
            self.sync();
        }
        handled
    }

    fn tap(&mut self, id: fusuma_ui::NodeId) {
        let r = self.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            self.tree.pointer(e);
        }
    }

    fn frame(&mut self, r: &mut Screen) -> Option<Rect> {
        self.tree.update(r, &THEME, BOUNDS);
        self.tree.paint_frame(r, &THEME, BOUNDS).unwrap()
    }

    fn texts(&self) -> Vec<String> {
        self.tree
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }

    fn button(&self, label: &str) -> fusuma_ui::NodeId {
        self.tree
            .semantics()
            .find(|s| s.name == label)
            .map(|s| s.id)
            .unwrap()
    }
}

/// The same page, built by hand with the imperative API.
fn by_hand(s: &State) -> Tree<32> {
    let mut t = Tree::<32>::new();
    let root = t.column(None, Style::new()).unwrap();
    let col = t
        .column(
            Some(root),
            Style::new().padding(8).gap(6).align(CrossAlign::Start),
        )
        .unwrap();
    t.label(Some(col), "Counter", TextRole::Title).unwrap();
    let c = t.card(Some(col), Style::new().padding(8).gap(6)).unwrap();
    t.label(Some(c), &format!("Count: {}", s.n), TextRole::Body)
        .unwrap();
    let r = t.row(Some(c), Style::new().gap(6)).unwrap();
    t.button(Some(r), "-", ButtonVariant::Secondary).unwrap();
    t.button(Some(r), "+", ButtonVariant::Primary).unwrap();
    t.checkbox(Some(c), "Verbose", s.verbose).unwrap();
    let hint = t
        .label(Some(c), "Tap + or - to change it", TextRole::Caption)
        .unwrap();
    t.set_hidden(hint, !s.verbose);
    t.label(Some(col), "footer", TextRole::Caption).unwrap();
    t
}

fn pixels(r: &Screen) -> Vec<Color> {
    (0..i32::from(H))
        .flat_map(|y| (0..i32::from(W)).map(move |x| (x, y)))
        .map(|(x, y)| r.pixel(x, y).unwrap())
        .collect()
}

fn draw(t: &mut Tree<32>) -> Vec<Color> {
    let mut r = screen();
    t.update(&r, &THEME, BOUNDS);
    r.clear(THEME.colors.background).unwrap();
    t.paint(&mut r, &THEME).unwrap();
    pixels(&r)
}

#[test]
fn the_view_builds_the_same_picture_as_the_page_made_by_hand() {
    for (n, verbose) in [(0, false), (7, true), (-3, false)] {
        let state = State { n, verbose };
        let mut a = App::new();
        a.state = State { n, verbose };
        a.sync();
        let mut hand = by_hand(&state);
        assert!(
            draw(&mut a.tree) == draw(&mut hand),
            "n = {n}, verbose = {verbose}"
        );
    }
}

#[test]
fn pressing_a_button_is_a_message_and_the_screen_follows_the_state() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    assert!(a.texts().contains(&"Count: 0".to_owned()));
    let plus = a.button("+");
    for expected in 1..=3 {
        a.tap(plus);
        assert_eq!(a.pump(), 1);
        assert_eq!(a.state.n, expected);
        a.frame(&mut r);
        assert!(a.texts().contains(&format!("Count: {expected}")));
    }
    let minus = a.button("-");
    a.tap(minus);
    a.pump();
    assert_eq!(a.state.n, 2);
    // Toggling the checkbox sends its new state, and the hint appears.
    assert!(
        !a.texts().iter().any(|t| t.starts_with("Tap +")),
        "hidden at first"
    );
    let check = a.button("Verbose");
    a.tap(check);
    assert_eq!(a.pump(), 1);
    assert!(a.state.verbose);
    a.frame(&mut r);
    assert!(
        a.texts().iter().any(|t| t.starts_with("Tap +")),
        "shown now"
    );
    a.tap(check);
    a.pump();
    assert!(!a.state.verbose);
    a.frame(&mut r);
    assert!(
        !a.texts().iter().any(|t| t.starts_with("Tap +")),
        "hidden again"
    );
}

#[test]
fn a_change_of_one_value_redraws_only_where_it_is() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    assert_eq!(a.frame(&mut r), None, "nothing changed, nothing drawn");
    a.state.n = 1;
    a.sync();
    let drawn = a.frame(&mut r).expect("the count changed");
    let label = a
        .tree
        .rect(
            a.tree
                .semantics()
                .find(|s| s.name == "Count: 1")
                .unwrap()
                .id,
        )
        .unwrap();
    assert!(
        drawn.intersect(&label) == label,
        "the label is redrawn: {drawn:?} for {label:?}"
    );
    assert!(
        u64::from(drawn.w) * u64::from(drawn.h) * 6 < u64::from(W) * u64::from(H),
        "a small part of the screen: {drawn:?}"
    );
    // The same view again changes nothing at all.
    a.sync();
    assert_eq!(a.frame(&mut r), None);
}

#[test]
fn a_rebuild_keeps_the_structure_and_the_names_and_a_late_message_finds_nothing() {
    let mut a = App::new();
    let nodes = a.tree.len();
    let footer = a
        .tree
        .find(fusuma_ui::Id::new(9))
        .expect("the name is on its node");
    for n in 0..20 {
        a.state.n = n;
        a.state.verbose = n % 2 == 0;
        a.sync();
    }
    assert_eq!(a.tree.len(), nodes, "no node came or went");
    assert_eq!(a.tree.find(fusuma_ui::Id::new(9)), Some(footer));
    assert_eq!(a.host.msgs().len(), 3, "minus, plus and the checkbox");
    // An action from a node the table does not know is not a message.
    let stray = fusuma_ui::Action {
        source: footer,
        kind: ActionKind::Clicked,
    };
    assert_eq!(a.host.message(&a.tree, stray), None);
}

#[test]
fn a_running_program_asks_the_allocator_for_nothing() {
    let mut a = App::new();
    let mut r = screen();
    for _ in 0..3 {
        a.frame(&mut r);
    }
    let plus = a.button("+");
    let check = a.button("Verbose");
    let info = measure(|| {
        for k in 0..30 {
            a.tap(if k % 5 == 0 { check } else { plus });
            a.pump();
            a.frame(&mut r);
        }
    });
    assert_eq!(info.count_total, 0, "{info:?}");
    assert!(a.state.n > 10, "the session did something: {}", a.state.n);
}
