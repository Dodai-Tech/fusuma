#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The components made of views (alert, banner, tag, chip, segmented control, breadcrumbs, pagination, steps, stat card,
//! empty state, skeleton, snackbar): what they build, which message each part sends, that the page window is right for
//! every page of every size, and that a pass with nothing changed costs no allocation.

use allocation_counter::measure;
use fusuma::{
    Host, PAGE_SLOTS, PageSlot, StepState, Tone, View, alert, banner, breadcrumbs, chip, column,
    empty_state, page_window, pagination, segmented, skeleton, snackbar, stat_card, step_state,
    steps, tag, text,
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
}

#[derive(Clone, Copy)]
struct State {
    seg: usize,
    page: u32,
    pages: u32,
    step: usize,
    chips: [bool; 3],
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
