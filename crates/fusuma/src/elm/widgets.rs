//! Views for the rest of the widgets: radio buttons, switches, sliders, text fields, list rows, tabs, wrapped text,
//! scrolling lists, dialogs, menus, and the gestures a view can listen for.

use fusuma_ui::{
    BuildError, ButtonVariant, CrossAlign, Length, NodeId, Style, Swipe, SwipeDir, TextRole,
};

use super::{Children, Cx, Handler, View, ViewMarker};

/// A radio button: ticked or not; a group is the radio buttons that share a parent, one ticked at most.
#[derive(Clone, Copy, Debug)]
pub struct Radio<'a, M> {
    text: &'a str,
    checked: bool,
    on_select: Option<M>,
}

/// A radio button.
#[must_use]
pub const fn radio<M>(text: &str, checked: bool) -> Radio<'_, M> {
    Radio {
        text,
        checked,
        on_select: None,
    }
}

impl<M> Radio<'_, M> {
    /// Sends `message` when it is chosen.
    #[must_use]
    pub fn on_select(mut self, message: M) -> Self {
        self.on_select = Some(message);
        self
    }
}

impl<M: Clone> Radio<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.text);
        cx.tree.set_checked(node, self.checked);
        if let Some(m) = &self.on_select {
            cx.msgs.on(node, Handler::Press(m.clone()));
        }
    }
}

impl<M: Clone> View<M> for Radio<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.radio(parent, self.text, self.checked)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Radio<'_, M> {}

/// A switch.
#[derive(Clone, Copy, Debug)]
pub struct Switch<'a, M> {
    text: &'a str,
    on: bool,
    on_toggle: Option<fn(bool) -> M>,
}

/// A switch with `text`, on or off.
#[must_use]
pub const fn switch<M>(text: &str, on: bool) -> Switch<'_, M> {
    Switch {
        text,
        on,
        on_toggle: None,
    }
}

impl<M> Switch<'_, M> {
    /// Sends a message made from the new state when it is toggled.
    #[must_use]
    pub const fn on_toggle(mut self, f: fn(bool) -> M) -> Self {
        self.on_toggle = Some(f);
        self
    }
}

impl<M: Clone> Switch<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.text);
        cx.tree.set_checked(node, self.on);
        if let Some(f) = self.on_toggle {
            cx.msgs.on(node, Handler::Toggle(f));
        }
    }
}

impl<M: Clone> View<M> for Switch<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.switch(parent, self.text, self.on)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Switch<'_, M> {}

/// A slider, `permille` thousandths along.
#[derive(Clone, Copy, Debug)]
pub struct Slider<M> {
    permille: u16,
    on_change: Option<fn(u16) -> M>,
}

/// A slider.
#[must_use]
pub const fn slider<M>(permille: u16) -> Slider<M> {
    Slider {
        permille,
        on_change: None,
    }
}

impl<M> Slider<M> {
    /// Sends a message made from the new position (thousandths) when it moves.
    #[must_use]
    pub const fn on_change(mut self, f: fn(u16) -> M) -> Self {
        self.on_change = Some(f);
        self
    }
}

impl<M: Clone> Slider<M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        // A slider that is being dragged is not moved under the finger by a view that lags a frame behind.
        if !cx.tree.is_active(node) {
            cx.tree.set_slider(node, self.permille);
        }
        if let Some(f) = self.on_change {
            cx.msgs.on(node, Handler::Value(f));
        }
    }
}

impl<M: Clone> View<M> for Slider<M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.slider(parent, self.permille)?;
        cx.tree.set_style(node, Style::new().width(Length::Fill(1)));
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Slider<M> {}

/// A text field (one line, or a text area with [`TextField::rows`]). The view gives the value; the tree keeps the caret, the selection and the undo; every change
/// the user makes is a message with the new text, and the program's state becomes the value again.
#[derive(Clone, Copy, Debug)]
pub struct TextField<'a, M> {
    value: &'a str,
    placeholder: &'static str,
    on_input: Option<fn(&str) -> M>,
    on_submit: Option<M>,
    rows: u8,
}

/// A text field holding `value`.
#[must_use]
pub const fn text_field<M>(value: &str) -> TextField<'_, M> {
    TextField {
        value,
        placeholder: "",
        on_input: None,
        on_submit: None,
        rows: 0,
    }
}

