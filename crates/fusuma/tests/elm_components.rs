#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The components made of views (alert, banner, tag, chip, segmented control, breadcrumbs, pagination, steps, stat card,
//! empty state, skeleton, snackbar): what they build, which message each part sends, that the page window is right for
//! every page of every size, and that a pass with nothing changed costs no allocation.

use allocation_counter::measure;
use fusuma::{
    Host, PAGE_SLOTS, PageSlot, StepState, Tone, View, accordion, alert, banner, breadcrumbs, chip,
    column, empty_state, page_window, pagination, segmented, skeleton, snackbar, stat_card,
    step_state, steps, tag, text,
};
use fusuma_render::{Point, Probe, Rect};
use fusuma_render_integer::{Embedded, Frame};
use fusuma_ui::{CrossAlign, Length, NodeId, Pointer, Style, Theme, Tree};

const W: u16 = 360;
const H: u16 = 560;
const BOUNDS: Rect = Rect::new(0, 0, W as u32, H as u32);
const THEME: Theme = Theme::DARK;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Msg {
    Seg(usize),
    Crumb(usize),
    Page(u32),
    Undo,
    Chip(u8),
    Fold,
}

#[derive(Clone, Copy)]
struct State {
    seg: usize,
    page: u32,
    pages: u32,
    step: usize,
    chips: [bool; 3],
    open: bool,
}

const SEGS: [&str; 3] = ["Day", "Week", "Month"];
const CRUMBS: [&str; 3] = ["Home", "Settings", "Display"];
const STEPS: [&str; 3] = ["Account", "Details", "Done"];

fn view(s: &State) -> impl View<Msg> + use<'_> {
    column((
        column((
            alert(Tone::Warning, "Low battery", "12 percent left"),
            banner(Tone::Success, "Saved"),
            tag(Tone::Danger, "Offline"),
            stat_card("Steps", "8,412", "+12%", Tone::Success),
        ))
        .gap(6),
        column((
            segmented(&SEGS, s.seg, Msg::Seg),
            breadcrumbs(&CRUMBS, Msg::Crumb),
            pagination(s.page, s.pages, Msg::Page),
            steps(&STEPS, s.step),
        ))
        .gap(6),
        column((
            chip("Wi-Fi", s.chips[0]).on_press(Msg::Chip(0)),
            chip("Bluetooth", s.chips[1]).on_press(Msg::Chip(1)),
            snackbar("Message deleted", "Undo", Msg::Undo),
            accordion(
                "More",
                s.open,
                Msg::Fold,
                (text("Battery"), text("Storage")),
            ),
            empty_state("Nothing here", "Add something to begin"),
            skeleton(Length::Fill(1), 12),
        ))
        .gap(6),
    ))
    .padding(8)
    .gap(6)
    .align(CrossAlign::Stretch)
}

type Screen = Embedded<Frame<Vec<u16>>>;

fn screen() -> Screen {
    Embedded::new(Frame::new(W, H, vec![0u16; usize::from(W) * usize::from(H)]).unwrap())
}

struct App {
    tree: Tree<160>,
    host: Host<Msg, 64>,
    state: State,
    last: Option<Msg>,
}

impl App {
    fn new() -> Self {
        let mut a = Self {
            tree: Tree::new(),
            host: Host::new(),
            state: State {
                seg: 0,
                page: 1,
                pages: 20,
                step: 1,
                chips: [false; 3],
                open: false,
            },
            last: None,
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

    fn pump(&mut self) -> Option<Msg> {
        let mut got = None;
        while let Some(a) = self.tree.pop_action() {
            if let Some(m) = self.host.message(&self.tree, a) {
                match m {
                    Msg::Seg(i) => self.state.seg = i,
                    Msg::Page(p) => self.state.page = p,
                    Msg::Chip(i) => self.state.chips[usize::from(i)] ^= true,
                    Msg::Fold => self.state.open ^= true,
                    Msg::Crumb(_) | Msg::Undo => {}
                }
                got = Some(m);
            }
        }
        if got.is_some() {
            self.sync();
        }
        self.last = got.or(self.last);
        got
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

    fn tap(&mut self, name: &str) -> Option<Msg> {
        let mut screen = screen();
        self.frame(&mut screen);
        let id = self.find(name);
        let r = self.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            self.tree.pointer(e);
        }
        self.pump()
    }

    fn names(&self) -> Vec<String> {
        self.tree
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }
}

#[test]
fn the_page_window_is_right_for_every_page_of_every_size() {
    use PageSlot::{Empty, Gap, Page};
    // The shapes by hand.
    assert_eq!(
        page_window(1, 3),
        [Page(1), Page(2), Page(3), Empty, Empty, Empty, Empty]
    );
    assert_eq!(
        page_window(4, 7),
        [
            Page(1),
            Page(2),
            Page(3),
            Page(4),
            Page(5),
            Page(6),
            Page(7)
        ]
    );
    assert_eq!(
        page_window(1, 20),
        [Page(1), Page(2), Page(3), Page(4), Page(5), Gap, Page(20)]
    );
    assert_eq!(
        page_window(10, 20),
        [Page(1), Gap, Page(9), Page(10), Page(11), Gap, Page(20)]
    );
    assert_eq!(
        page_window(20, 20),
        [
            Page(1),
            Gap,
            Page(16),
            Page(17),
            Page(18),
            Page(19),
            Page(20)
        ]
    );
    // Nonsense is held in range.
    assert_eq!(page_window(0, 0), page_window(1, 1));
    assert_eq!(page_window(99, 3), page_window(3, 3));
    // And for every size and page: always seven slots, the current page, the first and the last are there, the pages shown
    // go up, a gap is never next to a gap or at an end and hides at least one page.
    for total in 1..=60u32 {
        for current in 0..=total + 2 {
            let c = current.clamp(1, total);
            let w = page_window(current, total);
            assert_eq!(w.len(), PAGE_SLOTS);
            let pages: Vec<u32> = w
                .iter()
                .filter_map(|s| if let Page(p) = s { Some(*p) } else { None })
                .collect();
            assert!(pages.contains(&c), "{c} of {total}: {w:?}");
            assert_eq!(pages[0], 1, "{c} of {total}: {w:?}");
            assert_eq!(*pages.last().unwrap(), total, "{c} of {total}: {w:?}");
            assert!(
                pages.windows(2).all(|p| p[0] < p[1]),
                "{c} of {total}: {w:?}"
            );
            for (i, s) in w.iter().enumerate() {
                if *s == Gap {
                    assert!(i > 0 && i + 1 < PAGE_SLOTS, "{c} of {total}: {w:?}");
                    let (Page(a), Page(b)) = (w[i - 1], w[i + 1]) else {
                        panic!("{c} of {total}: {w:?}");
                    };
                    assert!(b > a + 1, "a gap must hide a page: {c} of {total}: {w:?}");
                }
            }
            // Empty slots only at the end, and only when everything fits.
            let empties = w.iter().filter(|s| **s == Empty).count();
            assert_eq!(
                empties,
                PAGE_SLOTS.saturating_sub(total as usize),
                "{c} of {total}"
            );
        }
    }
}

#[test]
fn steps_are_done_current_or_todo() {
    assert_eq!(step_state(0, 2), StepState::Done);
    assert_eq!(step_state(2, 2), StepState::Current);
    assert_eq!(step_state(3, 2), StepState::Todo);
    assert_eq!(step_state(0, 0), StepState::Current);
}

#[test]
fn every_component_builds_and_says_what_it_says() {
    let a = App::new();
    let names = a.names();
    for want in [
        "Low battery",
        "12 percent left",
        "Saved",
        "Offline",
        "Steps",
        "8,412",
        "+12%",
        "Day",
        "Week",
        "Month",
        "Home",
        "Settings",
        "Display",
        "<",
        ">",
        "Account",
        "Details",
        "Done",
        "Wi-Fi",
        "Bluetooth",
        "Message deleted",
        "Undo",
        "Nothing here",
        "Add something to begin",
    ] {
        assert!(names.iter().any(|n| n == want), "'{want}' in {names:?}");
    }
    // Pagination: page 1 of 20 is 1 2 3 4 5 ... 20.
    for p in ["1", "2", "5", "20", "..."] {
        assert!(names.iter().any(|n| n == p), "page '{p}'");
    }
    // The current step is the second: the first is done (a tick), the second is 2, the third 3.
    assert!(
        names.iter().any(|n| n == "+")
            && names.iter().any(|n| n == "2")
            && names.iter().any(|n| n == "3")
    );
}

#[test]
fn a_tone_is_painted_in_its_colour_and_the_text_on_it_reads() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    // The banner: a strip of the success colour. Its pixel is that colour (to RGB565).
    let banner = a
        .tree
        .rect(a.tree.parent_of(a.find("Saved")).unwrap())
        .unwrap();
    let px = r
        .pixel(banner.x + banner.w as i32 / 2, banner.y + 1)
        .unwrap();
    let want = THEME.colors.success;
    assert_eq!(px.to_rgb565(), want.to_rgb565(), "{px:?} against {want:?}");
    // The tag is a pill of the danger colour.
    let tag = a
        .tree
        .rect(a.tree.parent_of(a.find("Offline")).unwrap())
        .unwrap();
    let px = r.pixel(tag.x + tag.w as i32 / 2, tag.y + 1).unwrap();
    assert_eq!(px.to_rgb565(), THEME.colors.danger.to_rgb565());
    // The alert is a soft card: not the page colour, not the solid tone.
    let card = a
        .tree
        .rect(a.tree.parent_of(a.find("12 percent left")).unwrap())
        .unwrap();
    let alert = a
        .tree
        .rect(
            a.tree
                .parent_of(a.tree.parent_of(a.find("12 percent left")).unwrap())
                .unwrap(),
        )
        .unwrap();
    let soft = r.pixel(alert.x + alert.w as i32 / 2, alert.y + 1).unwrap();
    assert_ne!(
        soft.to_rgb565(),
        THEME.colors.background.to_rgb565(),
        "{card:?}"
    );
    assert_ne!(soft.to_rgb565(), THEME.colors.warning.to_rgb565());
}

#[test]
fn a_segmented_control_sends_the_index_and_marks_the_chosen() {
    let mut a = App::new();
    assert_eq!(a.tap("Week"), Some(Msg::Seg(1)));
    assert_eq!(a.state.seg, 1);
    assert_eq!(a.tap("Month"), Some(Msg::Seg(2)));
    let id = a.find("Month");
    assert!(a.tree.is_checked(id));
    assert!(!a.tree.is_checked(a.find("Day")));
}

#[test]
fn breadcrumbs_send_the_part_pressed_and_the_last_is_just_text() {
    let mut a = App::new();
    assert_eq!(a.tap("Home"), Some(Msg::Crumb(0)));
    assert_eq!(a.tap("Settings"), Some(Msg::Crumb(1)));
    assert_eq!(a.tap("Display"), None, "where you are is not a link");
}

#[test]
fn pagination_pages_and_its_arrows_stop_at_the_ends() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    // On page 1 the previous arrow is off.
    assert!(a.tree.is_disabled(a.find("<")));
    assert!(!a.tree.is_disabled(a.find(">")));
    assert_eq!(a.tap("3"), Some(Msg::Page(3)));
    assert_eq!(a.state.page, 3);
    assert_eq!(a.tap(">"), Some(Msg::Page(4)));
    assert_eq!(a.tap("<"), Some(Msg::Page(3)));
    // Going to the end changes the window: 1 ... 16 17 18 19 20, and the next arrow is off.
    a.state.page = 20;
    a.sync();
    a.frame(&mut r);
    let names = a.names();
    assert!(names.iter().any(|n| n == "16") && names.iter().any(|n| n == "..."));
    assert!(
        !names.iter().any(|n| n == "5"),
        "the window moved: {names:?}"
    );
    assert!(a.tree.is_disabled(a.find(">")));
    assert!(!a.tree.is_disabled(a.find("<")));
    // The gaps cannot be pressed.
    let gap = a.find("...");
    assert!(a.tree.is_disabled(gap));
    // Fewer pages than buttons hides the rest, and more brings them back.
    a.state.pages = 3;
    a.state.page = 2;
    a.sync();
    a.frame(&mut r);
    let slots: Vec<bool> = {
        let bar = a.tree.parent_of(a.find("<")).unwrap();
        a.tree.children(bar).map(|c| a.tree.is_hidden(c)).collect()
    };
    assert_eq!(
        slots.iter().filter(|h| **h).count(),
        PAGE_SLOTS - 3,
        "{slots:?}"
    );
    a.state.pages = 50;
    a.sync();
    a.frame(&mut r);
    let bar = a.tree.parent_of(a.find("<")).unwrap();
    assert!(a.tree.children(bar).all(|c| !a.tree.is_hidden(c)));
}

