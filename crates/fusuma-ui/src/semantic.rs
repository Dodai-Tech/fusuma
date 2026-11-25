//! The semantic tree: a read-only description of every widget that is on the screen, with no pixels in it. A role, a
//! name, a state, a rectangle, and where it sits in the tree. Anything that presents the tree some other way reads it:
//! the text UI draws cells from it, and an accessibility bridge (a screen reader) describes the same widgets from it.

use fusuma_render::Rect;

use crate::theme::TextRole;
use crate::tree::{Kind, NodeId, Tree};

/// What a widget is, for someone who cannot see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A container: a row, a column, or a modal panel's background.
    Group,
    /// A text.
    Label,
    /// A heading.
    Heading,
    /// An icon (its name is its character).
    Icon,
    /// A paragraph of wrapped text.
    Paragraph,
    /// A button.
    Button,
    /// A checkbox.
    Checkbox,
    /// A radio button.
    Radio,
    /// A switch.
    Switch,
    /// A slider.
    Slider,
    /// A progress bar.
    Progress,
    /// A row of a list.
    ListItem,
    /// A tab.
    Tab,
    /// A text field.
    TextField,
    /// A container that scrolls.
    ScrollArea,
    /// A modal dialog.
    Dialog,
    /// A menu.
    Menu,
    /// A message that shows for a while.
    Toast,
    /// Empty space.
    Spacer,
}

/// One widget, as it is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Semantic<'a> {
    /// The handle of the widget.
    pub id: NodeId,
    /// What it is.
    pub role: Role,
    /// Its text: a label's text, a button's name, a text field's content, a toast's message.
    pub name: &'a str,
    /// A text field's hint while it is empty (its placeholder); empty otherwise.
    pub hint: &'a str,
    /// An icon in front of the name (a button, list item or tab), as the icon font's character.
    pub icon: Option<char>,
    /// On or off, for a checkbox, a radio button, a switch.
    pub checked: Option<bool>,
    /// Chosen, for a list item or a tab.
    pub selected: Option<bool>,
    /// A slider's or a progress bar's value in thousandths; how far a scroll area is scrolled in pixels.
    pub value: Option<i32>,
    /// Where it is on the screen.
    pub rect: Rect,
    /// How many containers it is inside.
    pub depth: usize,
    /// Whether it cannot be used (itself or through an ancestor).
    pub disabled: bool,
    /// Whether the keyboard acts on it.
    pub focused: bool,
    /// Whether the pointer is over it.
    pub hovered: bool,
    /// Whether it is being pressed.
    pub pressed: bool,
    /// Whether it listens for a long press or a swipe (see [`Tree::set_long_press`]).
    pub gestures: bool,
}

impl<const N: usize> Tree<N> {
    /// Every widget on the screen, in tree order (a parent before its children, which is also reading order), with its
    /// depth. A dialog or menu that is closed is not there, nor is what is inside it, nor a toast that is not
    /// showing.
    pub fn semantics(&self) -> impl Iterator<Item = Semantic<'_>> + '_ {
        let root = self.root.map(usize::from);
        let mut cur = root;
        core::iter::from_fn(move || {
            loop {
                let i = cur?;
                let top = root?;
                if self.is_absent(i) {
                    // Not there: nothing under it either.
                    cur = self.after_subtree(i, top);
                    continue;
                }
                cur = self.preorder_next(i, top);
                return Some(self.semantic_of(i));
            }
        })
    }

