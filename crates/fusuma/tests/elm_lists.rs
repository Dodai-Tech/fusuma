#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]
//! Keyed lists, memoised views, hand-built subtrees, and the views for the other widgets.

use fusuma::{
    Cx, Handler, Host, Imperative, button, card, column, dialog, each, gestures, imperative, item,
    memo, menu, named, radio, row, scroll, slider, switch, tab, tab_bar, text, text_field,
};
use fusuma_render::{Point, Rect};
use fusuma_render_vello_cpu::VelloCpu;
use fusuma_ui::{
    Action, ActionKind, CrossAlign, Id, NodeId, Pointer, Style, Swipe, SwipeDir, Theme, Tree,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Msg {
    Pick(u32),
    Typed(u32),
    Radio(u8),
    Switch(bool),
    Slide(u16),
    Dismiss,
    Hold,
    Swipe(SwipeDir),
    Plain,
}

const THEME: Theme = Theme::DARK;

fn measure() -> VelloCpu {
    fusuma_render_vello_cpu::new(8, 8).unwrap()
}

struct Page {
    tree: Tree<64>,
    host: Host<Msg, 32>,
    root: NodeId,
}

impl Page {
    fn new() -> Self {
        let mut tree = Tree::new();
        let root = tree
            .column(
                None,
                Style::new().gap(2).padding(4).align(CrossAlign::Start),
            )
            .unwrap();
        Self {
            tree,
            host: Host::new(),
            root,
        }
    }

    fn sync<V: fusuma::Children<Msg>>(&mut self, children: &V) {
        self.host
            .sync_children(&mut self.tree, self.root, children)
            .unwrap();
        self.tree
            .update(&measure(), &THEME, Rect::new(0, 0, 300, 600));
    }

    fn names(&self) -> Vec<String> {
        self.tree
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }

    fn tap(&mut self, id: NodeId) -> Option<Msg> {
        let r = self.tree.rect(id).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            self.tree.pointer(e);
        }
        self.message()
    }

    fn message(&mut self) -> Option<Msg> {
        let mut found = None;
        while let Some(a) = self.tree.pop_action() {
            found = found.or(self.host.message(&self.tree, a));
        }
        found
    }

    fn named(&self, label: &str) -> NodeId {
        self.tree
            .semantics()
            .find(|s| s.name == label)
            .map(|s| s.id)
            .unwrap_or_else(|| panic!("no {label}"))
    }
}

#[derive(Clone, Copy)]
struct Row {
    id: u32,
    name: &'static str,
}

fn rows(list: &[Row]) -> impl fusuma::Children<Msg> + '_ {
    (column((each(
        list,
        |r| r.id,
        |r| item(r.name, false).on_select(Msg::Pick(r.id)),
    ),))
    .gap(2),)
}

const A: Row = Row {
    id: 1,
    name: "alpha",
};
const B: Row = Row {
    id: 2,
    name: "beta",
};
const C: Row = Row {
    id: 3,
    name: "gamma",
};
const D: Row = Row {
    id: 4,
    name: "delta",
};

fn rows_of(p: &Page) -> Vec<(u32, NodeId)> {
    let col = p.tree.first_child(p.root).unwrap();
    p.tree
        .children(col)
        .map(|n| (p.tree.key_of(n), n))
        .collect()
}

#[test]
fn rows_that_stay_keep_their_node_and_the_order_follows_the_list() {
    let mut p = Page::new();
    p.sync(&rows(&[A, B, C]));
    let before = rows_of(&p);
    assert_eq!(before.iter().map(|r| r.0).collect::<Vec<_>>(), [1, 2, 3]);
    // Reorder, remove one, add one at the top: the survivors keep their nodes.
    p.sync(&rows(&[D, C, A]));
    let after = rows_of(&p);
    assert_eq!(after.iter().map(|r| r.0).collect::<Vec<_>>(), [4, 3, 1]);
    let node = |rows: &[(u32, NodeId)], k| rows.iter().find(|r| r.0 == k).map(|r| r.1);
    assert_eq!(node(&after, 3), node(&before, 3), "gamma is the same node");
    assert_eq!(node(&after, 1), node(&before, 1), "alpha too");
    assert_eq!(node(&after, 2), None);
    assert!(
        !p.tree.contains(node(&before, 2).unwrap()),
        "beta was removed"
    );
    assert_eq!(p.names(), ["delta", "gamma", "alpha"]);
    // A message goes to the row it was made for, wherever the row is now.
    let gamma = node(&after, 3).unwrap();
    assert_eq!(p.tap(gamma), Some(Msg::Pick(3)));
    let delta = node(&after, 4).unwrap();
    assert_eq!(p.tap(delta), Some(Msg::Pick(4)));
}