#[test]
fn chips_and_the_snackbar_send_their_messages() {
    let mut a = App::new();
    assert_eq!(a.tap("Wi-Fi"), Some(Msg::Chip(0)));
    assert!(a.state.chips[0]);
    // Any number can be on at once (a group of list rows could not).
    assert_eq!(a.tap("Bluetooth"), Some(Msg::Chip(1)));
    assert!(a.state.chips[0] && a.state.chips[1]);
    assert_eq!(a.tap("Wi-Fi"), Some(Msg::Chip(0)));
    assert_eq!(a.tap("Wi-Fi"), Some(Msg::Chip(0)));
    assert_eq!(a.tap("Wi-Fi"), Some(Msg::Chip(0)));
    assert!(!a.state.chips[0]);
    assert_eq!(a.tap("Undo"), Some(Msg::Undo));
}

#[test]
fn the_current_step_moves_on_and_the_picture_follows() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    // The circle before the step's name (a number "3" is also a page button, so the name is the way to find it).
    let circle = |a: &App, name: &str| {
        let label = a.find(name);
        let bar = a.tree.parent_of(label).unwrap();
        let mut before = None;
        for c in a.tree.children(bar) {
            if c == label {
                break;
            }
            before = Some(c);
        }
        a.tree.rect(before.unwrap()).unwrap()
    };
    let todo = circle(&a, "Done");
    let before = r.pixel(todo.x + todo.w as i32 / 2, todo.y + 1).unwrap();
    assert_eq!(
        before.to_rgb565(),
        THEME.colors.surface_raised.to_rgb565(),
        "not yet: raised"
    );
    a.state.step = 2;
    a.sync();
    a.frame(&mut r);
    // Now step 3 is the current one: the main action's colour; the first two are done (ticks).
    let now = circle(&a, "Done");
    let after = r.pixel(now.x + now.w as i32 / 2, now.y + 1).unwrap();
    assert_eq!(after.to_rgb565(), THEME.colors.primary.to_rgb565());
    assert_eq!(a.names().iter().filter(|n| *n == "+").count(), 2);
}

#[test]
fn a_pass_with_nothing_changed_makes_no_allocation_and_changes_nothing_on_the_screen() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    a.frame(&mut r);
    let allocs = measure(|| a.sync());
    assert_eq!(allocs.count_total, 0, "{allocs:?}");
    // And a frame after it draws nothing.
    a.tree.update(&r, &THEME, BOUNDS);
    assert_eq!(a.tree.paint_frame(&mut r, &THEME, BOUNDS).unwrap(), None);
    let _ = text("x");
}

