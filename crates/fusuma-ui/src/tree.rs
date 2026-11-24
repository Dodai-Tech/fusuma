//! The retained widget tree: a fixed-size arena whose nodes keep their own state between frames.

use fusuma_anim::Fx;
use fusuma_render::{Point, Rect, Size};

use crate::theme::{ButtonVariant, Round, Surface, TextRole};

use crate::textinput::InputText;

/// How many bytes of text a label or a button keeps.
pub const TEXT_CAP: usize = 64;

/// A short piece of text stored inline, so a widget owns its text and needs no allocator. Longer
/// text is cut at a character boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmallText {
    len: u8,
    buf: [u8; TEXT_CAP],
}

impl SmallText {
    /// No text.
    pub const EMPTY: Self = Self {
        len: 0,
        buf: [0; TEXT_CAP],
    };

    /// Text from `s`, cut to [`TEXT_CAP`] bytes.
    #[must_use]
    pub fn new(s: &str) -> Self {
        let mut t = Self::EMPTY;
        t.set(s);
        t
    }

    /// Replaces the text, cutting it at a character boundary if it is too long.
    pub fn set(&mut self, s: &str) {
        let mut end = s.len().min(TEXT_CAP);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.buf[..end].copy_from_slice(&s.as_bytes()[..end]);
        self.len = end as u8;
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..usize::from(self.len)]).unwrap_or("")
    }
}

/// A handle to a node. It goes stale when the node is removed: its slot may be reused, but the old
/// handle then finds nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub(crate) index: u16,
    pub(crate) generation: u8,
}

/// The direction a container lays its children out in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Axis {
    /// Top to bottom.
    #[default]
    Column,
    /// Left to right.
    Row,
}

/// How much room a node takes along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Length {
    /// What its content needs.
    #[default]
    Content,
    /// Exactly this many pixels.
    Fixed(u32),
    /// A share of the room that is left, in proportion to the weight (0 counts as 1).
    Fill(u16),
}

/// Where a container puts a child across its own direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrossAlign {
    /// At the start.
    #[default]
    Start,
    /// In the middle.
    Center,
    /// At the end.
    End,
    /// Stretched to the full width (or height); a fixed size stays as it is.
    Stretch,
}

/// How a node is sized and drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    /// Width.
    pub width: Length,
    /// Height.
    pub height: Length,
    /// Space between the edge and the content, on all sides.
    pub padding: u32,
    /// Space between children.
    pub gap: u32,
    /// Where children sit across the container's direction.
    pub align: CrossAlign,
    /// The container's background.
    pub background: Surface,
    /// The container's corner rounding.
    pub round: Round,
}

impl Style {
    /// Content-sized, no padding, transparent.
    pub const DEFAULT: Self = Self {
        width: Length::Content,
        height: Length::Content,
        padding: 0,
        gap: 0,
        align: CrossAlign::Start,
        background: Surface::None,
        round: Round::None,
    };

    /// The default style.
    #[must_use]
    pub const fn new() -> Self {
        Self::DEFAULT
    }

    /// With a width.
    #[must_use]
    pub const fn width(self, width: Length) -> Self {
        Self { width, ..self }
    }

    /// With a height.
    #[must_use]
    pub const fn height(self, height: Length) -> Self {
        Self { height, ..self }
    }

    /// With padding.
    #[must_use]
    pub const fn padding(self, padding: u32) -> Self {
        Self { padding, ..self }
    }

    /// With a gap between children.
    #[must_use]
    pub const fn gap(self, gap: u32) -> Self {
        Self { gap, ..self }
    }

    /// With a cross-axis alignment.
    #[must_use]
    pub const fn align(self, align: CrossAlign) -> Self {
        Self { align, ..self }
    }

    /// With a background.
    #[must_use]
    pub const fn background(self, background: Surface) -> Self {
        Self { background, ..self }
    }

    /// With rounded corners.
    #[must_use]
    pub const fn round(self, round: Round) -> Self {
        Self { round, ..self }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why a node could not be added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    /// The tree has no free slot.
    Full,
    /// The parent handle is stale or was never valid.
    NoSuchParent,
    /// The parent is a label, a button or a spacer: only rows and columns have children.
    NotContainer,
    /// The tree already has a root.
    SecondRoot,
}

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Stack(Axis),
    Label {
        text: SmallText,
        role: TextRole,
    },
    Button {
        text: SmallText,
        variant: ButtonVariant,
        /// 0 at rest, 1 fully hovered or pressed; moves over time in the animation pass.
        blend: Fx,
    },
    Spacer,
    Checkbox {
        text: SmallText,
        checked: bool,
    },
    Radio {
        text: SmallText,
        checked: bool,
    },
    ListItem {
        text: SmallText,
        selected: bool,
    },
    /// Progress in thousandths, 0 to 1000.
    Progress {
        permille: u16,
    },
    Switch {
        text: SmallText,
        on: bool,
    },
    /// A value from 0 to 1000 thousandths, dragged with the pointer or moved with the keys.
    Slider {
        permille: u16,
    },
    /// A single-line text field: `text` is at most [`crate::INPUT_CAP`] bytes; the cursor and the other end of the
    /// selection are byte offsets on character boundaries.
    TextInput {
        text: InputText,
        cursor: u8,
        anchor: u8,
    },
    /// A modal panel centred over the screen and drawn on top of everything: a column of its own, out of the flow of
    /// its parent. Closed until [`Tree::open_dialog`].
    Dialog {
        open: bool,
        /// What had the focus when it opened, to give it back on close.
        prev: Option<NodeId>,
    },
    /// A column that shows a window onto content taller than itself: `offset` pixels scrolled, of
    /// `content` pixels of children.
    Scroll {
        offset: i32,
        content: u32,
        /// How far the content is pulled past an end (negative above the top), by a drag or a bounce. 0 at rest.
        over: i32,
    },
}