#[test]
fn a_random_run_of_changes_always_shows_exactly_the_list_in_order() {
    let pool = [
        A,
        B,
        C,
        D,
        Row { id: 5, name: "eps" },
        Row {
            id: 6,
            name: "zeta",
        },
    ];
    let mut seed = 11u64;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) as usize) % n
    };
    let mut p = Page::new();
    for step in 0..200 {
        // A random selection of the pool in a random order.
        let mut list: Vec<Row> = Vec::new();
        for _ in 0..next(7) {
            let r = pool[next(pool.len())];
            if !list.iter().any(|x| x.id == r.id) {
                list.push(r);
            }
        }
        let nodes_before = p.tree.len();
        p.sync(&rows(&list));
        let want: Vec<&str> = list.iter().map(|r| r.name).collect();
        assert_eq!(p.names(), want, "step {step}");
        assert_eq!(
            rows_of(&p).iter().map(|r| r.0).collect::<Vec<_>>(),
            list.iter().map(|r| r.id).collect::<Vec<_>>()
        );
        assert!(
            p.tree.len() <= nodes_before.max(2 + list.len() + 4),
            "nodes do not pile up: {}",
            p.tree.len()
        );
    }
}

/// The text of each row's field lives in the program's state, as a controlled field requires.
fn fields<'a>(list: &'a [Row], texts: &'a [(u32, &'a str)]) -> impl fusuma::Children<Msg> + 'a {
    (column((each(
        list,
        |r| r.id,
        move |r| {
            let value = texts.iter().find(|t| t.0 == r.id).map_or("", |t| t.1);
            text_field(value).on_input(|_| Msg::Typed(0))
        },
    ),)),)
}

#[test]
fn a_text_field_in_a_row_keeps_its_node_text_and_focus_when_the_list_is_reordered() {
    let mut p = Page::new();
    p.sync(&fields(&[A, B, C], &[]));
    let col = p.tree.first_child(p.root).unwrap();
    let second = p.tree.children(col).nth(1).unwrap();
    p.tree.focus(second);
    p.tree.type_text("hello");
    assert_eq!(p.message(), Some(Msg::Typed(0)));
    // The program stores what was typed against the row, and the view says so.
    let texts = [(2, "hello")];
    p.sync(&fields(&[C, B, A], &texts));
    let col = p.tree.first_child(p.root).unwrap();
    let moved: Vec<NodeId> = p.tree.children(col).collect();
    assert_eq!(moved[1], second, "the middle row is the same node");
    assert_eq!(p.tree.text(second), Some("hello"));
    assert!(p.tree.is_focused(second), "and it kept the focus");
    // Move that row to the end: the text goes with it.
    p.sync(&fields(&[C, A, B], &texts));
    let col = p.tree.first_child(p.root).unwrap();
    assert_eq!(p.tree.children(col).last(), Some(second));
    assert_eq!(p.tree.text(second), Some("hello"));
    // A controlled field shows what the state says: a state that forgot the text clears the field.
    p.sync(&fields(&[C, A, B], &[]));
    assert_eq!(p.tree.text(second), Some(""));
}

struct Counter(std::cell::Cell<u32>);

#[test]
fn a_memoised_view_is_skipped_while_its_value_is_the_same_and_keeps_its_messages() {
    let seen = Counter(std::cell::Cell::new(0));
    let view = |label: &'static str, value: u32| {
        seen.0.set(seen.0.get() + 1);
        (memo(
            value as u16,
            card((text(label), button("go").on_press(Msg::Plain))),
        ),)
    };
    let mut p = Page::new();
    p.sync(&view("one", 1));
    assert_eq!(p.names(), ["one", "go"]);
    // The label changed but the value did not: the subtree is skipped, so the old text stays.
    p.sync(&view("two", 1));
    assert_eq!(p.names(), ["one", "go"], "skipped");
    // Its message still works after a pass that did not visit it.
    let go = p.named("go");
    assert_eq!(p.tap(go), Some(Msg::Plain));
    // A new value makes it again.
    p.sync(&view("two", 2));
    assert_eq!(p.names(), ["two", "go"]);
    p.sync(&view("two", 2));
    assert_eq!(p.tap(go), Some(Msg::Plain));
}