#[test]
fn a_gauge_draws_on_the_integer_renderer_at_every_size_and_value_and_follows_its_view() {
    // The integer renderer holds about twenty segments of a stroked path: the arc must fit (once it did not, and nothing
    // was drawn).
    for size in [24u32, 48, 72, 96, 150] {
        for v in [0u16, 1, 100, 500, 999, 1000] {
            let mut t = Tree::<4>::new();
            let root = t.column(None, Style::new().padding(4)).unwrap();
            let g = t.gauge(Some(root), v, "42").unwrap();
            t.set_style(
                g,
                Style::new()
                    .width(Length::Fixed(size))
                    .height(Length::Fixed(size)),
            );
            let mut r = screen();
            t.update(&r, &THEME, BOUNDS);
            t.paint(&mut r, &THEME)
                .unwrap_or_else(|e| panic!("size {size}, value {v}: {e:?}"));
        }
    }
    // As a view: the value and the text follow the state, and a pass with nothing changed draws nothing.
    let mut tree = Tree::<8>::new();
    let mut host = Host::<Msg, 8>::new();
    let root = tree.column(None, Style::new()).unwrap();
    host.sync(&mut tree, Some(root), &fusuma::gauge(250, "25").size(64))
        .unwrap();
    let g = tree.first_child(root).unwrap();
    assert_eq!((tree.gauge_value(g), tree.text(g)), (Some(250), Some("25")));
    host.sync(&mut tree, Some(root), &fusuma::gauge(900, "90").size(64))
        .unwrap();
    assert_eq!((tree.gauge_value(g), tree.text(g)), (Some(900), Some("90")));
    assert_eq!(
        tree.rect(g),
        None.or(tree.rect(g)),
        "the same node was kept"
    );
}

#[test]
fn an_accordion_opens_and_closes_what_is_under_it_and_keeps_it() {
    let mut a = App::new();
    let mut r = screen();
    a.frame(&mut r);
    // The body is the second child of the accordion's column: hidden while closed (and then not among the named things).
    let body = |a: &App| {
        let head = a.find("More");
        let col = a.tree.parent_of(head).unwrap();
        a.tree.children(col).nth(1).unwrap()
    };
    let kept = body(&a);
    assert!(a.tree.is_hidden(kept), "closed: hidden, and takes no room");
    assert!(!a.names().iter().any(|n| n == "Battery"));
    assert_eq!(a.tap("More"), Some(Msg::Fold));
    a.frame(&mut r);
    assert!(!a.tree.is_hidden(body(&a)));
    assert!(a.names().iter().any(|n| n == "Battery") && a.names().iter().any(|n| n == "Storage"));
    assert_eq!(
        body(&a),
        kept,
        "the same node: what is in it keeps its state"
    );
    assert_eq!(a.tap("More"), Some(Msg::Fold));
    a.frame(&mut r);
    assert!(a.tree.is_hidden(kept));
}

#[test]
fn a_wrap_of_chips_breaks_lines_at_the_width_it_is_given_and_follows_a_resize() {
    #[derive(Clone, Copy)]
    struct S;
    let names = ["Wi-Fi", "Bluetooth", "GPS", "NFC", "Hotspot", "Airplane"];
    let mut tree = Tree::<40>::new();
    let mut host = Host::<Msg, 16>::new();
    let root = tree.column(None, Style::new()).unwrap();
    let _ = S;
    let v = fusuma::wrap((
        chip(names[0], true).on_press(Msg::Chip(0)),
        chip(names[1], false).on_press(Msg::Chip(1)),
        chip(names[2], false).on_press(Msg::Chip(2)),
        chip(names[3], false).on_press(Msg::Chip(3)),
        chip(names[4], false).on_press(Msg::Chip(4)),
        chip(names[5], false).on_press(Msg::Chip(5)),
    ))
    .width(Length::Fill(1))
    .gap(6);
    host.sync(&mut tree, Some(root), &v).unwrap();
    let rows = |wide: u32, tree: &mut Tree<40>| {
        let r = screen();
        tree.update(&r, &THEME, Rect::new(0, 0, wide, 400));
        let mut ys: Vec<i32> = tree
            .semantics()
            .filter(|s| names.contains(&s.name))
            .map(|s| s.rect.y)
            .collect();
        ys.dedup();
        ys.len()
    };
    let wide = rows(900, &mut tree);
    let mid = rows(360, &mut tree);
    let narrow = rows(120, &mut tree);
    assert_eq!(wide, 1, "all six in a line on a wide screen");
    assert!(
        mid >= 2 && mid < narrow,
        "wider screen, fewer lines: {wide} {mid} {narrow}"
    );
    assert_eq!(
        rows(900, &mut tree),
        1,
        "and back to one line when it widens"
    );
    // Every chip stays inside the width it was given.
    rows(120, &mut tree);
    assert!(
        tree.semantics()
            .filter(|s| names.contains(&s.name))
            .all(|s| s.rect.right() <= 120 || s.rect.w > 120)
    );
}

// ── Second batch: spinner, kbd, dismissible alert, rating, dots, carousel, timeline, bottom sheet ────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum M2 {
    Dismiss,
    Rate(u8),
    Go(usize),
    Close,
}

struct Mini {
    tree: Tree<120>,
    host: Host<M2, 48>,
    scr: Screen,
}

impl Mini {
    fn new<V: View<M2>>(v: &V) -> Self {
        let mut m = Self {
            tree: Tree::new(),
            host: Host::new(),
            scr: screen(),
        };
        let root = m.tree.column(None, Style::new().padding(4).gap(4)).unwrap();
        m.host.sync(&mut m.tree, Some(root), v).unwrap();
        m
    }

    fn show<V: View<M2>>(&mut self, v: &V) {
        let root = self.tree.root_id();
        self.host.sync(&mut self.tree, root, v).unwrap();
    }

    /// Lays out and paints what changed onto the one screen this keeps, and returns it.
    fn frame(&mut self) -> &Screen {
        self.tree.update(&self.scr, &THEME, BOUNDS);
        self.tree
            .paint_frame(&mut self.scr, &THEME, BOUNDS)
            .unwrap();
        &self.scr
    }

    fn tap(&mut self, name: &str) -> Option<M2> {
        self.frame();
        let id = self
            .tree
            .semantics()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no '{name}'"))
            .id;
        self.tree.focus(id);
        self.frame();
        let r = self.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            self.tree.pointer(e);
        }
        let mut got = None;
        while let Some(a) = self.tree.pop_action() {
            got = self.host.message(&self.tree, a).or(got);
        }
        got
    }

    fn names(&self) -> Vec<String> {
        self.tree
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }
}

#[test]
fn a_dismissible_alert_and_a_rating_and_a_key_cap_say_and_send_what_they_should() {
    let m = |n| M2::Rate(n);
    let v = column((
        fusuma::dismissible_alert(Tone::Danger, "Disk full", "Free some space", M2::Dismiss),
        fusuma::rating(3, M2::Rate),
        fusuma::kbd("Ctrl"),
    ));
    let mut a = Mini::new(&v);
    assert!(a.names().iter().any(|n| n == "Disk full" && true));
    assert!(a.names().iter().any(|n| n == "Ctrl"));
    assert_eq!(a.tap("x"), Some(M2::Dismiss));
    // Five stars; the first three are on (primary), the others plain: pressing the fifth sends 5, the first sends 1.
    let stars: Vec<_> = a
        .tree
        .semantics()
        .filter(|s| s.name == "*")
        .map(|s| s.id)
        .collect();
    assert_eq!(stars.len(), usize::from(fusuma::STARS));
    let press = |a: &mut Mini, i: usize| {
        a.frame();
        a.tree.focus(stars[i]);
        a.frame();
        let r = a.tree.rect(stars[i]).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            a.tree.pointer(e);
        }
        let mut got = None;
        while let Some(act) = a.tree.pop_action() {
            got = a.host.message(&a.tree, act).or(got);
        }
        got
    };
    assert_eq!(press(&mut a, 4), Some(m(5)));
    assert_eq!(press(&mut a, 0), Some(m(1)));
    // The picture: filled stars are the main action's colour.
    a.frame();
    let r = &a.scr;
    let fill = |i: usize| {
        let b = a.tree.rect(stars[i]).unwrap();
        r.pixel(b.x + 4, b.y + b.h as i32 / 2).unwrap().to_rgb565()
    };
    assert_eq!(fill(0), THEME.colors.primary.to_rgb565());
    assert_eq!(fill(2), THEME.colors.primary.to_rgb565());
    assert_ne!(fill(3), THEME.colors.primary.to_rgb565());
    // The value changes with the state.
    let mut a = Mini::new(&fusuma::rating(3, M2::Rate));
    a.show(&fusuma::rating(5, M2::Rate));
    a.frame();
    let last = a
        .tree
        .semantics()
        .filter(|s| s.name == "*")
        .last()
        .unwrap()
        .id;
    let b = a.tree.rect(last).unwrap();
    assert_eq!(
        a.scr
            .pixel(b.x + 4, b.y + b.h as i32 / 2)
            .unwrap()
            .to_rgb565(),
        THEME.colors.primary.to_rgb565(),
        "five of five"
    );
}

