#![doc = include_str!("../README.md")]

use accesskit::{
    Action, ActionData, ActionRequest, Live, Node, NodeId as AkId, Rect as AkRect, Role as AkRole,
    Toggled, TreeId, TreeInfo as AkTree, TreeUpdate,
};
use fusuma_render::Rect;
use fusuma_ui::{App, Assist, Assisted, NodeId, Role, Semantic, Tree};

/// The accessibility id of the window node that holds the whole tree.
pub const WINDOW: AkId = AkId(0);

/// The AccessKit id of a widget: its handle plus one (0 is the window).
#[must_use]
pub fn ak_id(id: NodeId) -> AkId {
    AkId(u64::from(id.to_raw()) + 1)
}

/// The widget an AccessKit id names, or `None` for the window or a number that is not a handle.
#[must_use]
pub fn widget_of(id: AkId) -> Option<NodeId> {
    let raw = id.0.checked_sub(1)?;
    u32::try_from(raw).ok().map(NodeId::from_raw)
}

fn role(r: Role) -> AkRole {
    match r {
        Role::Group | Role::Spacer => AkRole::GenericContainer,
        Role::Label => AkRole::Label,
        Role::Heading => AkRole::Heading,
        Role::Icon | Role::Image => AkRole::Image,
        Role::Paragraph => AkRole::Paragraph,
        Role::Button => AkRole::Button,
        Role::Checkbox => AkRole::CheckBox,
        Role::Radio => AkRole::RadioButton,
        Role::Switch => AkRole::Switch,
        Role::Slider => AkRole::Slider,
        Role::Progress => AkRole::ProgressIndicator,
        Role::ListItem => AkRole::ListBoxOption,
        Role::Tab => AkRole::Tab,
        Role::TextField => AkRole::TextInput,
        Role::ScrollArea => AkRole::ScrollView,
        Role::Dialog => AkRole::Dialog,
        Role::Menu => AkRole::Menu,
        Role::Toast => AkRole::Status,
    }
}

fn bounds(r: Rect) -> AkRect {
    AkRect::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.x) + f64::from(r.w),
        f64::from(r.y) + f64::from(r.h),
    )
}

/// The AccessKit node of one widget (without its children).
fn node(s: &Semantic<'_>) -> Node {
    let mut n = Node::new(role(s.role));
    n.set_bounds(bounds(s.rect));
    // An icon's name is its character in the icon font: say nothing rather than a private-use glyph.
    let name = if s.role == Role::Icon { "" } else { s.name };
    match s.role {
        // A text field's content is its value; the name it has for the reader is its hint (what it is for).
        Role::TextField => {
            n.set_value(s.name);
            if !s.hint.is_empty() {
                n.set_placeholder(s.hint);
                n.set_label(s.hint);
            }
        }
        _ if !name.is_empty() => n.set_label(name),
        _ => {}
    }
    if let Some(on) = s.checked {
        n.set_toggled(if on { Toggled::True } else { Toggled::False });
    }
    if let Some(on) = s.selected {
        n.set_selected(on);
    }
    if s.disabled {
        n.set_disabled();
    }
    match (s.role, s.value) {
        (Role::Slider | Role::Progress, Some(v)) => {
            n.set_min_numeric_value(0.0);
            n.set_max_numeric_value(1000.0);
            n.set_numeric_value(f64::from(v));
            if s.role == Role::Slider {
                n.set_numeric_value_step(50.0);
                n.add_action(Action::Increment);
                n.add_action(Action::Decrement);
            }
        }
        (Role::ScrollArea, Some(v)) => {
            n.set_scroll_y(f64::from(v));
            n.add_action(Action::ScrollIntoView);
        }
        _ => {}
    }
    match s.role {
        Role::Button
        | Role::Checkbox
        | Role::Radio
        | Role::Switch
        | Role::ListItem
        | Role::Tab
        | Role::Menu => {
            if !s.disabled {
                n.add_action(Action::Click);
            }
        }
        Role::TextField => n.add_action(Action::ReplaceSelectedText),
        Role::Toast => n.set_live(Live::Polite),
        Role::Dialog => n.set_modal(),
        _ => {}
    }
    if !s.disabled
        && !matches!(
            s.role,
            Role::Group
                | Role::Spacer
                | Role::Label
                | Role::Heading
                | Role::Paragraph
                | Role::Icon
                | Role::Image
                | Role::Progress
                | Role::Toast
        )
    {
        n.add_action(Action::Focus);
    }
    n
}