    /// The description of widget `id`, if it is on the screen.
    #[must_use]
    pub fn semantic(&self, id: NodeId) -> Option<Semantic<'_>> {
        let i = self.slot(id)?;
        // Not there if it, or what it is inside, is closed.
        let mut cur = Some(i);
        while let Some(c) = cur {
            if self.is_absent(c) {
                return None;
            }
            cur = self.nodes[c].parent.map(usize::from);
        }
        Some(self.semantic_of(i))
    }

    /// Whether node `i` is not on the screen by itself: a closed dialog, a toast that is not showing.
    fn is_absent(&self, i: usize) -> bool {
        if self.nodes[i].hidden {
            return true;
        }
        match self.nodes[i].kind {
            Kind::Dialog { open, .. } => !open && !self.dialog_visible(i),
            Kind::Toast { phase, .. } => phase == crate::toast::Phase::Hidden,
            _ => false,
        }
    }

    fn semantic_of(&self, i: usize) -> Semantic<'_> {
        let n = &self.nodes[i];
        let id = self.id_of(i);
        let mut depth = 0;
        let mut up = n.parent.map(usize::from);
        while let Some(p) = up {
            depth += 1;
            up = self.nodes[p].parent.map(usize::from);
        }
        let (role, name, hint, icon, checked, selected, value) = match &n.kind {
            Kind::Stack(_) => (Role::Group, "", "", None, None, None, None),
            Kind::Spacer => (Role::Spacer, "", "", None, None, None, None),
            Kind::Label { text, role } => {
                let r = match role {
                    TextRole::Title => Role::Heading,
                    TextRole::Icon => Role::Icon,
                    _ => Role::Label,
                };
                (r, text.as_str(), "", None, None, None, None)
            }
            Kind::Paragraph { text, .. } => {
                (Role::Paragraph, text.as_str(), "", None, None, None, None)
            }
            Kind::Button { text, icon, .. } => {
                (Role::Button, text.as_str(), "", *icon, None, None, None)
            }
            Kind::Checkbox { text, checked } => (
                Role::Checkbox,
                text.as_str(),
                "",
                None,
                Some(*checked),
                None,
                None,
            ),
            Kind::Radio { text, checked } => (
                Role::Radio,
                text.as_str(),
                "",
                None,
                Some(*checked),
                None,
                None,
            ),
            Kind::Switch { text, on } => {
                (Role::Switch, text.as_str(), "", None, Some(*on), None, None)
            }
            Kind::ListItem {
                text,
                selected,
                icon,
            } => (
                Role::ListItem,
                text.as_str(),
                "",
                *icon,
                None,
                Some(*selected),
                None,
            ),
            Kind::Tab {
                text,
                selected,
                icon,
            } => (
                Role::Tab,
                text.as_str(),
                "",
                *icon,
                None,
                Some(*selected),
                None,
            ),
            Kind::Progress { permille } => (
                Role::Progress,
                "",
                "",
                None,
                None,
                None,
                Some(i32::from(*permille)),
            ),
            Kind::Slider { permille } => (
                Role::Slider,
                "",
                "",
                None,
                None,
                None,
                Some(i32::from(*permille)),
            ),
            Kind::TextInput {
                text, placeholder, ..
            } => (
                Role::TextField,
                text.as_str(),
                *placeholder,
                None,
                None,
                None,
                None,
            ),
            Kind::Dialog { popover, .. } => (
                if popover.is_some() {
                    Role::Menu
                } else {
                    Role::Dialog
                },
                "",
                "",
                None,
                None,
                None,
                None,
            ),
            Kind::Toast { text, .. } => (Role::Toast, text.as_str(), "", None, None, None, None),
            Kind::Scroll { offset, .. } => {
                (Role::ScrollArea, "", "", None, None, None, Some(*offset))
            }
        };
        Semantic {
            id,
            role,
            name,
            hint,
            icon,
            checked,
            selected,
            value,
            rect: n.rect,
            depth,
            disabled: n.disabled_eff,
            focused: self.focus == Some(id),
            hovered: n.hovered,
            pressed: n.active,
            gestures: n.gestures != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::Rect;
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{CrossAlign, Length, Style};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(8, 8).unwrap()
    }

    struct Ui {
        t: Tree<32>,
        check: NodeId,
        field: NodeId,
        dialog: NodeId,
        toast: NodeId,
    }

    fn ui() -> Ui {
        let mut t = Tree::<32>::new();
        let root = t
            .column(
                None,
                Style::new().gap(4).padding(4).align(CrossAlign::Stretch),
            )
            .unwrap();
        t.label(Some(root), "Settings", TextRole::Title).unwrap();
        let row = t.row(Some(root), Style::new().gap(4)).unwrap();
        t.icon(Some(row), 'i').unwrap();
        t.button_with_icon(Some(row), 'b', "Save", ButtonVariant::Primary)
            .unwrap();
        let check = t.checkbox(Some(root), "Notify", true).unwrap();
        t.radio(Some(root), "Small", false).unwrap();
        t.switch(Some(root), "Wi-Fi", false).unwrap();
        t.slider(Some(root), 250).unwrap();
        t.progress_bar(Some(root), 600).unwrap();
        let field = t.text_input(Some(root), "").unwrap();
        t.set_placeholder(field, "Name");
        let bar = t.tab_bar(Some(root), Style::new()).unwrap();
        t.tab(Some(bar), None, "One", true).unwrap();
        t.tab(Some(bar), Some('t'), "Two", false).unwrap();
        let list = t
            .scroll(
                Some(root),
                Style::new()
                    .height(Length::Fixed(60))
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        t.list_item(Some(list), "Row", true).unwrap();
        t.paragraph(Some(root), "A paragraph", TextRole::Body)
            .unwrap();
        let dialog = t.dialog(Some(root), Style::new().padding(8)).unwrap();
        t.label(Some(dialog), "Sure?", TextRole::Body).unwrap();
        let toast = t.toast(Some(root)).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 400));
        Ui {
            t,
            check,
            field,
            dialog,
            toast,
        }
    }

    fn roles(t: &Tree<32>) -> Vec<(usize, Role)> {
        t.semantics().map(|s| (s.depth, s.role)).collect()
    }

    #[test]
    fn every_widget_is_described_in_reading_order_with_its_depth() {
        let u = ui();
        use Role::*;
        assert_eq!(
            roles(&u.t),
            [
                (0, Group),
                (1, Heading),
                (1, Group),
                (2, Icon),
                (2, Button),
                (1, Checkbox),
                (1, Radio),
                (1, Switch),
                (1, Slider),
                (1, Progress),
                (1, TextField),
                (1, Group),
                (2, Tab),
                (2, Tab),
                (1, ScrollArea),
                (2, ListItem),
                (1, Paragraph),
            ],
            "the closed dialog and the hidden toast are not there"
        );
        // 17 shown of 17 + the dialog (and its label) + the toast.
        assert_eq!(u.t.semantics().count(), u.t.len() - 3);
    }

    #[test]
    fn names_states_and_values_are_what_the_widgets_hold() {
        let u = ui();
        let by = |role: Role| {
            u.t.semantics()
                .filter(move |s| s.role == role)
                .collect::<Vec<_>>()
        };
        assert_eq!(by(Role::Heading)[0].name, "Settings");
        let b = by(Role::Button)[0];
        assert_eq!((b.name, b.icon), ("Save", Some('b')));
        assert_eq!(by(Role::Icon)[0].name, "i");
        assert_eq!(by(Role::Checkbox)[0].checked, Some(true));
        assert_eq!(by(Role::Radio)[0].checked, Some(false));
        assert_eq!(by(Role::Switch)[0].checked, Some(false));
        assert_eq!(by(Role::Slider)[0].value, Some(250));
        assert_eq!(by(Role::Progress)[0].value, Some(600));
        let f = by(Role::TextField)[0];
        assert_eq!((f.name, f.hint), ("", "Name"));
        let tabs = by(Role::Tab);
        assert_eq!(
            (tabs[0].selected, tabs[1].selected, tabs[1].icon),
            (Some(true), Some(false), Some('t'))
        );
        assert_eq!(by(Role::ListItem)[0].selected, Some(true));
        assert_eq!(by(Role::ScrollArea)[0].value, Some(0));
        assert_eq!(by(Role::Paragraph)[0].name, "A paragraph");
        let r = by(Role::Checkbox)[0].rect;
        assert!(r.w > 0 && r.h > 0, "a rectangle on the screen: {r:?}");
    }

    #[test]
    fn it_follows_the_state_focus_disabled_and_what_is_open() {
        let mut u = ui();
        u.t.set_checked(u.check, false);
        u.t.focus(u.field);
        u.t.set_enabled(u.check, false);
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 400));
        let s = u.t.semantic(u.check).unwrap();
        assert_eq!(
            (s.checked, s.disabled, s.focused),
            (Some(false), true, false)
        );
        let f = u.t.semantic(u.field).unwrap();
        assert!(f.focused && !f.disabled);
        u.t.type_text("Ana");
        assert_eq!(u.t.semantic(u.field).unwrap().name, "Ana");
        // A dialog appears with its content; a toast with its message.
        assert!(u.t.semantic(u.dialog).is_none());
        u.t.open_dialog(u.dialog);
        u.t.show_toast(u.toast, "Saved", 1000);
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 400));
        let all: Vec<_> = u.t.semantics().collect();
        let d = all.iter().find(|s| s.role == Role::Dialog).unwrap();
        assert_eq!(d.id, u.dialog);
        assert!(
            all.iter()
                .any(|s| s.role == Role::Label && s.name == "Sure?" && s.depth == 2)
        );
        assert_eq!(
            all.iter().find(|s| s.role == Role::Toast).unwrap().name,
            "Saved"
        );
        // Closed again (and its animation done): gone.
        u.t.close_dialog(u.dialog);
        u.t.advance(&THEME, 5000);
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 400));
        assert!(u.t.semantic(u.dialog).is_none());
        assert!(u.t.semantics().all(|s| s.name != "Sure?"));
    }

    #[test]
    fn a_menu_is_a_menu_pressed_and_gesture_flags_show_and_odd_handles_give_nothing() {
        let mut u = ui();
        let root = u.t.root().unwrap();
        let menu = u.t.menu(Some(root), Style::new()).unwrap();
        u.t.list_item(Some(menu), "Copy", false).unwrap();
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 400));
        assert!(u.t.semantic(menu).is_none());
        u.t.open_menu(menu, u.check);
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 400));
        assert_eq!(u.t.semantic(menu).unwrap().role, Role::Menu);
        u.t.set_swipe(u.check, crate::Swipe::Horizontal);
        assert!(u.t.semantic(u.check).unwrap().gestures);
        let gone = u.check;
        u.t.remove(gone);
        assert!(u.t.semantic(gone).is_none());
        assert_eq!(Tree::<4>::new().semantics().count(), 0);
    }
}