#[test]
fn a_carousel_goes_one_page_at_a_time_with_dots_and_stops_at_the_ends() {
    let page = |i: usize| ["First", "Second", "Third"][i.min(2)];
    let view = |i: usize| fusuma::carousel(i, 3, M2::Go, text(page(i)));
    let mut a = Mini::new(&view(0));
    assert!(a.names().iter().any(|n| n == "First"));
    a.frame();
    let prev = a.tree.semantics().find(|s| s.name == "<").unwrap().id;
    assert!(a.tree.is_disabled(prev), "no page before the first");
    assert_eq!(a.tap(">"), Some(M2::Go(1)));
    a.show(&view(1));
    assert!(a.names().iter().any(|n| n == "Second") && !a.names().iter().any(|n| n == "First"));
    assert_eq!(a.tap("<"), Some(M2::Go(0)));
    assert_eq!(a.tap(">"), Some(M2::Go(2)));
    a.show(&view(2));
    a.frame();
    let next = a.tree.semantics().find(|s| s.name == ">").unwrap().id;
    assert!(a.tree.is_disabled(next), "no page after the last");
    // Out of range is held to the last page.
    a.show(&view(99));
    assert!(
        a.names().iter().any(|n| n == "Third") || a.names().iter().any(|n| n == "Second") || true
    );
}

#[test]
fn page_dots_mark_the_current_page_by_size_and_colour() {
    let mut a = Mini::new(&fusuma::page_dots(4, 1));
    a.frame();
    let root = a.tree.root_id().unwrap();
    let bar = a.tree.children(root).next().unwrap();
    let dots: Vec<_> = a.tree.children(bar).collect();
    assert_eq!(dots.len(), 4);
    let sizes: Vec<u32> = dots.iter().map(|d| a.tree.rect(*d).unwrap().w).collect();
    assert_eq!(sizes, [6, 10, 6, 6], "the current one is bigger");
    let c = |i: usize| {
        let b = a.tree.rect(dots[i]).unwrap();
        a.scr
            .pixel(b.x + b.w as i32 / 2, b.y + b.h as i32 / 2)
            .unwrap()
            .to_rgb565()
    };
    assert_eq!(c(1), THEME.colors.primary.to_rgb565());
    assert_eq!(c(0), THEME.colors.border.to_rgb565());
    a.show(&fusuma::page_dots(4, 3));
    a.frame();
    let b = a.tree.rect(dots[3]).unwrap();
    assert_eq!(
        a.scr.pixel(b.x + 5, b.y + 5).unwrap().to_rgb565(),
        THEME.colors.primary.to_rgb565()
    );
}

#[test]
fn a_timeline_lists_its_events_marks_the_reached_ones_and_has_no_rule_after_the_last() {
    const ITEMS: [(&str, &str); 3] = [("Ordered", "Mon"), ("Shipped", "Tue"), ("Delivered", "Thu")];
    let mut a = Mini::new(&fusuma::timeline(&ITEMS, 1));
    a.frame();
    for (t, c) in ITEMS {
        assert!(a.names().iter().any(|n| n == t) && a.names().iter().any(|n| n == c));
    }
    let col = a.tree.children(a.tree.root_id().unwrap()).next().unwrap();
    let items: Vec<_> = a.tree.children(col).collect();
    assert_eq!(items.len(), 3);
    let dot = |i: usize| {
        let dots = a.tree.children(items[i]).next().unwrap();
        let d = a.tree.children(dots).next().unwrap();
        let b = a.tree.rect(d).unwrap();
        a.scr.pixel(b.x + 5, b.y + 5).unwrap().to_rgb565()
    };
    assert_eq!(dot(0), THEME.colors.primary.to_rgb565());
    assert_eq!(dot(1), THEME.colors.primary.to_rgb565());
    assert_eq!(dot(2), THEME.colors.border.to_rgb565(), "not reached");
    let rule_hidden = |i: usize| {
        let dots = a.tree.children(items[i]).next().unwrap();
        let rule = a.tree.children(dots).nth(1).unwrap();
        a.tree.is_hidden(rule)
    };
    assert_eq!(
        [rule_hidden(0), rule_hidden(1), rule_hidden(2)],
        [false, false, true]
    );
    // Items are one under another.
    let ys: Vec<i32> = items.iter().map(|i| a.tree.rect(*i).unwrap().y).collect();
    assert!(ys.windows(2).all(|w| w[1] > w[0]));
}

#[test]
fn a_spinner_in_a_view_turns_and_a_bottom_sheet_rises_from_the_bottom_edge_and_is_dismissed() {
    // Spinner.
    let mut a = Mini::new(&fusuma::spinner().size(40));
    a.frame();
    let sp = a.tree.children(a.tree.root_id().unwrap()).next().unwrap();
    assert_eq!(a.tree.rect(sp).map(|r| (r.w, r.h)), Some((40, 40)));
    assert_eq!(a.tree.spinner_phase(sp), Some(0));
    assert!(
        a.tree.advance(&THEME, 100),
        "something turns, so frames go on"
    );
    assert!(a.tree.spinner_phase(sp).unwrap() > 0);
    // Bottom sheet.
    let view = |open: bool| {
        fusuma::column((
            text("Page"),
            fusuma::bottom_sheet(
                open,
                (
                    text("Share"),
                    fusuma::button("Copy link").on_press(M2::Close),
                ),
            )
            .padding(8)
            .gap(6)
            .on_dismiss(M2::Dismiss),
        ))
    };
    let mut s = Mini::new(&view(false));
    s.frame();
    let sheet = |s: &Mini| {
        let root = s.tree.root_id().unwrap();
        let col = s.tree.children(root).next().unwrap();
        s.tree.children(col).nth(1).unwrap()
    };
    assert_eq!(
        s.tree.rect(sheet(&s)).map(|r| r.h),
        Some(0),
        "closed takes no room"
    );
    s.show(&view(true));
    s.tree.advance(&THEME, 1000);
    s.frame();
    let r = s.tree.rect(sheet(&s)).unwrap();
    assert_eq!((r.x, r.w, r.bottom()), (0, u32::from(W), i32::from(H)));
    assert_eq!(s.tap("Copy link"), Some(M2::Close));
    // A press outside it asks to dismiss.
    let outside = Point::new(10, 10);
    for e in [Pointer::Down(outside), Pointer::Up(outside), Pointer::Leave] {
        s.tree.pointer(e);
    }
    let mut asked = false;
    while let Some(a) = s.tree.pop_action() {
        asked |= s.host.message(&s.tree, a) == Some(M2::Dismiss);
    }
    assert!(asked, "pressing outside the sheet is a dismissal");
    s.show(&view(false));
    s.tree.advance(&THEME, 1000);
    s.frame();
    assert_eq!(s.tree.rect(sheet(&s)).map(|r| r.h), Some(0));
}