/// The whole screen of a [`Tree`] as an AccessKit update (see [`update_of`]).
#[must_use]
pub fn tree_update<const N: usize>(tree: &Tree<N>, window_name: &str) -> TreeUpdate {
    update_of(
        |visit| tree.semantics().for_each(|s| visit(&s)),
        window_name,
    )
}

/// The whole screen of an [`App`] as an AccessKit update (see [`update_of`]); an app that does not describe itself gives just the
/// window.
#[must_use]
pub fn app_update<A: App>(app: &A, window_name: &str) -> TreeUpdate {
    update_of(|visit| app.semantics(visit), window_name)
}

/// An update from any source of widgets in reading order: a window named `window_name` holding every widget, nested the way the
/// tree is, and the focused widget as the focus (the window when none is).
#[must_use]
pub fn update_of(
    each: impl FnOnce(&mut dyn FnMut(&Semantic<'_>)),
    window_name: &str,
) -> TreeUpdate {
    let mut window = Node::new(AkRole::Window);
    window.set_label(window_name);
    let mut nodes: Vec<(AkId, Node)> = Vec::new();
    // The open containers: (depth, id, children so far).
    let mut open: Vec<(usize, AkId, Vec<AkId>)> = vec![(0, WINDOW, Vec::new())];
    let mut focus = WINDOW;
    fn close(open: &mut Vec<(usize, AkId, Vec<AkId>)>, nodes: &mut [(AkId, Node)], keep: usize) {
        while open.len() > 1 && open.last().is_some_and(|o| o.0 >= keep) {
            let (_, id, kids) = open.pop().expect("a container is open");
            if let Some(slot) = nodes.iter_mut().rev().find(|(i, _)| *i == id) {
                slot.1.set_children(kids);
            }
        }
    }
    each(&mut |s| {
        // A widget at depth d is a child of the nearest open container above it; close the ones at its depth or deeper.
        close(&mut open, &mut nodes, s.depth + 1);
        let id = ak_id(s.id);
        if let Some(parent) = open.last_mut() {
            parent.2.push(id);
        }
        if s.focused {
            focus = id;
        }
        nodes.push((id, node(s)));
        open.push((s.depth + 1, id, Vec::new()));
    });
    close(&mut open, &mut nodes, 1);
    if let Some((_, _, kids)) = open.pop() {
        window.set_children(kids);
    }
    nodes.push((WINDOW, window));
    TreeUpdate {
        nodes,
        tree: Some(AkTree {
            root: WINDOW,
            toolkit_name: Some("Fusuma".into()),
            toolkit_version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
        tree_id: TreeId::ROOT,
        focus,
    }
}

/// What [`perform`] did with a request: the toolkit's own answer, [`Assisted`].
pub type Outcome = Assisted;

/// What a request asks, as the toolkit's own words; `None` for one this toolkit does not offer.
#[must_use]
pub fn assist_of(request: &ActionRequest) -> Option<(NodeId, Assist<'_>)> {
    let id = widget_of(request.target_node)?;
    let a = match request.action {
        Action::Click => Assist::Click(id),
        Action::Focus => Assist::Focus(id),
        Action::Increment => Assist::Step(id, 1),
        Action::Decrement => Assist::Step(id, -1),
        Action::ReplaceSelectedText => match &request.data {
            Some(ActionData::Value(text)) => Assist::Type(id, text),
            _ => return None,
        },
        Action::ScrollIntoView => Assist::Reveal(id),
        _ => return None,
    };
    Some((id, a))
}

/// Does what an assistive technology asks of the widget it names, on a [`Tree`].
pub fn perform<const N: usize>(tree: &mut Tree<N>, request: &ActionRequest) -> Outcome {
    perform_with(request, |a| tree.assist(a))
}

/// The same for an [`App`] (a window runs an app, not a tree).
pub fn perform_on_app<A: App>(app: &mut A, request: &ActionRequest) -> Outcome {
    perform_with(request, |a| app.assist(a))
}

fn perform_with(request: &ActionRequest, assist: impl FnOnce(Assist<'_>) -> Assisted) -> Outcome {
    if request.target_node == WINDOW {
        // The window takes no action.
        return Outcome::Ignored;
    }
    if widget_of(request.target_node).is_none() {
        // A number that is no handle at all names nothing that is there.
        return Outcome::Gone;
    }
    match assist_of(request) {
        Some((_, a)) => assist(a),
        // A widget that is there but a request that is not offered: an unknown widget is still refused as gone first.
        None => Outcome::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::Rect as R;
    use fusuma_render_vello_cpu::VelloCpu;
    use fusuma_ui::{ButtonVariant, CrossAlign, Style, TextRole, Theme};

    use super::*;

    const THEME: Theme = Theme::DARK;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(300, 300).unwrap()
    }

    struct Ui {
        t: Tree<32>,
        go: NodeId,
        check: NodeId,
        slider: NodeId,
        field: NodeId,
        label: NodeId,
        off: NodeId,
    }

    fn ui() -> Ui {
        let mut t = Tree::<32>::new();
        let root = t
            .column(
                None,
                Style::new().gap(4).padding(8).align(CrossAlign::Stretch),
            )
            .unwrap();
        let label = t.label(Some(root), "Settings", TextRole::Title).unwrap();
        let row = t.row(Some(root), Style::new().gap(4)).unwrap();
        let go = t.button(Some(row), "Save", ButtonVariant::Primary).unwrap();
        let off = t
            .button(Some(row), "Delete", ButtonVariant::Secondary)
            .unwrap();
        let check = t.checkbox(Some(root), "Notify me", false).unwrap();
        let slider = t.slider(Some(root), 300).unwrap();
        let field = t.text_input(Some(root), "Ada").unwrap();
        t.set_enabled(off, false);
        t.update(&screen(), &THEME, R::new(0, 0, 300, 300));
        Ui {
            t,
            go,
            check,
            slider,
            field,
            label,
            off,
        }
    }

    fn find(u: &TreeUpdate, id: NodeId) -> &Node {
        &u.nodes
            .iter()
            .find(|(i, _)| *i == ak_id(id))
            .expect("the widget is in the update")
            .1
    }

    #[test]
    fn the_update_is_a_window_with_every_widget_in_it_roles_names_states_and_values() {
        let u = ui();
        let up = tree_update(&u.t, "Fusuma showcase");
        let info = up.tree.as_ref().unwrap();
        assert_eq!(info.root, WINDOW);
        assert_eq!(info.toolkit_name.as_deref(), Some("Fusuma"));
        let window = &up.nodes.iter().find(|(i, _)| *i == WINDOW).unwrap().1;
        assert_eq!(
            (window.role(), window.label()),
            (AkRole::Window, Some("Fusuma showcase"))
        );
        // The window holds the root column, which holds the label, the row, the checkbox, the slider and the field.
        assert_eq!(window.children().len(), 1);
        let root = up
            .nodes
            .iter()
            .find(|(i, _)| *i == window.children()[0])
            .unwrap();
        assert_eq!(
            root.1.children().len(),
            5,
            "label, row, checkbox, slider, field"
        );
        let (label, go, check, slider, field, off) = (
            find(&up, u.label),
            find(&up, u.go),
            find(&up, u.check),
            find(&up, u.slider),
            find(&up, u.field),
            find(&up, u.off),
        );
        assert_eq!(
            (label.role(), label.label()),
            (AkRole::Heading, Some("Settings")),
            "a title is a heading"
        );
        assert_eq!((go.role(), go.label()), (AkRole::Button, Some("Save")));
        assert!(go.supports_action(Action::Click) && go.supports_action(Action::Focus));
        assert!(
            off.is_disabled() && !off.supports_action(Action::Click),
            "a disabled button cannot be clicked"
        );
        assert_eq!(
            (check.role(), check.toggled(), check.label()),
            (AkRole::CheckBox, Some(Toggled::False), Some("Notify me"))
        );
        assert_eq!(slider.role(), AkRole::Slider);
        assert_eq!(
            (
                slider.numeric_value(),
                slider.min_numeric_value(),
                slider.max_numeric_value()
            ),
            (Some(300.0), Some(0.0), Some(1000.0))
        );
        assert!(
            slider.supports_action(Action::Increment) && slider.supports_action(Action::Decrement)
        );
        assert_eq!(
            (field.role(), field.value()),
            (AkRole::TextInput, Some("Ada"))
        );
        assert!(field.supports_action(Action::ReplaceSelectedText));
        // Bounds are where the widget is.
        let r = u.t.rect(u.go).unwrap();
        let b = go.bounds().unwrap();
        assert_eq!(
            (b.x0, b.y0, b.x1, b.y1),
            (
                f64::from(r.x),
                f64::from(r.y),
                f64::from(r.right()),
                f64::from(r.bottom())
            )
        );
        // Every child named by a parent is in the update, once, and has no other parent.
        let ids: Vec<AkId> = up.nodes.iter().map(|(i, _)| *i).collect();
        let mut seen = std::collections::HashSet::new();
        for (_, n) in &up.nodes {
            for c in n.children() {
                assert!(ids.contains(c), "a named child is in the update");
                assert!(seen.insert(*c), "a node has one parent");
            }
        }
        assert_eq!(
            ids.len(),
            seen.len() + 1,
            "everything but the window is somebody's child"
        );
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "no id twice");
    }

    #[test]
    fn the_focus_follows_the_keyboard_and_state_changes_show_in_the_next_update() {
        let mut u = ui();
        assert_eq!(tree_update(&u.t, "w").focus, WINDOW, "no focus: the window");
        assert!(u.t.focus(u.check));
        assert_eq!(tree_update(&u.t, "w").focus, ak_id(u.check));
        u.t.activate_node(u.check);
        let up = tree_update(&u.t, "w");
        assert_eq!(find(&up, u.check).toggled(), Some(Toggled::True));
        u.t.set_slider(u.slider, 800);
        assert_eq!(
            find(&tree_update(&u.t, "w"), u.slider).numeric_value(),
            Some(800.0)
        );
    }

    fn req(action: Action, id: NodeId, data: Option<ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: ak_id(id),
            data,
        }
    }

    #[test]
    fn requests_do_what_a_person_would_and_refuse_what_cannot_be_done() {
        let mut u = ui();
        // Click presses a button (an action the app sees) and ticks a checkbox.
        assert_eq!(
            perform(&mut u.t, &req(Action::Click, u.go, None)),
            Outcome::Done
        );
        let got: Vec<_> = std::iter::from_fn(|| u.t.pop_action()).collect();
        assert!(
            got.iter().any(|a| a.source == u.go),
            "the app is told the button was pressed: {got:?}"
        );
        assert_eq!(
            perform(&mut u.t, &req(Action::Click, u.check, None)),
            Outcome::Done
        );
        assert!(u.t.is_checked(u.check));
        // A disabled button, and a label, cannot be clicked.
        assert_eq!(
            perform(&mut u.t, &req(Action::Click, u.off, None)),
            Outcome::Ignored
        );
        assert_eq!(
            perform(&mut u.t, &req(Action::Click, u.label, None)),
            Outcome::Ignored
        );
        // Focus.
        assert_eq!(
            perform(&mut u.t, &req(Action::Focus, u.field, None)),
            Outcome::Done
        );
        assert_eq!(tree_update(&u.t, "w").focus, ak_id(u.field));
        // Increment and decrement move a slider by a step, held to 0 and 1000.
        assert_eq!(
            perform(&mut u.t, &req(Action::Increment, u.slider, None)),
            Outcome::Done
        );
        assert_eq!(u.t.slider_value(u.slider), Some(350));
        for _ in 0..30 {
            perform(&mut u.t, &req(Action::Increment, u.slider, None));
        }
        assert_eq!(u.t.slider_value(u.slider), Some(1000));
        for _ in 0..30 {
            perform(&mut u.t, &req(Action::Decrement, u.slider, None));
        }
        assert_eq!(u.t.slider_value(u.slider), Some(0));
        assert_eq!(
            perform(&mut u.t, &req(Action::Increment, u.go, None)),
            Outcome::Ignored,
            "a button has no value"
        );
        // Typing into a field focuses it and types.
        let typed = ActionData::Value("!".into());
        assert_eq!(
            perform(
                &mut u.t,
                &req(Action::ReplaceSelectedText, u.field, Some(typed))
            ),
            Outcome::Done
        );
        assert_eq!(u.t.text(u.field), Some("Ada!"));
        assert_eq!(
            perform(&mut u.t, &req(Action::ReplaceSelectedText, u.field, None)),
            Outcome::Ignored,
            "no text given"
        );
        // Something this toolkit does not offer.
        assert_eq!(
            perform(&mut u.t, &req(Action::ShowTooltip, u.go, None)),
            Outcome::Ignored
        );
    }

    #[test]
    fn a_request_about_a_widget_that_is_gone_or_never_was_is_refused_not_redirected() {
        let mut u = ui();
        let go = u.go;
        u.t.remove(go);
        assert_eq!(
            perform(&mut u.t, &req(Action::Click, go, None)),
            Outcome::Gone
        );
        // A new widget may take the freed slot: the old handle still finds nothing (the generation moved on).
        let again = u.t.button(None, "x", ButtonVariant::Primary);
        let _ = again;
        assert_eq!(
            perform(&mut u.t, &req(Action::Click, go, None)),
            Outcome::Gone
        );
        // The window, and numbers that are no handle at all.
        let window = ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: WINDOW,
            data: None,
        };
        assert_eq!(perform(&mut u.t, &window), Outcome::Ignored);
        let wild = ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: AkId(u64::MAX),
            data: None,
        };
        assert_eq!(perform(&mut u.t, &wild), Outcome::Gone);
        assert_eq!(widget_of(WINDOW), None);
        assert_eq!(widget_of(ak_id(u.check)), Some(u.check));
    }

    #[test]
    fn a_modal_dialog_and_a_toast_are_described_for_what_they_are() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new()).unwrap();
        let behind = t
            .button(Some(root), "Behind", ButtonVariant::Primary)
            .unwrap();
        let dialog = t.dialog(Some(root), Style::new().padding(8)).unwrap();
        let ok = t
            .button(Some(dialog), "OK", ButtonVariant::Primary)
            .unwrap();
        let toast = t.toast(Some(root)).unwrap();
        t.update(&screen(), &THEME, R::new(0, 0, 300, 300));
        // Closed: neither is in the tree.
        let up = tree_update(&t, "w");
        assert!(
            up.nodes
                .iter()
                .all(|(i, _)| *i != ak_id(dialog) && *i != ak_id(ok))
        );
        t.open_dialog(dialog);
        t.show_toast(toast, "Saved", 5000);
        t.advance(&THEME, 1000);
        t.update(&screen(), &THEME, R::new(0, 0, 300, 300));
        let up = tree_update(&t, "w");
        let d = find(&up, dialog);
        assert_eq!(d.role(), AkRole::Dialog);
        assert!(d.is_modal());
        assert_eq!(find(&up, toast).live(), Some(Live::Polite));
        // What is behind a modal cannot be clicked by a reader either.
        assert_eq!(
            perform(&mut t, &req(Action::Click, behind, None)),
            Outcome::Ignored
        );
        assert_eq!(
            perform(&mut t, &req(Action::Click, ok, None)),
            Outcome::Done
        );
    }
}