impl<M> TextField<'_, M> {
    /// Shown, muted, while it is empty.
    #[must_use]
    pub const fn placeholder(mut self, text: &'static str) -> Self {
        self.placeholder = text;
        self
    }

    /// Makes it a text area of `rows` lines (at least 2): Enter breaks the line instead of submitting, Up and Down move between
    /// lines. The text is kept like a field's, at most [`fusuma_ui::INPUT_CAP`] bytes, breaks included.
    #[must_use]
    pub const fn rows(mut self, rows: u8) -> Self {
        self.rows = rows;
        self
    }

    /// Sends `message` when Enter is pressed in the field.
    #[must_use]
    pub fn on_submit(mut self, message: M) -> Self {
        self.on_submit = Some(message);
        self
    }

    /// Sends a message made from the new text whenever it changes.
    #[must_use]
    pub const fn on_input(mut self, f: fn(&str) -> M) -> Self {
        self.on_input = Some(f);
        self
    }
}

impl<M: Clone> TextField<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        // Only when it differs: the user's own typing comes back as the value, and setting it again would move the caret.
        if cx.tree.text(node) != Some(self.value) {
            cx.tree.set_text(node, self.value);
        }
        cx.tree.set_placeholder(node, self.placeholder);
        if let Some(f) = self.on_input {
            cx.msgs.on(node, Handler::Text(f));
        }
        if let Some(m) = &self.on_submit {
            cx.msgs.on(node, Handler::Press(m.clone()));
        }
    }
}

impl<M: Clone> View<M> for TextField<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = if self.rows > 0 {
            cx.tree.text_area(parent, self.value, self.rows)?
        } else {
            cx.tree.text_input(parent, self.value)?
        };
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for TextField<'_, M> {}

/// A row of a list or a menu.
#[derive(Clone, Copy, Debug)]
pub struct Item<'a, M> {
    text: &'a str,
    selected: bool,
    icon: Option<fusuma_icons::Icon>,
    on_select: Option<M>,
}

/// A list row.
#[must_use]
pub const fn item<M>(text: &str, selected: bool) -> Item<'_, M> {
    Item {
        text,
        selected,
        icon: None,
        on_select: None,
    }
}

impl<M> Item<'_, M> {
    /// Sends `message` when it is chosen (or pressed, in a menu).
    #[must_use]
    pub fn on_select(mut self, message: M) -> Self {
        self.on_select = Some(message);
        self
    }

    /// An icon before the text.
    #[must_use]
    pub const fn icon(mut self, icon: fusuma_icons::Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl<M: Clone> Item<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.text);
        cx.tree.set_icon(node, self.icon);
        cx.tree.set_checked(node, self.selected);
        if let Some(m) = &self.on_select {
            cx.msgs.on(node, Handler::Press(m.clone()));
        }
    }
}

impl<M: Clone> View<M> for Item<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.list_item(parent, self.text, self.selected)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Item<'_, M> {}

/// A tab.
#[derive(Clone, Copy, Debug)]
pub struct Tab<'a, M> {
    text: &'a str,
    selected: bool,
    icon: Option<fusuma_icons::Icon>,
    on_select: Option<M>,
}

/// A tab of a tab bar.
#[must_use]
pub const fn tab<M>(text: &str, selected: bool) -> Tab<'_, M> {
    Tab {
        text,
        selected,
        icon: None,
        on_select: None,
    }
}

impl<M> Tab<'_, M> {
    /// Sends `message` when it is chosen.
    #[must_use]
    pub fn on_select(mut self, message: M) -> Self {
        self.on_select = Some(message);
        self
    }

    /// An icon before the text.
    #[must_use]
    pub const fn icon(mut self, icon: fusuma_icons::Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl<M: Clone> Tab<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.text);
        cx.tree.set_icon(node, self.icon);
        cx.tree.set_checked(node, self.selected);
        if let Some(m) = &self.on_select {
            cx.msgs.on(node, Handler::Press(m.clone()));
        }
    }
}

impl<M: Clone> View<M> for Tab<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.tab(parent, self.icon, self.text, self.selected)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Tab<'_, M> {}

/// Wrapped text that lives for ever (a constant), as wide as its parent gives it.
#[derive(Clone, Copy, Debug)]
pub struct Paragraph {
    text: &'static str,
}

/// A paragraph.
#[must_use]
pub const fn paragraph(text: &'static str) -> Paragraph {
    Paragraph { text }
}

impl<M: Clone> View<M> for Paragraph {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.paragraph_static(parent, self.text, TextRole::Body)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_paragraph_static(node, self.text);
    }
}

impl ViewMarker for Paragraph {}

/// An icon as a view of its own.
#[derive(Clone, Copy, Debug)]
pub struct IconView(fusuma_icons::Icon);

/// An icon.
#[must_use]
pub const fn icon(icon: fusuma_icons::Icon) -> IconView {
    IconView(icon)
}

impl<M: Clone> View<M> for IconView {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.icon(parent, self.0)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        let mut buf = [0u8; 4];
        cx.tree.set_text(node, self.0.glyph().encode_utf8(&mut buf));
    }
}

