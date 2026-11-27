//! The views that are one widget each: text, a button, a checkbox, a progress bar, a gap.

use core::fmt::{self, Write};

use fusuma_ui::{BuildError, ButtonVariant, Length, NodeId, SmallText, Style, TextRole};

use super::{Cx, Handler, View, ViewMarker};

/// A piece of text. Plain (`text`), or made with `.title()` or `.caption()`.
#[derive(Clone, Copy, Debug)]
pub struct Text<'a> {
    text: &'a str,
    role: TextRole,
}

/// Text in the body size.
#[must_use]
pub const fn text(text: &str) -> Text<'_> {
    Text {
        text,
        role: TextRole::Body,
    }
}

impl Text<'_> {
    /// As a heading.
    #[must_use]
    pub const fn title(mut self) -> Self {
        self.role = TextRole::Title;
        self
    }

    /// Small and muted.
    #[must_use]
    pub const fn caption(mut self) -> Self {
        self.role = TextRole::Caption;
        self
    }

    /// Fixed-width, for code.
    #[must_use]
    pub const fn code(mut self) -> Self {
        self.role = TextRole::Code;
        self
    }
}

impl<M: Clone> View<M> for Text<'_> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.label(parent, self.text, self.role)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.text);
    }
}

/// Text made by formatting, in a buffer on the stack (no allocation); longer than the widget keeps is cut.
#[derive(Clone, Copy, Debug)]
pub struct FmtText {
    buf: SmallText,
    role: TextRole,
}

/// Text from `format_args!`, in the body size.
#[must_use]
pub fn text_fmt(args: fmt::Arguments<'_>) -> FmtText {
    let mut buf = SmallText::EMPTY;
    let _ = buf.write_fmt(args);
    FmtText {
        buf,
        role: TextRole::Body,
    }
}

impl FmtText {
    /// As a heading.
    #[must_use]
    pub const fn title(mut self) -> Self {
        self.role = TextRole::Title;
        self
    }

    /// Small and muted.
    #[must_use]
    pub const fn caption(mut self) -> Self {
        self.role = TextRole::Caption;
        self
    }
}

impl<M: Clone> View<M> for FmtText {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.label(parent, self.buf.as_str(), self.role)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.buf.as_str());
    }
}

/// A button.
#[derive(Clone, Debug)]
pub struct Button<'a, M> {
    text: &'a str,
    variant: ButtonVariant,
    on_press: Option<M>,
    enabled: bool,
    icon: Option<char>,
}

/// A button with `text`, secondary until [`Button::primary`].
#[must_use]
pub const fn button<M>(text: &str) -> Button<'_, M> {
    Button {
        text,
        variant: ButtonVariant::Secondary,
        on_press: None,
        enabled: true,
        icon: None,
    }
}

impl<'a, M> Button<'a, M> {
    /// The main action's look.
    #[must_use]
    pub fn primary(mut self) -> Self {
        self.variant = ButtonVariant::Primary;
        self
    }

    /// Sends `message` when pressed.
    #[must_use]
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    /// Whether it can be used (it is by default).
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// An icon (a character of the icon font) before the text.
    #[must_use]
    pub const fn icon(mut self, icon: char) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl<M: Clone> Button<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.text);
        cx.tree.set_variant(node, self.variant);
        cx.tree.set_enabled(node, self.enabled);
        cx.tree.set_icon(node, self.icon);
        if let Some(m) = &self.on_press {
            cx.msgs.on(node, Handler::Press(m.clone()));
        }
    }
}

impl<M: Clone> View<M> for Button<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.button(parent, self.text, self.variant)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

/// A checkbox.
#[derive(Clone, Copy, Debug)]
pub struct Checkbox<'a, M> {
    text: &'a str,
    checked: bool,
    enabled: bool,
    on_toggle: Option<fn(bool) -> M>,
}

/// A checkbox with `text`, ticked or not.
#[must_use]
pub const fn checkbox<M>(text: &str, checked: bool) -> Checkbox<'_, M> {
    Checkbox {
        text,
        checked,
        enabled: true,
        on_toggle: None,
    }
}