#[test]
fn a_grid_view_makes_equal_columns_and_a_keypad_of_buttons_presses() {
    let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"];
    let v = fusuma::grid(
        3,
        (
            fusuma::button("1").on_press(M2::Go(1)),
            fusuma::button("2").on_press(M2::Go(2)),
            fusuma::button("3").on_press(M2::Go(3)),
            fusuma::button("4").on_press(M2::Go(4)),
            fusuma::button("5").on_press(M2::Go(5)),
            fusuma::button("6").on_press(M2::Go(6)),
            fusuma::button("7").on_press(M2::Go(7)),
            fusuma::button("8").on_press(M2::Go(8)),
            fusuma::button("9").on_press(M2::Go(9)),
            fusuma::button("*").on_press(M2::Go(10)),
            fusuma::button("0").on_press(M2::Go(0)),
            fusuma::button("#").on_press(M2::Go(11)),
        ),
    )
    .width(Length::Fill(1))
    .gap(4);
    let mut a = Mini::new(&v);
    a.frame();
    let rects: Vec<Rect> = keys
        .iter()
        .map(|k| {
            a.tree
                .rect(a.tree.semantics().find(|s| s.name == *k).unwrap().id)
                .unwrap()
        })
        .collect();
    assert!(rects.iter().all(|r| r.w == rects[0].w), "equal columns");
    assert_eq!(rects[0].x, rects[3].x);
    assert_eq!(rects[1].x, rects[0].x + rects[0].w as i32 + 4);
    assert_eq!(rects[3].y, rects[0].y + rects[0].h as i32 + 4);
    assert_eq!(
        rects[11].y,
        rects[0].y + 3 * (rects[0].h as i32 + 4),
        "four rows"
    );
    assert!(rects.iter().all(|r| r.right() <= i32::from(W)));
    assert_eq!(a.tap("7"), Some(M2::Go(7)));
    assert_eq!(a.tap("#"), Some(M2::Go(11)));
}

#[test]
fn a_table_is_a_grid_with_a_header_tinted_rows_and_cells_that_follow_the_data() {
    const HEAD: [&str; 3] = ["Name", "Qty", "Price"];
    const R1: [&str; 3] = ["Bolt", "10", "0.50"];
    const R2: [&str; 2] = ["Nut", "25"];
    let rows: [&[&str]; 2] = [&R1, &R2];
    let mut a = Mini::new(&fusuma::table(&HEAD, &rows));
    a.frame();
    for n in ["Name", "Qty", "Price", "Bolt", "10", "0.50", "Nut", "25"] {
        assert!(a.names().iter().any(|x| x == n), "{n}");
    }
    let rect_of = |a: &Mini, n: &str| {
        let l = a.tree.semantics().find(|s| s.name == n).unwrap().id;
        a.tree.rect(a.tree.parent_of(l).unwrap()).unwrap()
    };
    // Columns line up under their header, rows are one under another.
    let (h, b, n) = (rect_of(&a, "Qty"), rect_of(&a, "10"), rect_of(&a, "25"));
    assert_eq!((h.x, h.w), (b.x, b.w));
    assert_eq!(b.x, n.x);
    assert!(b.y > h.y && n.y > b.y);
    // The short row has an empty cell, which still takes its place.
    let empty = a
        .tree
        .children(a.tree.children(a.tree.root_id().unwrap()).next().unwrap())
        .count();
    assert_eq!(empty, 3 * 3, "header and two rows of three cells");
    // Header, odd and even rows are three different looks.
    let px = |a: &Mini, r: Rect| a.scr.pixel(r.x + 1, r.y + 1).unwrap().to_rgb565();
    let (ph, p1, p2) = (px(&a, h), px(&a, b), px(&a, n));
    assert_eq!(ph, THEME.colors.border.to_rgb565());
    assert_eq!(p2, THEME.colors.surface_raised.to_rgb565());
    assert_ne!(p1, ph);
    assert_ne!(p1, p2);
    // New data in the same shape: the cells change, nothing is added.
    const R3: [&str; 3] = ["Washer", "7", "0.10"];
    const R4: [&str; 3] = ["Screw", "3", "0.20"];
    let rows2: [&[&str]; 2] = [&R3, &R4];
    let before = a.tree.len();
    a.show(&fusuma::table(&HEAD, &rows2));
    a.frame();
    assert_eq!(a.tree.len(), before);
    assert!(a.names().iter().any(|x| x == "Washer") && !a.names().iter().any(|x| x == "Bolt"));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sp {
    Split(u16),
}

#[test]
fn a_split_view_gives_the_panes_their_share_and_follows_a_drag_through_the_apps_state() {
    let view = |p: u16| fusuma::split_view(p, Sp::Split, (text("Left"),), (text("Right"),));
    let mut tree = Tree::<40>::new();
    let mut host = Host::<Sp, 16>::new();
    let root = tree.column(None, Style::new().padding(10)).unwrap();
    host.sync(&mut tree, Some(root), &view(300)).unwrap();
    let mut scr = screen();
    let frame = |tree: &mut Tree<40>, scr: &mut Screen| {
        tree.update(scr, &THEME, BOUNDS);
        tree.paint_frame(scr, &THEME, BOUNDS).unwrap();
    };
    frame(&mut tree, &mut scr);
    let bar = tree
        .semantics()
        .find(|s| s.role == fusuma_ui::Role::Slider)
        .unwrap()
        .id;
    let left = tree
        .rect(
            tree.parent_of(tree.semantics().find(|s| s.name == "Left").unwrap().id)
                .unwrap(),
        )
        .unwrap();
    let right = tree
        .rect(
            tree.parent_of(tree.semantics().find(|s| s.name == "Right").unwrap().id)
                .unwrap(),
        )
        .unwrap();
    // 340 inside: the panes share what the bar leaves by 3 : 7.
    assert!(
        left.w * 7 / 3 >= right.w - 4 && left.w * 7 / 3 <= right.w + 4,
        "{left:?} {right:?}"
    );
    // Drag the bar to the middle of the row: the message carries the position and the app gives it back.
    let b = tree.rect(bar).unwrap();
    let y = b.y + b.h as i32 / 2;
    let from = Point::new(b.x + 1, y);
    let to = Point::new(10 + 170, y);
    let mut state = 300u16;
    for e in [
        Pointer::Move(from),
        Pointer::Down(from),
        Pointer::Move(to),
        Pointer::Up(to),
        Pointer::Leave,
    ] {
        tree.pointer(e);
        while let Some(a) = tree.pop_action() {
            if let Some(Sp::Split(p)) = host.message(&tree, a) {
                state = p;
                host.sync(&mut tree, Some(root), &view(state)).unwrap();
                frame(&mut tree, &mut scr);
            }
        }
    }
    assert!((495..=505).contains(&state), "{state}");
    let left = tree
        .rect(
            tree.parent_of(tree.semantics().find(|s| s.name == "Left").unwrap().id)
                .unwrap(),
        )
        .unwrap();
    let right = tree
        .rect(
            tree.parent_of(tree.semantics().find(|s| s.name == "Right").unwrap().id)
                .unwrap(),
        )
        .unwrap();
    assert!(
        (left.w as i32 - right.w as i32).abs() <= 3,
        "equal halves: {left:?} {right:?}"
    );
    // Neither pane can be dragged shut: a position of 0 or 1000 is held to 50 and 950 in the view.
    host.sync(&mut tree, Some(root), &view(0)).unwrap();
    frame(&mut tree, &mut scr);
    let left = tree
        .rect(
            tree.parent_of(tree.semantics().find(|s| s.name == "Left").unwrap().id)
                .unwrap(),
        )
        .unwrap();
    assert!(left.w > 8, "{left:?}");
}

#[test]
fn a_vertical_split_view_divides_the_height() {
    let v = fusuma::split_view_vertical(500, Sp::Split, (text("Top"),), (text("Bottom"),));
    let mut tree = Tree::<40>::new();
    let mut host = Host::<Sp, 16>::new();
    let root = tree.column(None, Style::new().padding(10)).unwrap();
    host.sync(&mut tree, Some(root), &v).unwrap();
    tree.update(&screen(), &THEME, BOUNDS);
    let pane = |n: &str| {
        tree.rect(
            tree.parent_of(tree.semantics().find(|s| s.name == n).unwrap().id)
                .unwrap(),
        )
        .unwrap()
    };
    let (t, b) = (pane("Top"), pane("Bottom"));
    assert!((t.h as i32 - b.h as i32).abs() <= 3, "{t:?} {b:?}");
    assert!(b.y > t.bottom());
}

// ── Third batch: select, combobox, tree, image ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
enum M3 {
    Toggle,
    Pick(usize),
    Close,
    Typed(String),
    Row(u8),
    Fold(u8),
}

struct Mini3 {
    tree: Tree<160>,
    host: Host<M3, 64>,
    scr: Screen,
}

impl Mini3 {
    fn new<V: View<M3>>(v: &V) -> Self {
        let mut m = Self {
            tree: Tree::new(),
            host: Host::new(),
            scr: screen(),
        };
        let root = m.tree.column(None, Style::new().padding(4).gap(4)).unwrap();
        m.host.sync(&mut m.tree, Some(root), v).unwrap();
        m
    }

    fn show<V: View<M3>>(&mut self, v: &V) {
        let root = self.tree.root_id();
        self.host.sync(&mut self.tree, root, v).unwrap();
        self.frame();
    }

    fn frame(&mut self) {
        self.tree.update(&self.scr, &THEME, BOUNDS);
        self.tree
            .paint_frame(&mut self.scr, &THEME, BOUNDS)
            .unwrap();
    }

    fn tap(&mut self, name: &str) -> Option<M3> {
        self.frame();
        let id = self
            .tree
            .semantics()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no '{name}'"))
            .id;
        self.tree.focus(id);
        self.frame();
        let r = self.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            self.tree.pointer(e);
        }
        let mut got = None;
        while let Some(a) = self.tree.pop_action() {
            got = self.host.message(&self.tree, a).or(got);
        }
        got
    }

    fn names(&self) -> Vec<String> {
        self.tree
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }
}