impl ViewMarker for IconView {}

/// What a container made by [`scroll`] or [`tab_bar`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Scroll,
    TabBar,
    Dialog,
    Sheet,
    Menu,
}

/// A container with a window onto taller content, a tab bar, a dialog or a menu.
#[derive(Clone, Copy, Debug)]
pub struct Special<C, M> {
    kind: Kind,
    style: Style,
    children: C,
    open: bool,
    anchor: Option<fusuma_ui::Id>,
    on_dismiss: Option<M>,
}

/// A column that scrolls when its content is taller than it is (give it a height).
#[must_use]
pub const fn scroll<C, M>(children: C) -> Special<C, M> {
    Special {
        kind: Kind::Scroll,
        style: Style::DEFAULT,
        children,
        open: true,
        anchor: None,
        on_dismiss: None,
    }
}

/// A row that gives each of its tabs an equal share.
#[must_use]
pub const fn tab_bar<C, M>(children: C) -> Special<C, M> {
    Special {
        kind: Kind::TabBar,
        style: Style::DEFAULT,
        children,
        open: true,
        anchor: None,
        on_dismiss: None,
    }
}

/// A modal dialog, open or not.
#[must_use]
pub const fn dialog<C, M>(open: bool, children: C) -> Special<C, M> {
    Special {
        kind: Kind::Dialog,
        style: Style::DEFAULT,
        children,
        open,
        anchor: None,
        on_dismiss: None,
    }
}

/// A bottom sheet, open or not: a modal panel on the bottom edge, as wide as the screen and as tall as its content.
#[must_use]
pub const fn bottom_sheet<C, M>(open: bool, children: C) -> Special<C, M> {
    Special {
        kind: Kind::Sheet,
        style: Style::DEFAULT,
        children,
        open,
        anchor: None,
        on_dismiss: None,
    }
}

/// A menu that hangs from the node called `anchor` while it is open.
#[must_use]
pub const fn menu<C, M>(open: bool, anchor: fusuma_ui::Id, children: C) -> Special<C, M> {
    Special {
        kind: Kind::Menu,
        style: Style::DEFAULT,
        children,
        open,
        anchor: Some(anchor),
        on_dismiss: None,
    }
}

impl<C, M> Special<C, M> {
    /// Room around the children.
    #[must_use]
    pub fn padding(mut self, px: u32) -> Self {
        self.style = self.style.padding(px);
        self
    }

    /// Room between the children.
    #[must_use]
    pub fn gap(mut self, px: u32) -> Self {
        self.style = self.style.gap(px);
        self
    }

    /// The width.
    #[must_use]
    pub fn width(mut self, length: Length) -> Self {
        self.style = self.style.width(length);
        self
    }

    /// The height.
    #[must_use]
    pub fn height(mut self, length: Length) -> Self {
        self.style = self.style.height(length);
        self
    }

    /// How the children line up across the axis.
    #[must_use]
    pub fn align(mut self, align: CrossAlign) -> Self {
        self.style = self.style.align(align);
        self
    }

    /// Sends `message` when a dialog or menu is dismissed (a press outside, Escape).
    #[must_use]
    pub fn on_dismiss(mut self, message: M) -> Self {
        self.on_dismiss = Some(message);
        self
    }
}

impl<M: Clone, C: Children<M>> Special<C, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_style(node, self.style);
        match self.kind {
            Kind::Dialog | Kind::Sheet => {
                if self.open != cx.tree.is_dialog_open(node) {
                    if self.open {
                        cx.tree.open_dialog(node);
                    } else {
                        cx.tree.close_dialog(node);
                    }
                }
            }
            Kind::Menu => {
                if self.open != cx.tree.is_menu_open(node) {
                    if self.open {
                        if let Some(anchor) = self.anchor.and_then(|id| cx.tree.find(id)) {
                            cx.tree.open_menu(node, anchor);
                        }
                    } else {
                        cx.tree.close_menu(node);
                    }
                }
            }
            Kind::Scroll | Kind::TabBar => {}
        }
        if let Some(m) = &self.on_dismiss {
            cx.msgs.on(node, Handler::Dismiss(m.clone()));
        }
    }
}

impl<M: Clone, C: Children<M>> View<M> for Special<C, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = match self.kind {
            Kind::Scroll => cx.tree.scroll(parent, self.style)?,
            Kind::TabBar => cx.tree.tab_bar(parent, self.style)?,
            Kind::Dialog => cx.tree.dialog(parent, self.style)?,
            Kind::Sheet => cx.tree.bottom_sheet(parent, self.style)?,
            Kind::Menu => cx.tree.menu(parent, self.style)?,
        };
        self.children.build_all(cx, node)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.children.rebuild_all(cx, node);
        self.sync(cx, node);
    }
}