impl<M> Checkbox<'_, M> {
    /// Whether it can be used (it can by default).
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sends a message made from the new state (`fn(bool) -> M`, such as a variant of the message enum) when it is
    /// toggled.
    #[must_use]
    pub const fn on_toggle(mut self, f: fn(bool) -> M) -> Self {
        self.on_toggle = Some(f);
        self
    }
}

impl<M: Clone> Checkbox<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.text);
        cx.tree.set_checked(node, self.checked);
        cx.tree.set_enabled(node, self.enabled);
        if let Some(f) = self.on_toggle {
            cx.msgs.on(node, Handler::Toggle(f));
        }
    }
}

impl<M: Clone> View<M> for Checkbox<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.checkbox(parent, self.text, self.checked)?;
        self.sync(cx, node);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

/// A progress bar, `permille` thousandths full, as wide as its parent gives it.
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    permille: u16,
    height: Option<u32>,
}

/// A progress bar.
#[must_use]
pub const fn progress(permille: u16) -> Progress {
    Progress {
        permille,
        height: None,
    }
}

impl Progress {
    /// `px` tall. A design that draws a box-shaped progress round its edge (cyberdeck) does so from here; others show a
    /// bar in the middle.
    #[must_use]
    pub const fn tall(self, px: u32) -> Self {
        Self {
            height: Some(px),
            ..self
        }
    }
}

impl<M: Clone> View<M> for Progress {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = cx.tree.progress_bar(parent, self.permille)?;
        let style = Style::new().width(Length::Fill(1));
        cx.tree.set_style(
            node,
            match self.height {
                Some(h) => style.height(Length::Fixed(h)),
                None => style,
            },
        );
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_progress(node, self.permille);
    }
}

/// Empty room that grows to fill what is left (a spring).
#[derive(Clone, Copy, Debug)]
pub struct Spacer;

/// A spring.
#[must_use]
pub const fn spacer() -> Spacer {
    Spacer
}

impl<M: Clone> View<M> for Spacer {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.spacer(
            parent,
            Style::new().width(Length::Fill(1)).height(Length::Fill(1)),
        )
    }

    fn rebuild<const N: usize, const K: usize>(&self, _: &mut Cx<'_, M, N, K>, _: NodeId) {}
}

impl ViewMarker for Text<'_> {}
impl ViewMarker for FmtText {}
impl<M> ViewMarker for Button<'_, M> {}
impl<M> ViewMarker for Checkbox<'_, M> {}
impl ViewMarker for Progress {}
impl ViewMarker for Spacer {}
impl ViewMarker for HostSurface {}

/// A message that shows for a while (see `Task::toast`), out of the flow. Give it a name with `named` so a task can find
/// it.
#[derive(Clone, Copy, Debug)]
pub struct Toast;

/// A toast, hidden until a task shows one.
#[must_use]
pub const fn toast() -> Toast {
    Toast
}

impl<M: Clone> View<M> for Toast {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.toast(parent)
    }

    fn rebuild<const N: usize, const K: usize>(&self, _: &mut Cx<'_, M, N, K>, _: NodeId) {}
}

impl ViewMarker for Toast {}

/// A place the host fills: a video, a camera, a map, a native view. It takes the room it is given and draws nothing; the
/// host finds where it is in `Shell::layers`.
#[derive(Clone, Copy, Debug)]
pub struct HostSurface {
    id: u32,
    style: Style,
}

/// A surface the host knows as `id`, filling its parent until `size` says otherwise.
#[must_use]
pub const fn host_surface(id: u32) -> HostSurface {
    HostSurface {
        id,
        style: Style::new().width(Length::Fill(1)).height(Length::Fill(1)),
    }
}

impl HostSurface {
    /// `width` by `height`.
    #[must_use]
    pub const fn size(self, width: Length, height: Length) -> Self {
        Self {
            style: Style::new().width(width).height(height),
            ..self
        }
    }
}

impl<M: Clone> View<M> for HostSurface {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.surface(parent, self.id, self.style)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_style(node, self.style);
    }
}