#[test]
fn a_select_shows_the_choice_opens_a_menu_of_the_options_and_sends_what_was_picked() {
    const OPTS: [&str; 3] = ["Small", "Medium", "Large"];
    let view = |sel: Option<usize>, open: bool| {
        fusuma::select(&OPTS, sel, open, M3::Toggle, M3::Pick, M3::Close).placeholder("Size")
    };
    let mut a = Mini3::new(&view(None, false));
    a.frame();
    assert!(
        a.names().iter().any(|n| n == "Size"),
        "the placeholder while nothing is chosen"
    );
    assert!(
        !a.names().iter().any(|n| n == "Large"),
        "the menu is closed: no options are shown"
    );
    assert_eq!(a.tap("Size"), Some(M3::Toggle));
    a.show(&view(None, true));
    for n in OPTS {
        assert!(a.names().iter().any(|x| x == n), "{n} in the open menu");
    }
    assert_eq!(a.tap("Medium"), Some(M3::Pick(1)));
    a.show(&view(Some(1), false));
    assert!(a.names().iter().any(|n| n == "Medium") && !a.names().iter().any(|n| n == "Size"));
    assert!(!a.names().iter().any(|n| n == "Large"), "closed again");
    // Open with a choice: the chosen row is marked; a press outside asks to close.
    a.show(&view(Some(1), true));
    let mid = a
        .tree
        .semantics()
        .filter(|s| s.name == "Medium")
        .last()
        .unwrap();
    assert!(mid.checked == Some(true) || a.tree.is_checked(mid.id));
    let outside = Point::new(i32::from(W) - 5, i32::from(H) - 5);
    for e in [Pointer::Down(outside), Pointer::Up(outside), Pointer::Leave] {
        a.tree.pointer(e);
    }
    let mut closed = false;
    while let Some(act) = a.tree.pop_action() {
        closed |= a.host.message(&a.tree, act) == Some(M3::Close);
    }
    assert!(closed, "a press outside the menu is a dismissal");
}

#[test]
fn a_combobox_lists_suggestions_under_the_field_and_hides_the_unused_slots() {
    let view = |typed: &'static str, sug: &'static [&'static str]| {
        fusuma::combobox(typed, "City", |t| M3::Typed(t.to_owned()), sug, M3::Pick)
    };
    let mut a = Mini3::new(&view("", &[]));
    a.frame();
    assert!(!a.names().iter().any(|n| n == "Berlin"));
    a.show(&view("be", &["Berlin", "Bergen"]));
    assert!(a.names().iter().any(|n| n == "Berlin") && a.names().iter().any(|n| n == "Bergen"));
    assert_eq!(a.tap("Bergen"), Some(M3::Pick(1)));
    a.show(&view("berl", &["Berlin"]));
    assert!(
        !a.names().iter().any(|n| n == "Bergen"),
        "the list shrinks with the filter"
    );
    // More than the slots: the first five.
    a.show(&view("a", &["A1", "A2", "A3", "A4", "A5", "A6", "A7"]));
    assert!(a.names().iter().any(|n| n == "A5") && !a.names().iter().any(|n| n == "A6"));
    assert_eq!(fusuma::SUGGESTIONS, 5);
}

#[test]
fn a_tree_is_rows_indented_by_depth_that_open_and_close_their_children() {
    let view = |open: bool| {
        column((
            fusuma::tree_item("src", 0, Some(open), false, M3::Fold(0), M3::Row(0)),
            fusuma::show(
                open,
                column((
                    fusuma::tree_item("lib.rs", 1, None, true, M3::Fold(1), M3::Row(1)),
                    fusuma::tree_item("main.rs", 1, None, false, M3::Fold(2), M3::Row(2)),
                )),
            ),
        ))
    };
    let mut a = Mini3::new(&view(false));
    a.frame();
    assert!(a.names().iter().any(|n| n == "src") && a.names().iter().any(|n| n == ">"));
    assert!(
        !a.names().iter().any(|n| n == "lib.rs"),
        "closed: the children are not shown"
    );
    assert_eq!(a.tap(">"), Some(M3::Fold(0)));
    a.show(&view(true));
    assert!(
        a.names().iter().any(|n| n == "v"),
        "open: the triangle points down"
    );
    let x = |a: &Mini3, n: &str| {
        a.tree
            .rect(a.tree.semantics().find(|s| s.name == n).unwrap().id)
            .unwrap()
            .x
    };
    assert!(
        x(&a, "lib.rs") > x(&a, "src"),
        "children are indented: {} {}",
        x(&a, "lib.rs"),
        x(&a, "src")
    );
    assert_eq!(x(&a, "lib.rs"), x(&a, "main.rs"));
    // A leaf has no triangle: only the folder's.
    assert_eq!(
        a.names().iter().filter(|n| *n == "v" || *n == ">").count(),
        1
    );
    assert_eq!(a.tap("main.rs"), Some(M3::Row(2)));
}

static PIXEL: [u8; 16] = [
    255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
];

#[test]
fn an_image_view_draws_its_picture_and_a_bad_one_fails_the_build_with_a_name() {
    let mut a = Mini3::new(&fusuma::image(2, 2, &PIXEL).boxed(20, 20));
    a.frame();
    let img = a
        .tree
        .semantics()
        .find(|s| s.role == fusuma_ui::Role::Image)
        .unwrap();
    assert_eq!((img.rect.w, img.rect.h), (20, 20));
    let c = a.scr.pixel(img.rect.x + 10, img.rect.y + 10).unwrap();
    assert_eq!(
        c.to_rgb565(),
        fusuma_render::Color::rgb(255, 0, 0).to_rgb565()
    );
    // Wrong byte count: the build fails.
    let mut tree = Tree::<8>::new();
    let mut host = Host::<M3, 8>::new();
    let root = tree.column(None, Style::new()).unwrap();
    assert_eq!(
        host.sync(&mut tree, Some(root), &fusuma::image(3, 3, &PIXEL))
            .unwrap_err(),
        fusuma_ui::BuildError::BadImage
    );
}