impl<C, M> ViewMarker for Special<C, M> {}

/// The axes a swipe is listened for on, and what it sends.
type SwipeHandler<M> = (Swipe, fn(SwipeDir) -> M);

/// A view that listens for a long press and a swipe on its node.
#[derive(Clone, Copy, Debug)]
pub struct Gestures<V, M> {
    view: V,
    long_press: Option<M>,
    swipe: Option<SwipeHandler<M>>,
}

/// `view`, listening for gestures: add `.on_long_press(..)` and `.on_swipe(..)`.
#[must_use]
pub const fn gestures<V, M>(view: V) -> Gestures<V, M> {
    Gestures {
        view,
        long_press: None,
        swipe: None,
    }
}

impl<V, M> Gestures<V, M> {
    /// Sends `message` when the node is pressed and held.
    #[must_use]
    pub fn on_long_press(mut self, message: M) -> Self {
        self.long_press = Some(message);
        self
    }

    /// Sends a message made from the direction when a press travels far enough along `axes`.
    #[must_use]
    pub const fn on_swipe(mut self, axes: Swipe, f: fn(SwipeDir) -> M) -> Self {
        self.swipe = Some((axes, f));
        self
    }
}

impl<M: Clone, V: View<M>> Gestures<V, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_long_press(node, self.long_press.is_some());
        cx.tree
            .set_swipe(node, self.swipe.map_or(Swipe::Off, |(axes, _)| axes));
        if let Some(m) = &self.long_press {
            cx.msgs.on(node, Handler::LongPress(m.clone()));
        }
        if let Some((_, f)) = self.swipe {
            cx.msgs.on(node, Handler::Swipe(f));
        }
    }
}

impl<M: Clone, V: View<M>> View<M> for Gestures<V, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = self.view.build(cx, parent)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.view.rebuild(cx, node);
        self.sync(cx, node);
    }
}

impl<V, M> ViewMarker for Gestures<V, M> {}

/// A button that is primary or not, by a condition (a small convenience for a toggle that looks different when on).
#[must_use]
pub const fn variant(primary: bool) -> ButtonVariant {
    if primary {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Secondary
    }
}

/// A view that can be picked up and dragged, or that takes what is dropped on it (see [`draggable`] and [`drop_target`]).
#[derive(Clone, Copy, Debug)]
pub struct Dnd<V, M> {
    view: V,
    on_start: Option<M>,
    on_cancel: Option<M>,
    on_drop: Option<M>,
}

/// `view`, which can be picked up when a press on it travels: `on_start` is what picking it up sends (the app remembers
/// what is being carried), and [`Dnd::on_cancel`] what ending the drag over nothing sends. A press that does not travel is
/// still a click.
#[must_use]
pub const fn draggable<V, M>(view: V, on_start: M) -> Dnd<V, M> {
    Dnd {
        view,
        on_start: Some(on_start),
        on_cancel: None,
        on_drop: None,
    }
}

/// `view`, which takes what is dropped on it: `on_drop` is what a drop sends (the app moves what it was carrying, which it
/// remembered when the drag started). It is outlined while a drag is over it.
#[must_use]
pub const fn drop_target<V, M>(view: V, on_drop: M) -> Dnd<V, M> {
    Dnd {
        view,
        on_start: None,
        on_cancel: None,
        on_drop: Some(on_drop),
    }
}

impl<V, M> Dnd<V, M> {
    /// Sends `message` when a drag from this view ends without a drop.
    #[must_use]
    pub fn on_cancel(mut self, message: M) -> Self {
        self.on_cancel = Some(message);
        self
    }

    /// Also takes drops, sending `message` (a view that can be carried and also be dropped on).
    #[must_use]
    pub fn on_drop(mut self, message: M) -> Self {
        self.on_drop = Some(message);
        self
    }
}

impl<M: Clone, V: View<M>> Dnd<V, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_draggable(node, self.on_start.is_some());
        cx.tree.set_drop_target(node, self.on_drop.is_some());
        if let Some(m) = &self.on_start {
            cx.msgs.on(node, Handler::DragStart(m.clone()));
        }
        if let Some(m) = &self.on_cancel {
            cx.msgs.on(node, Handler::DragCancel(m.clone()));
        }
        if let Some(m) = &self.on_drop {
            cx.msgs.on(node, Handler::Drop(m.clone()));
        }
    }
}

impl<M: Clone, V: View<M>> View<M> for Dnd<V, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = self.view.build(cx, parent)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.view.rebuild(cx, node);
        self.sync(cx, node);
    }
}

impl<V, M> ViewMarker for Dnd<V, M> {}