struct Hand;

impl Imperative<Msg> for Hand {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, Msg, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, fusuma_ui::BuildError> {
        let col = cx.tree.column(parent, Style::new().gap(2))?;
        cx.tree
            .label(Some(col), "by hand", fusuma_ui::TextRole::Body)?;
        let b = cx.tree.button(
            Some(col),
            "hand button",
            fusuma_ui::ButtonVariant::Secondary,
        )?;
        cx.msgs.on(b, Handler::Press(Msg::Plain));
        Ok(col)
    }

    fn update<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, Msg, N, K>, node: NodeId) {
        let b = cx.tree.children(node).nth(1).unwrap();
        cx.msgs.on(b, Handler::Press(Msg::Plain));
    }
}

#[test]
fn hand_built_code_sits_in_a_page_next_to_views_and_its_messages_work() {
    let mut p = Page::new();
    let view = || (text("before"), imperative(Hand), text("after"));
    p.sync(&view());
    assert_eq!(p.names(), ["before", "by hand", "hand button", "after"]);
    p.sync(&view());
    let b = p.named("hand button");
    assert_eq!(
        p.tap(b),
        Some(Msg::Plain),
        "the handler was kept across a pass"
    );
    assert_eq!(p.names().len(), 4, "nothing was built twice");
}

#[test]
fn radio_switch_slider_and_text_field_views_send_what_the_widgets_send() {
    let mut p = Page::new();
    let view = |on: bool, pick: u8, value: &'static str| {
        (
            radio("one", pick == 1).on_select(Msg::Radio(1)),
            radio("two", pick == 2).on_select(Msg::Radio(2)),
            switch("power", on).on_toggle(Msg::Switch),
            slider(300).on_change(Msg::Slide),
            text_field(value)
                .placeholder("name")
                .on_input(|_| Msg::Typed(0)),
        )
    };
    p.sync(&view(false, 1, ""));
    let two = p.named("two");
    assert_eq!(p.tap(two), Some(Msg::Radio(2)));
    // The program's answer is the state; the view is made again from it.
    p.sync(&view(false, 2, ""));
    assert!(
        p.tree.is_checked(two) && !p.tree.is_checked(p.named("one")),
        "one is cleared"
    );
    let power = p.named("power");
    assert_eq!(p.tap(power), Some(Msg::Switch(true)));
    p.sync(&view(true, 2, ""));
    assert!(p.tree.is_checked(power));
    // The slider is moved by the view while nobody holds it.
    p.sync(&view(true, 2, ""));
    let field = p
        .tree
        .semantics()
        .find(|s| s.role == fusuma_ui::Role::TextField)
        .unwrap()
        .id;
    p.tree.focus(field);
    p.tree.type_text("ab");
    assert_eq!(p.message(), Some(Msg::Typed(0)));
    // The view says the value is what was typed: nothing is set again, so the caret stays.
    p.sync(&view(true, 2, "ab"));
    assert_eq!(p.tree.text(field), Some("ab"));
    assert_eq!(p.tree.input_cursor(field), Some((2, 2)));
    // The view can also set it from the state (a reset).
    p.sync(&view(true, 2, ""));
    assert_eq!(p.tree.text(field), Some(""));
}

#[test]
fn list_rows_and_tabs_choose_one_and_a_scroll_container_scrolls() {
    let mut p = Page::new();
    let view = |sel: u32, tabn: u32| {
        (
            tab_bar((
                tab("A", tabn == 0).on_select(Msg::Pick(100)),
                tab("B", tabn == 1).on_select(Msg::Pick(101)),
            )),
            scroll((
                item("r0", sel == 0).on_select(Msg::Pick(0)),
                item("r1", sel == 1).on_select(Msg::Pick(1)),
                item("r2", sel == 2).on_select(Msg::Pick(2)),
                item("r3", sel == 3).on_select(Msg::Pick(3)),
                item("r4", sel == 4).on_select(Msg::Pick(4)),
            ))
            .height(fusuma_ui::Length::Fixed(60))
            .width(fusuma_ui::Length::Fixed(120))
            .align(CrossAlign::Stretch),
        )
    };
    p.sync(&view(0, 0));
    let b = p.named("B");
    assert_eq!(p.tap(b), Some(Msg::Pick(101)));
    p.sync(&view(0, 1));
    assert!(p.tree.is_checked(b) && !p.tree.is_checked(p.named("A")));
    let r1 = p.named("r1");
    assert_eq!(p.tap(r1), Some(Msg::Pick(1)));
    p.sync(&view(1, 1));
    assert!(p.tree.is_checked(r1) && !p.tree.is_checked(p.named("r0")));
    let list = p
        .tree
        .semantics()
        .find(|s| s.role == fusuma_ui::Role::ScrollArea)
        .unwrap()
        .id;
    assert!(
        p.tree.scroll_max(list).unwrap() > 0,
        "five rows in 60 px scroll"
    );
}