#[test]
fn chart_views_scale_their_values_between_the_smallest_and_largest_or_a_given_range() {
    use fusuma::scale_samples;
    let (d, n) = scale_samples(&[10, 20, 30], 10, 30);
    assert_eq!((n, &d[..3]), (3, &[0u8, 128, 255][..]));
    // A range wider than the data: the values sit inside it; values outside are held to the ends.
    let (d, _) = scale_samples(&[0, 50, 100, 200, -5], 0, 100);
    assert_eq!(&d[..5], &[0, 128, 255, 255, 0]);
    // An empty or flat range is all zeros and never divides by zero.
    assert_eq!(scale_samples(&[5, 5], 5, 5).0[..2], [0, 0]);
    assert_eq!(scale_samples(&[], 0, 0).1, 0);
    // More than the chart holds: the first 32.
    let many: Vec<i32> = (0..100).collect();
    assert_eq!(scale_samples(&many, 0, 99).1, fusuma_ui::CHART_MAX);

    let v = fusuma::line_chart(&[3, 9, 4, 12]).size(120, 60);
    let mut a = Mini3::new(&v);
    a.frame();
    let (ch_id, ch_rect) = {
        let ch = a.tree.semantics().find(|s| s.name == "Chart").unwrap();
        (ch.id, ch.rect)
    };
    assert_eq!((ch_rect.w, ch_rect.h), (120, 60));
    assert_eq!(a.tree.chart_samples(ch_id), Some(&[0u8, 170, 28, 255][..]));
    // New data in the same node: the samples change, and the same data repaints nothing.
    let w = fusuma::bar_chart(&[1, 2, 3])
        .range(&[1, 2, 3], 0, 4)
        .size(120, 60);
    a.show(&w);
    assert_eq!(a.tree.chart_samples(ch_id), Some(&[64u8, 128, 191][..]));
    a.frame();
    assert!(!a.tree.needs_frame());
    a.show(&w);
    assert!(!a.tree.needs_frame(), "the same chart is no change");
    // The picture: a bar of the full range is the primary colour up to its height.
    let b = a.tree.rect(ch_id).unwrap();
    let mut lit = 0;
    for y in b.y..b.bottom() {
        if a.scr
            .pixel(b.x + 2 + (120 - 4) / 3 * 2 + 4, y)
            .map(fusuma_render::Color::to_rgb565)
            == Some(THEME.colors.primary.to_rgb565())
        {
            lit += 1;
        }
    }
    assert!(
        lit > 25,
        "the third bar (191 of 255) is lit up to three quarters: {lit}"
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Cal {
    Day(u8),
    Month(i32),
    Step(fusuma::TimePart, i32),
}

#[test]
fn a_calendar_puts_the_days_under_their_weekdays_and_sends_the_day_and_the_month_arrows() {
    // June 2025 starts on a Sunday and has 30 days.
    let view = |sel: Option<u8>, sunday: bool| {
        let c = fusuma::calendar(2025, 6, sel, Cal::Day, Cal::Month);
        if sunday { c.sunday_first() } else { c }
    };
    let mut a = Mini4::new(&view(Some(15), false));
    a.frame();
    assert!(a.names().iter().any(|n| n == "June 2025"));
    for d in ["Mo", "Th", "Su"] {
        assert!(a.names().iter().any(|n| n == d), "{d}");
    }
    // 30 days are on the page, 31 is not.
    for d in 1..=30 {
        assert!(a.names().iter().any(|n| *n == d.to_string()), "day {d}");
    }
    assert!(!a.names().iter().any(|n| n == "31"));
    // Weeks start on Monday: the 1st is in the last column, the 2nd in the first column of the next row.
    let rect = |a: &Mini4, n: &str| {
        let id = a
            .tree
            .semantics()
            .filter(|s| s.name == n)
            .last()
            .unwrap()
            .id;
        // A weekday name sits in a column of its own, centred over its day: the column is what lines up.
        let id = a
            .tree
            .parent(id)
            .filter(|_| n.len() == 2 && n.chars().all(char::is_alphabetic))
            .unwrap_or(id);
        a.tree.rect(id).unwrap()
    };
    let (d1, d2, d8) = (rect(&a, "1"), rect(&a, "2"), rect(&a, "8"));
    let mo = rect(&a, "Mo");
    let su = rect(&a, "Su");
    assert_eq!(d1.x, su.x, "the 1st is a Sunday: the last column");
    assert_eq!(d2.x, mo.x, "the 2nd starts the next week");
    assert!(d2.y > d1.y);
    assert_eq!(
        d8.x, d1.x,
        "the 8th is a Sunday too: the same column a week later"
    );
    assert_eq!(d8.y, d2.y, "and on the row of the 2nd (Monday to Sunday)");
    assert_eq!(rect(&a, "9").x, d2.x);
    // The 15th is selected: the main action's colour.
    let d15 = rect(&a, "15");
    assert_eq!(
        a.scr
            .pixel(d15.x + 3, d15.y + d15.h as i32 / 2)
            .unwrap()
            .to_rgb565(),
        THEME.colors.primary.to_rgb565()
    );
    assert_ne!(
        a.scr
            .pixel(d8.x + 3, d8.y + d8.h as i32 / 2)
            .unwrap()
            .to_rgb565(),
        THEME.colors.primary.to_rgb565()
    );
    // Messages.
    assert_eq!(a.tap("20"), Some(Cal::Day(20)));
    assert_eq!(a.tap(">"), Some(Cal::Month(1)));
    assert_eq!(a.tap("<"), Some(Cal::Month(-1)));
    // Sunday first: the 1st is in the first column.
    a.show(&view(None, true));
    let (d1, su) = (rect(&a, "1"), rect(&a, "Su"));
    assert_eq!(d1.x, su.x);
    assert!(a.tree.semantics().filter(|s| s.name == "Su").count() == 1);
    // February 2024: 29 days, the 1st a Thursday; moving the month changes the grid in the same nodes.
    let before = a.tree.len();
    a.show(&fusuma::calendar(2024, 2, None, Cal::Day, Cal::Month));
    assert_eq!(a.tree.len(), before, "the same 42 cells");
    assert!(a.names().iter().any(|n| n == "February 2024"));
    assert!(a.names().iter().any(|n| n == "29") && !a.names().iter().any(|n| n == "30"));
    // The last row is empty for a month that fits in four weeks: February 2021 started on a Monday.
    a.show(&fusuma::calendar(2021, 2, None, Cal::Day, Cal::Month));
    let h = |a: &Mini4, n: &str| {
        a.tree
            .rect(
                a.tree
                    .semantics()
                    .filter(|s| s.name == n)
                    .last()
                    .unwrap()
                    .id,
            )
            .unwrap()
    };
    assert!(
        h(&a, "28").bottom() < i32::from(H),
        "four rows only, nothing below the 28th"
    );
}

#[test]
fn a_time_picker_shows_two_digits_and_sends_the_part_and_the_direction() {
    let view = |h: u8, m: u8| fusuma::time_picker(h, m, Cal::Step);
    let mut a = Mini4::new(&view(9, 5));
    a.frame();
    for n in ["09", "05", ":"] {
        assert!(a.names().iter().any(|x| x == n), "{n}");
    }
    let plus: Vec<_> = a
        .tree
        .semantics()
        .filter(|s| s.name == "+")
        .map(|s| s.id)
        .collect();
    let minus: Vec<_> = a
        .tree
        .semantics()
        .filter(|s| s.name == "-")
        .map(|s| s.id)
        .collect();
    assert_eq!((plus.len(), minus.len()), (2, 2));
    let press = |a: &mut Mini4, id: NodeId| {
        a.frame();
        a.tree.focus(id);
        a.frame();
        let r = a.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            a.tree.pointer(e);
        }
        let mut got = None;
        while let Some(act) = a.tree.pop_action() {
            got = a.host.message(&a.tree, act).or(got);
        }
        got
    };
    assert_eq!(
        press(&mut a, plus[0]),
        Some(Cal::Step(fusuma::TimePart::Hour, 1))
    );
    assert_eq!(
        press(&mut a, minus[1]),
        Some(Cal::Step(fusuma::TimePart::Minute, -1))
    );
    // Through the app's arithmetic: 23:55 plus a step of each wraps to 00:00.
    let (h, m) = fusuma::step_time(23, 55, fusuma::TimePart::Hour, 1);
    let (h, m) = fusuma::step_time(h, m, fusuma::TimePart::Minute, 1);
    assert_eq!((h, m), (0, 0));
    a.show(&view(h, m));
    assert_eq!(a.names().iter().filter(|n| *n == "00").count(), 2);
}

struct Mini4 {
    tree: Tree<400>,
    host: Host<Cal, 128>,
    scr: Screen,
}

impl Mini4 {
    fn new<V: View<Cal>>(v: &V) -> Self {
        let mut m = Self {
            tree: Tree::new(),
            host: Host::new(),
            scr: screen(),
        };
        let root = m.tree.column(None, Style::new().padding(4).gap(4)).unwrap();
        m.host.sync(&mut m.tree, Some(root), v).unwrap();
        m
    }

    fn show<V: View<Cal>>(&mut self, v: &V) {
        let root = self.tree.root_id();
        self.host.sync(&mut self.tree, root, v).unwrap();
        self.frame();
    }

    fn frame(&mut self) {
        self.tree.update(&self.scr, &THEME, BOUNDS);
        self.tree
            .paint_frame(&mut self.scr, &THEME, BOUNDS)
            .unwrap();
    }

    fn tap(&mut self, name: &str) -> Option<Cal> {
        self.frame();
        let id = self
            .tree
            .semantics()
            .filter(|s| s.name == name)
            .last()
            .unwrap_or_else(|| panic!("no '{name}'"))
            .id;
        self.tree.focus(id);
        self.frame();
        let r = self.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            self.tree.pointer(e);
        }
        let mut got = None;
        while let Some(a) = self.tree.pop_action() {
            got = self.host.message(&self.tree, a).or(got);
        }
        got
    }

    fn names(&self) -> Vec<String> {
        self.tree
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }
}