/// A press that may turn into a scroll: where it began and how far it has been dragged.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Drag {
    pub scroll: NodeId,
    pub start_y: i32,
    pub offset0: i32,
    pub moving: bool,
    /// Where the finger would have the content, unclamped, in pixels.
    pub raw: i32,
    /// `raw` at the last frame, and the speed of the content in pixels a second, to start a fling with.
    pub last_raw: i32,
    pub vel: i32,
}

impl Kind {
    /// Whether the widget reacts to the pointer and the keyboard (it can be pressed and focused).
    pub(crate) const fn pressable(&self) -> bool {
        matches!(
            self,
            Self::Button { .. }
                | Self::Checkbox { .. }
                | Self::Radio { .. }
                | Self::ListItem { .. }
                | Self::Switch { .. }
                | Self::Slider { .. }
                | Self::TextInput { .. }
        )
    }

    pub(crate) const fn text(&self) -> Option<&SmallText> {
        match self {
            Self::Label { text, .. }
            | Self::Button { text, .. }
            | Self::Checkbox { text, .. }
            | Self::Radio { text, .. }
            | Self::Switch { text, .. }
            | Self::ListItem { text, .. } => Some(text),
            _ => None,
        }
    }

    pub(crate) const fn text_mut(&mut self) -> Option<&mut SmallText> {
        match self {
            Self::Label { text, .. }
            | Self::Button { text, .. }
            | Self::Checkbox { text, .. }
            | Self::Radio { text, .. }
            | Self::Switch { text, .. }
            | Self::ListItem { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The on/off state of a checkbox, a radio button or a list item.
    pub(crate) const fn flag_mut(&mut self) -> Option<&mut bool> {
        match self {
            Self::Checkbox { checked, .. } | Self::Radio { checked, .. } => Some(checked),
            Self::Switch { on, .. } => Some(on),
            Self::ListItem { selected, .. } => Some(selected),
            _ => None,
        }
    }
}

/// Which passes have work, as a small bit set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Flags(u8);

impl Flags {
    pub(crate) const LAYOUT: u8 = 1;
    pub(crate) const PAINT: u8 = 2;
    pub(crate) const ANIM: u8 = 4;

    pub(crate) const fn has(self, f: u8) -> bool {
        self.0 & f != 0
    }

    pub(crate) fn insert(&mut self, f: u8) {
        self.0 |= f;
    }

    pub(crate) fn remove(&mut self, f: u8) {
        self.0 &= !f;
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub(crate) alive: bool,
    pub(crate) generation: u8,
    pub(crate) kind: Kind,
    pub(crate) style: Style,
    pub(crate) parent: Option<u16>,
    pub(crate) first: Option<u16>,
    pub(crate) last: Option<u16>,
    pub(crate) next: Option<u16>,
    pub(crate) prev: Option<u16>,
    /// The size the node wants, from the layout pass.
    pub(crate) size: Size,
    /// Where the node is, in tree coordinates, from the layout pass.
    pub(crate) rect: Rect,
    /// Passes this node asked for.
    pub(crate) own: Flags,
    /// Passes this node or one of its descendants asked for.
    pub(crate) sub: Flags,
    pub(crate) hovered: bool,
    pub(crate) active: bool,
    pub(crate) disabled: bool,
    /// Disabled itself or through an ancestor.
    pub(crate) disabled_eff: bool,
}

impl Node {
    const EMPTY: Self = Self {
        alive: false,
        generation: 0,
        kind: Kind::Spacer,
        style: Style::DEFAULT,
        parent: None,
        first: None,
        last: None,
        next: None,
        prev: None,
        size: Size { w: 0, h: 0 },
        rect: Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
        own: Flags(0),
        sub: Flags(0),
        hovered: false,
        active: false,
        disabled: false,
        disabled_eff: false,
    };
}

/// What a widget reports to the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// A button was pressed and released on itself (or activated from the keyboard).
    Clicked,
    /// The text of a text field was changed by the user (typing, deleting, pasting, an input method). Read it
    /// with [`Tree::text`]. Not sent for [`Tree::set_text`].
    TextChanged,
    /// The user asked to close the open dialog: pressed outside it, or pressed Cancel (Escape). The app decides whether
    /// to [`close_dialog`](Tree::close_dialog); nothing closes by itself.
    Dismissed,
    /// A checkbox or a switch was switched by the user; carries the new state.
    Toggled(bool),
    /// A slider was moved by the user; carries the new value in thousandths. Not sent for changes made
    /// with [`Tree::set_slider`].
    Changed(u16),
    /// A radio button or a list item was chosen by the user. It is not sent when the one already
    /// chosen is activated again, nor for changes made with [`Tree::set_checked`].
    Selected,
}

/// Something that happened in the tree, for the application to handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    /// The widget it came from.
    pub source: NodeId,
    /// What happened.
    pub kind: ActionKind,
}

const ACTION_QUEUE: usize = 8;

/// A retained widget tree of at most `N` nodes.
///
/// Nodes keep their state (text, hover, pressed, animation) between frames, and the application
/// changes them through methods that mark what needs redoing. The passes that follow are
/// [`pointer`](Self::pointer) (events), [`update`](Self::update) (state and layout),
/// [`advance`](Self::advance) (animation) and [`paint`](Self::paint).
#[derive(Clone, Debug)]
pub struct Tree<const N: usize> {
    pub(crate) nodes: [Node; N],
    pub(crate) root: Option<u16>,
    /// The last place the pointer was, if it is over the tree.
    pub(crate) pointer: Option<Point>,
    /// The widget that gets the release of a press.
    pub(crate) captured: Option<NodeId>,
    /// A press inside a scroll container, which becomes a scroll when dragged far enough.
    pub(crate) drag: Option<Drag>,
    /// What a scroll container is doing after a finger let go: a fling, or a bounce back from an end.
    pub(crate) motion: Option<crate::scrollphys::Motion>,
    /// A press stopped a moving scroll: the lift after it is not a click.
    pub(crate) caught: bool,
    /// A widget to scroll into view at the next layout.
    pub(crate) pending_reveal: Option<NodeId>,
    /// Where the next character of the focused text field will appear, as of the last layout.
    pub(crate) caret: Option<Rect>,
    /// What an input method is composing, shown at the cursor of the focused text field.
    pub(crate) preedit: InputText,
    /// A press on a text field, to put its cursor at the next layout (which has a renderer to measure with).
    pub(crate) pending_click: Option<(NodeId, i32)>,
    pub(crate) hovered: Option<NodeId>,
    /// The widget the keyboard acts on.
    pub(crate) focus: Option<NodeId>,
    /// Whether to draw the focus ring: after a key, not after a touch or a click.
    pub(crate) focus_visible: bool,
    pub(crate) bounds: Option<Rect>,
    pub(crate) disabled_dirty: bool,
    /// The area that needs painting again: everything that changed, as one bounding rectangle.
    pub(crate) damage: Option<Rect>,
    /// The part of the damage that is a change and not the move of a scroll (see `scrollshift`).
    pub(crate) rest: Option<Rect>,
    /// A scroll container whose pixels are moved instead of redrawn.
    pub(crate) shift: Option<crate::scrollshift::ShiftState>,
    /// Whether the layout has run since the shift began: damage after that is in the positions the rows will be drawn.
    pub(crate) shift_laid: bool,
    actions: [Option<Action>; ACTION_QUEUE],
    action_head: usize,
    action_len: usize,
    dropped_actions: u32,
}

impl<const N: usize> Default for Tree<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Tree<N> {
    /// An empty tree.
    #[must_use]
    pub fn new() -> Self {
        const { assert!(N <= u16::MAX as usize, "a tree holds at most 65535 nodes") };
        Self {
            nodes: [Node::EMPTY; N],
            root: None,
            pointer: None,
            captured: None,
            drag: None,
            motion: None,
            caught: false,
            pending_reveal: None,
            caret: None,
            preedit: InputText::EMPTY,
            pending_click: None,
            hovered: None,
            focus: None,
            focus_visible: false,
            bounds: None,
            disabled_dirty: false,
            damage: None,
            rest: None,
            shift: None,
            shift_laid: false,
            actions: [None; ACTION_QUEUE],
            action_head: 0,
            action_len: 0,
            dropped_actions: 0,
        }
    }

    /// Removes every node.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// The index of `id` if it still names a node.
    pub(crate) fn slot(&self, id: NodeId) -> Option<usize> {
        let i = usize::from(id.index);
        let n = self.nodes.get(i)?;
        (n.alive && n.generation == id.generation).then_some(i)
    }

    pub(crate) fn id_of(&self, i: usize) -> NodeId {
        NodeId {
            index: i as u16,
            generation: self.nodes[i].generation,
        }
    }

    /// The root, if there is one.
    #[must_use]
    pub fn root(&self) -> Option<NodeId> {
        self.root.map(|i| self.id_of(usize::from(i)))
    }

    /// How many nodes there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| n.alive).count()
    }

    /// Whether the tree has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Marks that a pass has work for node `i`, and for every ancestor, so a pass can tell from the
    /// root whether anything below needs it.
    /// Like [`request`](Self::request) for a paint, but only `part` of the node needs redrawing (a slider thumb that
    /// moved a little): the damage is that part, cut to the scroll windows the node is in.
    pub(crate) fn request_part(&mut self, i: usize, flag: u8, part: Rect) {
        let part = self.clip_to_viewports(i, part.intersect(&self.nodes[i].rect));
        self.add_damage(part);
        self.nodes[i].own.insert(flag);
        let mut cur = Some(i);
        while let Some(c) = cur {
            self.nodes[c].sub.insert(flag);
            cur = self.nodes[c].parent.map(usize::from);
        }
    }

    pub(crate) fn request(&mut self, i: usize, flag: u8) {
        if flag & Flags::PAINT != 0 {
            let rect = self.visible_rect(i);
            self.add_damage(rect);
        }
        self.nodes[i].own.insert(flag);
        let mut cur = Some(i);
        while let Some(c) = cur {
            self.nodes[c].sub.insert(flag);
            cur = self.nodes[c].parent.map(usize::from);
        }
    }

    /// The part of node `i` that can be seen: its rectangle cut to the viewports of the scroll containers
    /// it is in.
    pub(crate) fn visible_rect(&self, i: usize) -> Rect {
        self.clip_to_viewports(i, self.nodes[i].rect)
    }

    /// `rect` cut to the viewports of the scroll containers above node `i`.
    pub(crate) fn clip_to_viewports(&self, i: usize, mut rect: Rect) -> Rect {
        let mut cur = self.nodes[i].parent.map(usize::from);
        while let Some(p) = cur {
            if matches!(self.nodes[p].kind, Kind::Scroll { .. }) {
                rect = rect.intersect(&self.nodes[p].rect);
            }
            cur = self.nodes[p].parent.map(usize::from);
        }
        rect
    }

    /// Adds `rect` to the area that needs painting.
    pub(crate) fn add_damage(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        // A change that reaches into a shifting window means its moved pixels cannot be trusted: redraw all of it.
        let hits = self
            .shift
            .and_then(|s| self.slot(s.scroll))
            .is_some_and(|i| {
                let o = self.nodes[i].rect.intersect(&rect);
                o.w > 0 && o.h > 0
            });
        if hits && !self.shift_laid {
            self.cancel_shift();
        }
        self.damage = Some(self.damage.map_or(rect, |d| d.union(&rect)));
        self.rest = Some(self.rest.map_or(rect, |d| d.union(&rect)));
    }

    /// Damage that the screen has to be sent but that needs no drawing: pixels that were moved.
    pub(crate) fn add_present_damage(&mut self, rect: Rect) {
        if !rect.is_empty() {
            self.damage = Some(self.damage.map_or(rect, |d| d.union(&rect)));
        }
    }

    /// Moves a node to `rect`; if that changes where it is, both the old and the new place need
    /// painting again.
    pub(crate) fn set_rect(&mut self, i: usize, rect: Rect) {
        let old = self.nodes[i].rect;
        if old != rect {
            let (old_seen, new_seen) = (
                self.clip_to_viewports(i, old),
                self.clip_to_viewports(i, rect),
            );
            // Rows moved by a scroll are not changes: the shift accounts for them.
            if !self.in_shift(i) {
                self.add_damage(old_seen);
                self.add_damage(new_seen);
            }
            self.nodes[i].rect = rect;
        }
    }

    pub(crate) fn clear_flag(&mut self, flag: u8) {
        for n in &mut self.nodes {
            n.own.remove(flag);
            n.sub.remove(flag);
        }
    }

    /// Adds a node under `parent` (or as the root when `parent` is `None`).
    pub(crate) fn add(
        &mut self,
        parent: Option<NodeId>,
        kind: Kind,
        style: Style,
    ) -> Result<NodeId, BuildError> {
        let parent_slot = match parent {
            None => {
                if self.root.is_some() {
                    return Err(BuildError::SecondRoot);
                }
                None
            }
            Some(p) => {
                let i = self.slot(p).ok_or(BuildError::NoSuchParent)?;
                if !matches!(
                    self.nodes[i].kind,
                    Kind::Stack(_) | Kind::Scroll { .. } | Kind::Dialog { .. }
                ) {
                    return Err(BuildError::NotContainer);
                }
                Some(i)
            }
        };
        let free = self
            .nodes
            .iter()
            .position(|n| !n.alive)
            .ok_or(BuildError::Full)?;
        let generation = self.nodes[free].generation;
        let prev = parent_slot.and_then(|p| self.nodes[p].last);
        self.nodes[free] = Node {
            alive: true,
            generation,
            kind,
            style,
            parent: parent_slot.map(|p| p as u16),
            prev,
            disabled_eff: parent_slot.is_some_and(|p| self.nodes[p].disabled_eff),
            ..Node::EMPTY
        };
        if let Some(p) = parent_slot {
            match prev {
                Some(l) => self.nodes[usize::from(l)].next = Some(free as u16),
                None => self.nodes[p].first = Some(free as u16),
            }
            self.nodes[p].last = Some(free as u16);
        } else {
            self.root = Some(free as u16);
        }
        self.request(free, Flags::LAYOUT | Flags::PAINT);
        Ok(self.id_of(free))
    }

    /// Adds a column.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn column(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.add(parent, Kind::Stack(Axis::Column), style)
    }