#[test]
fn a_dialog_is_open_when_the_state_says_so_and_dismissing_it_is_a_message() {
    let mut p = Page::new();
    let view = |open: bool| {
        (
            button("ask").on_press(Msg::Plain),
            dialog(open, (text("Sure?"), button("yes").on_press(Msg::Pick(1))))
                .padding(8)
                .on_dismiss(Msg::Dismiss),
        )
    };
    p.sync(&view(false));
    assert!(
        !p.names().contains(&"Sure?".to_owned()),
        "closed: not on the screen"
    );
    p.sync(&view(true));
    p.tree.advance(&THEME, 1000);
    p.sync(&view(true));
    assert!(p.names().contains(&"Sure?".to_owned()));
    // A press outside dismisses it: the app is told, and closes it by its state.
    p.tree.pointer(Pointer::Down(Point::new(2, 590)));
    p.tree.pointer(Pointer::Up(Point::new(2, 590)));
    assert_eq!(p.message(), Some(Msg::Dismiss));
    p.sync(&view(false));
    p.tree.advance(&THEME, 1000);
    p.sync(&view(false));
    assert!(!p.names().contains(&"Sure?".to_owned()));
}

#[test]
fn a_menu_hangs_from_the_node_called_its_anchor_while_the_state_says_open() {
    let mut p = Page::new();
    let anchor = Id::new(5);
    let view = |open: bool| {
        (
            row((named(anchor, button("menu").on_press(Msg::Plain)),)),
            menu(open, anchor, (item("copy", false).on_select(Msg::Pick(7)),))
                .on_dismiss(Msg::Dismiss),
        )
    };
    p.sync(&view(false));
    p.sync(&view(true));
    let m = p
        .tree
        .semantics()
        .find(|s| s.role == fusuma_ui::Role::Menu)
        .expect("open")
        .id;
    let (mr, ar) = (
        p.tree.rect(m).unwrap(),
        p.tree.rect(p.tree.find(anchor).unwrap()).unwrap(),
    );
    assert!(mr.y >= ar.bottom(), "below its anchor: {mr:?} {ar:?}");
    let copy = p.named("copy");
    assert_eq!(p.tap(copy), Some(Msg::Pick(7)));
    p.sync(&view(false));
    assert!(
        p.tree.semantics().all(|s| s.role != fusuma_ui::Role::Menu),
        "closed"
    );
}

#[test]
fn a_view_listens_for_a_long_press_and_a_swipe_on_its_node() {
    let mut p = Page::new();
    let view = || {
        gestures(card((text("swipe me"),)).padding(10))
            .on_long_press(Msg::Hold)
            .on_swipe(Swipe::Horizontal, Msg::Swipe)
    };
    p.sync(&(view(),));
    let card_node = p.tree.first_child(p.root).unwrap();
    let r = p.tree.rect(card_node).unwrap();
    let at = Point::new(r.x + 5, r.y + 5);
    p.tree.pointer(Pointer::Down(at));
    p.tree.pointer(Pointer::Move(Point::new(at.x + 60, at.y)));
    assert_eq!(p.message(), Some(Msg::Swipe(SwipeDir::Right)));
    p.tree.pointer(Pointer::Up(at));
    p.tree.pointer(Pointer::Leave);
    p.tree.pointer(Pointer::Down(at));
    p.tree.advance(&THEME, 600);
    assert_eq!(p.message(), Some(Msg::Hold));
    p.tree.pointer(Pointer::Up(at));
    // Without the modifier the node listens for nothing.
    p.sync(&(card((text("swipe me"),)).padding(10),));
    p.tree.pointer(Pointer::Down(at));
    p.tree.pointer(Pointer::Move(Point::new(at.x + 60, at.y)));
    assert_eq!(p.message(), None);
    let _ = (Action {
        source: card_node,
        kind: ActionKind::Clicked,
    },);
}