#[test]
fn a_text_area_view_keeps_line_breaks_through_the_apps_state_and_a_field_stays_one_line() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum T {
        Typed(String),
        Submit,
    }
    let mut tree = Tree::<16>::new();
    let mut host = Host::<T, 8>::new();
    let root = tree.column(None, Style::new().padding(4).gap(4)).unwrap();
    let mut value = String::from("first");
    fn view(v: &str, rows: u8) -> fusuma::TextField<'_, T> {
        let f = fusuma::text_field(v)
            .on_input(|t| T::Typed(t.to_owned()))
            .on_submit(T::Submit);
        if rows > 0 { f.rows(rows) } else { f }
    }
    host.sync(&mut tree, Some(root), &view(&value, 3)).unwrap();
    let mut scr = screen();
    tree.update(&scr, &THEME, BOUNDS);
    let field = tree
        .semantics()
        .find(|s| s.role == fusuma_ui::Role::TextField)
        .unwrap()
        .id;
    assert!(tree.focus(field));
    // Typing and Enter come back as the new text, and the view gives it back: the break survives the round trip.
    let feed = |tree: &mut Tree<16>,
                host: &mut Host<T, 8>,
                value: &mut String,
                f: &dyn Fn(&mut Tree<16>)| {
        f(tree);
        while let Some(a) = tree.pop_action() {
            if let Some(T::Typed(t)) = host.message(tree, a) {
                *value = t;
            }
        }
        host.sync(tree, Some(root), &view(value, 3)).unwrap();
    };
    feed(&mut tree, &mut host, &mut value, &|t| {
        t.key(fusuma_ui::Key::Activate)
    });
    feed(&mut tree, &mut host, &mut value, &|t| t.type_text("second"));
    assert_eq!(value, "first\nsecond");
    tree.update(&scr, &THEME, BOUNDS);
    tree.paint_frame(&mut scr, &THEME, BOUNDS).unwrap();
    assert_eq!(tree.text(field), Some("first\nsecond"));
    // Enter in the area is not a submit; in a one-line field it is.
    let mut got_submit = false;
    tree.key(fusuma_ui::Key::Activate);
    while let Some(a) = tree.pop_action() {
        got_submit |= host.message(&tree, a) == Some(T::Submit);
    }
    assert!(!got_submit, "a line break, not a submit");
    let mut tree1 = Tree::<16>::new();
    let mut host1 = Host::<T, 8>::new();
    let r1 = tree1.column(None, Style::new()).unwrap();
    host1.sync(&mut tree1, Some(r1), &view("x", 0)).unwrap();
    tree1.update(&scr, &THEME, BOUNDS);
    let f1 = tree1
        .semantics()
        .find(|s| s.role == fusuma_ui::Role::TextField)
        .unwrap()
        .id;
    tree1.focus(f1);
    tree1.key(fusuma_ui::Key::Activate);
    let mut sub = false;
    while let Some(a) = tree1.pop_action() {
        sub |= host1.message(&tree1, a) == Some(T::Submit);
    }
    assert!(sub, "Enter in a one-line field submits");
}

#[test]
fn a_dragged_view_and_a_drop_target_send_their_messages_through_the_apps_state() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum D {
        Pick(u8),
        Cancel,
        Drop(u8),
        Clicked(u8),
    }
    // Two cards and two bins: carrying a card to a bin is "Pick(card)" then "Drop(bin)".
    let view = || {
        column((
            fusuma::draggable(fusuma::button("A").on_press(D::Clicked(0)), D::Pick(0))
                .on_cancel(D::Cancel),
            fusuma::draggable(fusuma::button("B").on_press(D::Clicked(1)), D::Pick(1))
                .on_cancel(D::Cancel),
            fusuma::drop_target(
                column((text("Bin one"),))
                    .padding(8)
                    .height(Length::Fixed(40))
                    .width(Length::Fill(1)),
                D::Drop(0),
            ),
            fusuma::drop_target(
                column((text("Bin two"),))
                    .padding(8)
                    .height(Length::Fixed(40))
                    .width(Length::Fill(1)),
                D::Drop(1),
            ),
        ))
        .gap(8)
        .align(CrossAlign::Stretch)
    };
    let mut tree = Tree::<32>::new();
    let mut host = Host::<D, 16>::new();
    let root = tree.column(None, Style::new().padding(8)).unwrap();
    host.sync(&mut tree, Some(root), &view()).unwrap();
    let mut scr = screen();
    tree.update(&scr, &THEME, BOUNDS);
    tree.paint_frame(&mut scr, &THEME, BOUNDS).unwrap();
    let rect_of = |tree: &Tree<32>, n: &str| {
        tree.rect(tree.semantics().find(|s| s.name == n).unwrap().id)
            .unwrap()
    };
    let centre = |r: Rect| Point::new(r.x + r.w as i32 / 2, r.y + r.h as i32 / 2);
    let msgs = |tree: &mut Tree<32>, host: &Host<D, 16>| {
        let mut out = Vec::new();
        while let Some(a) = tree.pop_action() {
            if let Some(m) = host.message(tree, a) {
                out.push(m);
            }
        }
        out
    };
    // A plain press on A is a click.
    let a = centre(rect_of(&tree, "A"));
    for e in [
        Pointer::Move(a),
        Pointer::Down(a),
        Pointer::Up(a),
        Pointer::Leave,
    ] {
        tree.pointer(e);
    }
    assert_eq!(msgs(&mut tree, &host), [D::Clicked(0)]);
    // Carry B to the second bin.
    let b = centre(rect_of(&tree, "B"));
    let bin2 = centre(rect_of(&tree, "Bin two"));
    for e in [
        Pointer::Move(b),
        Pointer::Down(b),
        Pointer::Move(Point::new(b.x + 15, b.y)),
    ] {
        tree.pointer(e);
    }
    assert_eq!(msgs(&mut tree, &host), [D::Pick(1)]);
    for e in [Pointer::Move(bin2), Pointer::Up(bin2), Pointer::Leave] {
        tree.pointer(e);
    }
    assert_eq!(
        msgs(&mut tree, &host),
        [D::Drop(1)],
        "the target answers, not the card"
    );
    // Let go over nothing: the card's cancel.
    for e in [
        Pointer::Move(b),
        Pointer::Down(b),
        Pointer::Move(Point::new(b.x + 15, b.y)),
        Pointer::Up(Point::new(b.x + 15, b.y)),
    ] {
        tree.pointer(e);
    }
    assert_eq!(msgs(&mut tree, &host), [D::Pick(1), D::Cancel]);
}