    /// Adds a row.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn row(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.add(parent, Kind::Stack(Axis::Row), style)
    }

    /// Adds a label.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn label(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        role: TextRole,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Label {
                text: SmallText::new(text),
                role,
            },
            Style::DEFAULT,
        )
    }

    /// Adds a button.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn button(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        variant: ButtonVariant,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Button {
                text: SmallText::new(text),
                variant,
                blend: Fx::ZERO,
            },
            Style::DEFAULT,
        )
    }

    /// Adds a checkbox. Switching it by the user sends [`ActionKind::Toggled`].
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn checkbox(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        checked: bool,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Checkbox {
                text: SmallText::new(text),
                checked,
            },
            Style::DEFAULT,
        )
    }

    /// Adds a radio button. The radio buttons that share a parent are one group: choosing one
    /// clears the others, so put each group in its own container. Choosing sends
    /// [`ActionKind::Selected`].
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn radio(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        checked: bool,
    ) -> Result<NodeId, BuildError> {
        let id = self.add(
            parent,
            Kind::Radio {
                text: SmallText::new(text),
                checked: false,
            },
            Style::DEFAULT,
        )?;
        if checked {
            self.set_checked(id, true);
        }
        Ok(id)
    }

    /// Adds a row of a list that can be chosen from. The list items that share a parent are one
    /// list with at most one chosen; choosing sends [`ActionKind::Selected`].
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn list_item(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        selected: bool,
    ) -> Result<NodeId, BuildError> {
        let id = self.add(
            parent,
            Kind::ListItem {
                text: SmallText::new(text),
                selected: false,
            },
            Style::DEFAULT,
        )?;
        if selected {
            self.set_checked(id, true);
        }
        Ok(id)
    }

    /// Adds a scroll container: a column that clips its children to itself and scrolls them when they are
    /// taller than it. Give it a height (`Length::Fixed` or `Length::Fill`); with `Length::Content` it grows
    /// to its content and never scrolls. Children with a `Length::Fill` height take their natural size.
    /// Dragging, the mouse wheel and moving the focus to a hidden widget scroll it.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn scroll(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Scroll {
                offset: 0,
                content: 0,
                over: 0,
            },
            style,
        )
    }

    /// Adds a text field holding `text` (cut to [`crate::INPUT_CAP`] bytes), with the cursor at the end. It is
    /// focusable; typing, the edit commands and an input method change it and send
    /// [`ActionKind::TextChanged`]; Enter sends [`ActionKind::Clicked`] ("submit"). It is as wide as six
    /// controls are high unless the style says otherwise.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn text_input(&mut self, parent: Option<NodeId>, text: &str) -> Result<NodeId, BuildError> {
        let text = InputText::new(text);
        let end = text.as_str().len() as u8;
        self.add(
            parent,
            Kind::TextInput {
                text,
                cursor: end,
                anchor: end,
            },
            Style::DEFAULT,
        )
    }

    /// Adds a switch (a toggle). Switching it by the user sends [`ActionKind::Toggled`].
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn switch(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        on: bool,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Switch {
                text: SmallText::new(text),
                on,
            },
            Style::DEFAULT,
        )
    }

    /// Adds a slider, `permille` thousandths along (0 to 1000, more is 1000). Dragging it, or the Left and
    /// Right keys while it has the focus, send [`ActionKind::Changed`]. It wants a width: use
    /// [`CrossAlign::Stretch`] in a column or a [`Length::Fill`] width.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn slider(&mut self, parent: Option<NodeId>, permille: u16) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Slider {
                permille: permille.min(1000),
            },
            Style::DEFAULT,
        )
    }

    /// Adds a progress bar, `permille` thousandths full (0 to 1000, more is 1000). It takes the width
    /// its parent gives it: use [`CrossAlign::Stretch`] in a column or a [`Length::Fill`] width.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn progress_bar(
        &mut self,
        parent: Option<NodeId>,
        permille: u16,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Progress {
                permille: permille.min(1000),
            },
            Style::DEFAULT,
        )
    }

    /// Adds empty space, usually with a [`Length::Fill`] size.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn spacer(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.add(parent, Kind::Spacer, style)
    }

    /// Removes a node and everything below it. Handles to them go stale. Returns false for a stale
    /// handle.
    pub fn remove(&mut self, id: NodeId) -> bool {
        let Some(top) = self.slot(id) else {
            return false;
        };
        // Unlink from the parent first; the subtree keeps its own links while it is walked.
        let (parent, prev, next) = {
            let n = &self.nodes[top];
            (n.parent.map(usize::from), n.prev, n.next)
        };
        if let Some(p) = prev {
            self.nodes[usize::from(p)].next = next;
        } else if let Some(p) = parent {
            self.nodes[p].first = next;
        }
        if let Some(nx) = next {
            self.nodes[usize::from(nx)].prev = prev;
        } else if let Some(p) = parent {
            self.nodes[p].last = prev;
        }
        let mut cur = Some(top);
        while let Some(c) = cur {
            let following = self.preorder_next(c, top);
            let id = self.id_of(c);
            if self.captured == Some(id) {
                self.captured = None;
            }
            if self.hovered == Some(id) {
                self.hovered = None;
            }
            if self.focus == Some(id) {
                self.focus = None;
            }
            let gone = self.nodes[c].rect;
            self.add_damage(gone);
            let n = &mut self.nodes[c];
            n.alive = false;
            n.generation = n.generation.wrapping_add(1);
            n.own = Flags::default();
            n.sub = Flags::default();
            cur = following;
        }
        match parent {
            Some(p) => self.request(p, Flags::LAYOUT | Flags::PAINT),
            None => {
                self.root = None;
                self.bounds = None;
            }
        }
        true
    }

    /// The next node in depth-first order after `i`, staying inside the subtree of `top`.
    pub(crate) fn preorder_next(&self, i: usize, top: usize) -> Option<usize> {
        if let Some(c) = self.nodes[i].first {
            return Some(usize::from(c));
        }
        let mut cur = i;
        while cur != top {
            if let Some(n) = self.nodes[cur].next {
                return Some(usize::from(n));
            }
            cur = usize::from(self.nodes[cur].parent?);
        }
        None
    }

    pub(crate) fn leftmost_leaf(&self, mut i: usize) -> usize {
        while let Some(c) = self.nodes[i].first {
            i = usize::from(c);
        }
        i
    }

    /// The next node with all of its descendants already visited, after `i`.
    pub(crate) fn post_next(&self, i: usize) -> Option<usize> {
        match self.nodes[i].next {
            Some(n) => Some(self.leftmost_leaf(usize::from(n))),
            None => self.nodes[i].parent.map(usize::from),
        }
    }

    /// The children of `id`, first to last.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut next = self.slot(id).and_then(|i| self.nodes[i].first);
        core::iter::from_fn(move || {
            let i = usize::from(next?);
            next = self.nodes[i].next;
            Some(self.id_of(i))
        })
    }

    /// The parent of `id`.
    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let p = self.nodes[self.slot(id)?].parent?;
        Some(self.id_of(usize::from(p)))
    }

    /// Where `id` is, as of the last layout pass.
    #[must_use]
    pub fn rect(&self, id: NodeId) -> Option<Rect> {
        Some(self.nodes[self.slot(id)?].rect)
    }

    /// The text of a label, a button, a checkbox, a radio button or a list item.
    #[must_use]
    pub fn text(&self, id: NodeId) -> Option<&str> {
        let kind = &self.nodes[self.slot(id)?].kind;
        match kind {
            Kind::TextInput { text, .. } => Some(text.as_str()),
            _ => kind.text().map(SmallText::as_str),
        }
    }

    /// Whether the pointer is over `id`.
    #[must_use]
    pub fn is_hovered(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(|i| self.nodes[i].hovered)
    }

    /// Whether `id` is being pressed.
    #[must_use]
    pub fn is_active(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(|i| self.nodes[i].active)
    }

    /// Whether `id` cannot be used, itself or through an ancestor (as of the last update pass).
    #[must_use]
    pub fn is_disabled(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(|i| self.nodes[i].disabled_eff)
    }

    /// Changes the text of a label, a button, a checkbox, a radio button or a list item; the layout is
    /// redone.
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        let Some(i) = self.slot(id) else { return };
        if let Kind::TextInput {
            text: t,
            cursor,
            anchor,
        } = &mut self.nodes[i].kind
        {
            if t.as_str() != InputText::new(text).as_str() {
                t.set(text);
                // The cursor goes to the end, like a field filled in by the program.
                let end = t.as_str().len() as u8;
                (*cursor, *anchor) = (end, end);
                self.request(i, Flags::LAYOUT | Flags::PAINT);
            }
            return;
        }
        if let Some(t) = self.nodes[i].kind.text_mut()
            && t.as_str() != SmallText::new(text).as_str()
        {
            t.set(text);
            self.request(i, Flags::LAYOUT | Flags::PAINT);
        }
    }

    /// Changes the size and look of a node; the layout is redone.
    pub fn set_style(&mut self, id: NodeId, style: Style) {
        let Some(i) = self.slot(id) else { return };
        if self.nodes[i].style != style {
            self.nodes[i].style = style;
            self.request(i, Flags::LAYOUT | Flags::PAINT);
        }
    }

    /// Changes the look of a button.
    pub fn set_variant(&mut self, id: NodeId, variant: ButtonVariant) {
        let Some(i) = self.slot(id) else { return };
        if let Kind::Button { variant: v, .. } = &mut self.nodes[i].kind
            && *v != variant
        {
            *v = variant;
            self.request(i, Flags::PAINT);
        }
    }

    /// Enables or disables a node and everything below it.
    pub fn set_enabled(&mut self, id: NodeId, enabled: bool) {
        let Some(i) = self.slot(id) else { return };
        if self.nodes[i].disabled == enabled {
            self.nodes[i].disabled = !enabled;
            self.disabled_dirty = true;
            self.request(i, Flags::PAINT);
        }
    }

    pub(crate) fn push_action(&mut self, action: Action) {
        if self.action_len == ACTION_QUEUE {
            self.dropped_actions = self.dropped_actions.saturating_add(1);
            return;
        }
        let at = (self.action_head + self.action_len) % ACTION_QUEUE;
        self.actions[at] = Some(action);
        self.action_len += 1;
    }

    /// The next action the widgets produced, oldest first.
    pub fn pop_action(&mut self) -> Option<Action> {
        if self.action_len == 0 {
            return None;
        }
        let a = self.actions[self.action_head].take();
        self.action_head = (self.action_head + 1) % ACTION_QUEUE;
        self.action_len -= 1;
        a
    }

    /// How many actions were lost because the queue (8 deep) was full.
    #[must_use]
    pub const fn dropped_actions(&self) -> u32 {
        self.dropped_actions
    }

    /// The area that needs painting again, if any: the bounding rectangle of everything that changed
    /// since the last paint, in tree coordinates. Taking it clears it; pair it with
    /// [`paint_region`](Self::paint_region) to redraw only that part of the screen.
    pub fn take_damage(&mut self) -> Option<Rect> {
        self.cancel_shift();
        self.rest = None;
        self.damage.take()
    }

    /// Like [`take_damage`](Self::take_damage), limited to `limit` (the surface or the screen), so a
    /// node bigger than the screen does not make the area bigger than the screen. Damage entirely
    /// outside `limit` is no damage.
    pub fn take_damage_in(&mut self, limit: Rect) -> Option<Rect> {
        self.cancel_shift();
        self.rest = None;
        let clipped = self.damage.take()?.intersect(&limit);
        (!clipped.is_empty()).then_some(clipped)
    }

    /// Asks for everything to be painted again, for a change the tree cannot see: another theme.
    pub fn invalidate(&mut self) {
        let Some(root) = self.root.map(usize::from) else {
            return;
        };
        self.add_damage(self.nodes[root].rect);
        self.request(root, Flags::PAINT);
    }

    /// Whether anything needs a new frame: a redraw, a layout or a running animation.
    #[must_use]
    pub fn needs_frame(&self) -> bool {
        self.root.is_some_and(|r| {
            let f = self.nodes[usize::from(r)].sub;
            f.has(Flags::LAYOUT) || f.has(Flags::PAINT) || f.has(Flags::ANIM)
        }) || self.disabled_dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_text_cuts_on_a_character_boundary_and_keeps_short_text() {
        assert_eq!(SmallText::new("hello").as_str(), "hello");
        assert_eq!(SmallText::EMPTY.as_str(), "");
        let long = "é".repeat(40);
        let t = SmallText::new(&long);
        assert!(t.as_str().len() <= TEXT_CAP && t.as_str().chars().all(|c| c == 'é'));
        let mut t = SmallText::new("abc");
        t.set("");
        assert_eq!(t.as_str(), "");
        assert_eq!(SmallText::new(&"x".repeat(100)).as_str().len(), TEXT_CAP);
    }

    #[test]
    fn building_rules_are_enforced() {
        let mut t = Tree::<3>::new();
        assert!(t.is_empty() && t.root().is_none());
        assert_eq!(t.len(), 0);
        let root = t.column(None, Style::new()).unwrap();
        assert_eq!(t.column(None, Style::new()), Err(BuildError::SecondRoot));
        let label = t.label(Some(root), "a", TextRole::Body).unwrap();
        assert_eq!(
            t.label(Some(label), "b", TextRole::Body),
            Err(BuildError::NotContainer)
        );
        t.button(Some(root), "ok", ButtonVariant::Primary).unwrap();
        assert_eq!(t.spacer(Some(root), Style::new()), Err(BuildError::Full));
        assert_eq!(t.len(), 3);
        let stale = NodeId {
            index: 2,
            generation: 9,
        };
        assert_eq!(
            t.spacer(Some(stale), Style::new()),
            Err(BuildError::NoSuchParent)
        );
        let outside = NodeId {
            index: 500,
            generation: 0,
        };
        assert!(t.rect(outside).is_none() && t.text(outside).is_none() && !t.is_hovered(outside));
    }

    #[test]
    fn children_come_in_order_and_know_their_parent() {
        let mut t = Tree::<8>::new();
        let root = t.row(None, Style::new()).unwrap();
        let a = t.label(Some(root), "a", TextRole::Body).unwrap();
        let b = t.label(Some(root), "b", TextRole::Body).unwrap();
        let c = t.label(Some(root), "c", TextRole::Body).unwrap();
        assert!(t.children(root).eq([a, b, c]));
        assert_eq!(t.parent(b), Some(root));
        assert_eq!(t.parent(root), None);
        assert_eq!(t.children(a).count(), 0);
        assert_eq!(t.text(b), Some("b"));
        assert_eq!(t.text(root), None);
    }

    #[test]
    fn removing_a_subtree_unlinks_it_and_stales_its_handles() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let a = t.label(Some(root), "a", TextRole::Body).unwrap();
        let group = t.row(Some(root), Style::new()).unwrap();
        let g1 = t.label(Some(group), "g1", TextRole::Body).unwrap();
        let g2 = t
            .button(Some(group), "g2", ButtonVariant::Secondary)
            .unwrap();
        let z = t.label(Some(root), "z", TextRole::Body).unwrap();
        assert_eq!(t.len(), 6);
        assert!(t.remove(group));
        assert_eq!(t.len(), 3);
        assert!(t.children(root).eq([a, z]), "siblings are joined up");
        for gone in [group, g1, g2] {
            assert!(t.rect(gone).is_none() && !t.remove(gone));
        }
        // The freed slots are reused, and the old handles stay stale.
        let again = t.label(Some(root), "new", TextRole::Body).unwrap();
        assert!(t.rect(group).is_none() && t.rect(g1).is_none());
        assert_eq!(t.text(again), Some("new"));
        assert!(t.children(root).eq([a, z, again]));
        // Removing the first and the last child.
        assert!(t.remove(a) && t.remove(again));
        assert!(t.children(root).eq([z]));
        assert!(t.remove(root));
        assert!(t.is_empty());
        assert!(
            t.column(None, Style::new()).is_ok(),
            "a new root can follow"
        );
    }

    #[test]
    fn requests_travel_up_so_the_root_knows() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let row = t.row(Some(root), Style::new()).unwrap();
        let leaf = t.label(Some(row), "x", TextRole::Body).unwrap();
        t.clear_flag(Flags::LAYOUT | Flags::PAINT | Flags::ANIM);
        assert!(!t.needs_frame());
        t.set_text(leaf, "y");
        let (r, m, l) = (
            t.slot(root).unwrap(),
            t.slot(row).unwrap(),
            t.slot(leaf).unwrap(),
        );
        for i in [r, m, l] {
            assert!(
                t.nodes[i].sub.has(Flags::LAYOUT) && t.nodes[i].sub.has(Flags::PAINT),
                "node {i}"
            );
        }
        assert!(t.nodes[l].own.has(Flags::LAYOUT) && !t.nodes[r].own.has(Flags::LAYOUT));
        assert!(t.needs_frame());
    }

    #[test]
    fn setters_change_things_only_when_they_change_and_ignore_stale_handles() {
        let mut t = Tree::<4>::new();
        let root = t.column(None, Style::new()).unwrap();
        let b = t.button(Some(root), "go", ButtonVariant::Primary).unwrap();
        t.clear_flag(Flags::LAYOUT | Flags::PAINT);
        t.set_text(b, "go");
        t.set_style(b, Style::new());
        t.set_variant(b, ButtonVariant::Primary);
        assert!(!t.needs_frame(), "setting the same value is a no-op");
        t.set_variant(b, ButtonVariant::Secondary);
        assert!(t.needs_frame());
        t.set_text(root, "not text");
        assert_eq!(t.text(root), None);
        t.remove(b);
        t.set_text(b, "x");
        t.set_enabled(b, false);
        t.set_style(b, Style::new().padding(3));
        t.set_variant(b, ButtonVariant::Primary);
    }

    #[test]
    fn the_action_queue_is_first_in_first_out_and_bounded() {
        let mut t = Tree::<2>::new();
        let id = t.button(None, "x", ButtonVariant::Primary).unwrap();
        assert_eq!(t.pop_action(), None);
        for _ in 0..12 {
            t.push_action(Action {
                source: id,
                kind: ActionKind::Clicked,
            });
        }
        assert_eq!(t.dropped_actions(), 4);
        let mut n = 0;
        while t.pop_action().is_some() {
            n += 1;
        }
        assert_eq!(n, 8);
        t.push_action(Action {
            source: id,
            kind: ActionKind::Clicked,
        });
        assert!(t.pop_action().is_some() && t.pop_action().is_none());
    }

    #[test]
    fn depth_first_walks_visit_every_node_once_in_the_right_order() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let a = t.row(Some(root), Style::new()).unwrap();
        let a1 = t.label(Some(a), "a1", TextRole::Body).unwrap();
        let a2 = t.label(Some(a), "a2", TextRole::Body).unwrap();
        let b = t.label(Some(root), "b", TextRole::Body).unwrap();
        let top = t.slot(root).unwrap();
        let mut pre = [usize::MAX; 8];
        let (mut n, mut cur) = (0, Some(top));
        while let Some(c) = cur {
            pre[n] = c;
            n += 1;
            cur = t.preorder_next(c, top);
        }
        let order = [root, a, a1, a2, b].map(|id| t.slot(id).unwrap());
        assert_eq!(&pre[..n], &order);
        // Post-order: children before parents, ending at the root.
        let mut post = [usize::MAX; 8];
        let (mut n, mut cur) = (0, Some(t.leftmost_leaf(top)));
        while let Some(c) = cur {
            post[n] = c;
            n += 1;
            cur = if c == top { None } else { t.post_next(c) };
        }
        let want = [a1, a2, a, b, root].map(|id| t.slot(id).unwrap());
        assert_eq!(&post[..n], &want);
    }
}
