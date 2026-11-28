//! A program: the Elm architecture as one type a platform can run. You write `State`, `Msg`, `update`, `view` and
//! `subscription`; [`Runner`] owns the tree and does the rest, and is an [`App`], so it runs wherever a showcase does:
//! a window, a browser, a terminal, a board.

use fusuma_render::{Rect, Renderer};
use fusuma_ui::{App, Edit, Key, NodeId, Palette, Pointer, Screen, Shape, Style, Theme, Tree};

use super::{Children, Effect, Host, Recipe, Sub, Task};

/// How many timers (`Task::after`) can be waiting at once.
pub const TIMERS: usize = 8;
/// How many messages can be waiting to be handled at once (a message that sends more than this many is cut off).
pub const QUEUE: usize = 16;

/// An Elm program.
pub trait Program {
    /// Everything the program knows.
    type State: 'static;
    /// What can happen: small, and `Copy`, so a task and a subscription can hold it and a log can keep it. A value that
    /// comes with an event (a slider's position, the text of a field) is made by a `fn(..) -> Msg`, such as a variant.
    type Msg: Copy;

    /// The state to start from.
    fn init() -> Self::State;

    /// The one place the state changes. Returns what should happen next.
    fn update(state: &mut Self::State, msg: Self::Msg) -> Task<Self::Msg>;

    /// The screen for this state: the children of the page's root column. `screen` is the size, scale, shape and class of
    /// the display, so the layout can change with it.
    fn view<'a>(state: &'a Self::State, screen: &Screen) -> impl Children<Self::Msg> + 'a;

    /// Named starting states: a place to begin a demo, a screenshot or a test from (`Runner::from_preset`). None by
    /// default.
    fn presets() -> &'static [Preset<Self::State>] {
        &[]
    }

    /// A message for a change of the screen, or `None` (the default). It is asked when the device or its window changes in a way a
    /// layout cares about: the form factor, the layout class, the size class or the orientation (a rotation, a window dragged
    /// narrow, a monitor with another scale). Not for every pixel of a resize. The first screen the platform sends counts as a
    /// change from the one the program started with.
    fn on_screen(screen: &Screen) -> Option<Self::Msg> {
        let _ = screen;
        None
    }

    /// What the program wants to hear while its state is this. Nothing by default: the program sleeps until an input
    /// comes.
    fn subscription(state: &Self::State) -> Sub<Self::Msg> {
        let _ = state;
        Sub::none()
    }
}

/// A named starting state.
#[derive(Debug)]
pub struct Preset<S> {
    /// What it is called (in an address, a script, a test).
    pub name: &'static str,
    /// Makes the state.
    pub state: fn() -> S,
}

/// A recipe that is running.
#[derive(Clone, Copy, Debug)]
struct Active<M> {
    recipe: Recipe<M>,
    /// Milliseconds until an `Every` fires again, or a `Once` fires.
    left: u32,
    /// A `Once` that has fired: it stays so, and silent, for as long as it is wanted.
    spent: bool,
}

/// A timer started by `Task::after`.
#[derive(Clone, Copy, Debug)]
struct Timer<M> {
    left: u32,
    msg: M,
}

/// Runs a [`Program`] on a tree of `N` nodes with room for `K` message handlers, remembering the last `L` messages.
#[derive(Debug)]
pub struct Runner<P: Program, const N: usize, const K: usize, const L: usize = 0> {
    state: P::State,
    tree: Tree<N>,
    host: Host<P::Msg, K>,
    root: NodeId,
    theme: Theme,
    dark: bool,
    screen: Screen,
    timers: [Option<Timer<P::Msg>>; TIMERS],
    active: [Option<Active<P::Msg>>; super::SUB_CAP],
    queue: [Option<P::Msg>; QUEUE],
    queued: usize,
    /// The view has to be made again (a message was handled, or the screen changed).
    dirty: bool,
    /// The app's own clock, in milliseconds.
    now: u32,
    log: [Option<(u32, P::Msg)>; L],
    logged: usize,
}

impl<P: Program, const N: usize, const K: usize, const L: usize> Runner<P, N, K, L> {
    /// The program on a screen of `width` x `height` pixels.
    ///
    /// # Panics
    /// If `N` is too small for even the root node.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_state(P::init(), width, height)
    }
}

impl<P: Program, const N: usize, const K: usize, const L: usize> Runner<P, N, K, L> {
    /// The program started from the preset called `name` (see [`Program::presets`]), or `None` if there is none.
    #[must_use]
    pub fn from_preset(name: &str, width: u32, height: u32) -> Option<Self> {
        let preset = P::presets().iter().find(|p| p.name == name)?;
        Some(Self::with_state((preset.state)(), width, height))
    }

    /// The program, starting from `state` (a preset, or a state a log replayed to).
    ///
    /// # Panics
    /// If `N` is too small for even the root node.
    #[must_use]
    pub fn with_state(state: P::State, width: u32, height: u32) -> Self {
        let mut tree = Tree::new();
        let root = tree
            .column(None, Style::DEFAULT)
            .expect("a tree has room for its root");
        let mut r = Self {
            state,
            tree,
            host: Host::new(),
            root,
            theme: Theme::DARK,
            dark: true,
            screen: Screen::new(width, height),
            timers: [None; TIMERS],
            active: [None; super::SUB_CAP],
            queue: [None; QUEUE],
            queued: 0,
            dirty: true,
            now: 0,
            log: [None; L],
            logged: 0,
        };
        r.refresh_subscriptions();
        r
    }

    /// The program's state.
    #[must_use]
    pub const fn state(&self) -> &P::State {
        &self.state
    }

    /// The tree, to read (a test, a debugger, the semantic tree).
    #[must_use]
    pub const fn tree(&self) -> &Tree<N> {
        &self.tree
    }

    /// The tree, to drive (a simulator presses and types through it).
    pub fn tree_mut(&mut self) -> &mut Tree<N> {
        &mut self.tree
    }

    /// The program's clock, in milliseconds since it started.
    #[must_use]
    pub const fn now(&self) -> u32 {
        self.now
    }

    /// The screen the view is made for.
    #[must_use]
    pub const fn screen(&self) -> &Screen {
        &self.screen
    }

    /// The last messages handled, oldest first, with the time each was handled at (at most `L`: older ones are
    /// dropped). Empty when `L` is 0.
    pub fn log(&self) -> impl Iterator<Item = (u32, P::Msg)> + '_ {
        self.log[..self.logged.min(L)].iter().flatten().copied()
    }

    /// Sends `msg` to the program, as if a widget had: `update`, its task, and the view made again.
    pub fn send(&mut self, msg: P::Msg) {
        self.push(msg);
        self.drain();
    }

    fn push(&mut self, msg: P::Msg) {
        if let Some(slot) = self.queue.get_mut(self.queued) {
            *slot = Some(msg);
            self.queued += 1;
        }
    }

    fn pop(&mut self) -> Option<P::Msg> {
        if self.queued == 0 {
            return None;
        }
        let first = self.queue[0].take();
        // Shift the rest down: the queue is a handful of entries.
        for i in 1..self.queued {
            self.queue[i - 1] = self.queue[i].take();
        }
        self.queued -= 1;
        first
    }

    /// Handles every waiting message, and what they ask for, until there are none.
    fn drain(&mut self) {
        while let Some(msg) = self.pop() {
            self.record(msg);
            let task = P::update(&mut self.state, msg);
            self.dirty = true;
            self.run(task);
            self.refresh_subscriptions();
        }
    }

    /// Tells the program about a change that a layout cares about (see [`Program::on_screen`]).
    fn screen_changed(&mut self, before: &Screen) {
        let now = &self.screen;
        let same = before.form_factor() == now.form_factor()
            && before.class() == now.class()
            && before.size_class() == now.size_class()
            && before.is_reflective() == now.is_reflective()
            && (before.width >= before.height) == (now.width >= now.height);
        if !same && let Some(m) = P::on_screen(now) {
            self.push(m);
            self.drain();
        }
    }

    fn record(&mut self, msg: P::Msg) {
        if L == 0 {
            return;
        }
        let at = self.logged % L;
        self.log[at] = Some((self.now, msg));
        self.logged += 1;
    }

    fn run(&mut self, task: Task<P::Msg>) {
        for effect in task.effects().copied() {
            match effect {
                Effect::Send(m) => self.push(m),
                Effect::After(ms, m) => {
                    if let Some(slot) = self.timers.iter_mut().find(|t| t.is_none()) {
                        *slot = Some(Timer { left: ms, msg: m });
                    }
                }
                Effect::Focus(id) => {
                    self.tree.focus_id(id);
                }
                Effect::ScrollTo(id, offset) => {
                    if let Some(n) = self.tree.find(id) {
                        self.tree.scroll_to(n, offset);
                    }
                }
                Effect::Toast(id, text, hold) => {
                    if let Some(n) = self.tree.find(id) {
                        self.tree.show_toast(n, text, hold);
                    }
                }
                Effect::HideToast(id) => {
                    if let Some(n) = self.tree.find(id) {
                        self.tree.hide_toast(n);
                    }
                }
            }
        }
    }

    /// Starts the recipes that are new, keeps the ones still wanted, stops the rest.
    fn refresh_subscriptions(&mut self) {
        let wanted = P::subscription(&self.state);
        for slot in &mut self.active {
            if slot.is_some_and(|a| !wanted.recipes().any(|r| r.id() == a.recipe.id())) {
                *slot = None;
            }
        }
        for recipe in wanted.recipes().copied() {
            let running = self
                .active
                .iter()
                .flatten()
                .any(|a| a.recipe.id() == recipe.id());
            if running {
                continue;
            }
            if let Some(slot) = self.active.iter_mut().find(|a| a.is_none()) {
                let left = match recipe {
                    Recipe::Every(_, ms, _) | Recipe::Once(_, ms, _) => ms,
                    Recipe::Frames(..) | Recipe::Key(..) | Recipe::Resize(..) => 0,
                };
                *slot = Some(Active {
                    recipe,
                    left,
                    spent: false,
                });
            }
        }
    }

    /// Takes the tree's actions and turns the ones that are this program's into messages.
    fn collect_actions(&mut self) {
        while let Some(a) = self.tree.pop_action() {
            if let Some(m) = self.host.message(&self.tree, a) {
                self.push(m);
            }
        }
    }

    /// Makes the tree match the view of the state, if the state changed.
    fn sync(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let children = P::view(&self.state, &self.screen);
        // The tree has room for the program (a program is checked to fit), so a failure leaves a smaller page.
        let _ = self
            .host
            .sync_children(&mut self.tree, self.root, &children);
    }

    /// Whether anything keeps frames running: a frame subscription, or a change not yet shown.
    fn frames_wanted(&self) -> bool {
        self.dirty
            || self
                .active
                .iter()
                .flatten()
                .any(|a| matches!(a.recipe, Recipe::Frames(..)))
    }

    /// Replays `messages` through `update` from `state`, dropping the tasks they return (what a task caused arrives as
    /// messages of its own, which the log has). The state they lead to.
    pub fn replay(mut state: P::State, messages: impl IntoIterator<Item = P::Msg>) -> P::State {
        for m in messages {
            let _ = P::update(&mut state, m);
        }
        state
    }
}

impl<P: Program, const N: usize, const K: usize, const L: usize> App for Runner<P, N, K, L> {
    fn pointer(&mut self, event: Pointer) {
        self.tree.pointer(event);
        self.collect_actions();
        self.drain();
    }

    fn key(&mut self, key: Key) {
        // A shortcut the program subscribed to takes the key before the widgets do.
        let mut consumed = false;
        for i in 0..self.active.len() {
            if let Some(Active {
                recipe: Recipe::Key(_, f),
                ..
            }) = self.active[i]
                && let Some(m) = f(key)
            {
                self.push(m);
                consumed = true;
            }
        }
        if !consumed {
            self.tree.key(key);
        }
        self.collect_actions();
        self.drain();
    }

    fn text(&mut self, text: &str) {
        self.tree.type_text(text);
        self.collect_actions();
        self.drain();
    }

    fn edit(&mut self, edit: Edit) {
        self.tree.edit(edit);
        self.collect_actions();
        self.drain();
    }

    fn preedit(&mut self, text: &str) {
        self.tree.set_preedit(text);
    }

    fn wants_text(&self) -> bool {
        self.tree.wants_text()
    }

    fn text_cursor(&self) -> Option<Rect> {
        self.tree.text_cursor()
    }

    fn selected_text(&self) -> Option<&str> {
        self.tree.selected_text()
    }

    fn palette(&mut self, palette: Palette) {
        self.theme = Theme::DARK.with_colors(palette.colors(self.dark));
        self.tree.invalidate();
    }

    fn shape(&mut self, shape: Shape) {
        self.screen = self.screen.with_shape(shape);
        self.dirty = true;
    }

    fn screen(&mut self, screen: Screen) {
        // What the device is; the area and the scale are the ones the program already has.
        let before = self.screen;
        self.screen = Screen {
            width: self.screen.width,
            height: self.screen.height,
            scale_percent: self.screen.scale_percent,
            ..screen
        };
        self.theme = self.screen.theme(&self.theme);
        self.tree.invalidate();
        self.dirty = true;
        self.screen_changed(&before);
    }

    fn resized(&mut self, width: u32, height: u32, scale_percent: u32) {
        let before = self.screen;
        self.screen = Screen {
            width,
            height,
            ..self.screen
        }
        .with_scale(scale_percent);
        self.theme = self.screen.theme(&self.theme);
        self.tree.invalidate();
        // The view can choose its layout by the screen.
        self.dirty = true;
        // Whoever asked to hear of every resize does.
        if (before.width, before.height) != (width, height) {
            for i in 0..self.active.len() {
                if let Some(Active {
                    recipe: Recipe::Resize(_, f),
                    ..
                }) = self.active[i]
                {
                    self.push(f(width, height));
                }
            }
            self.drain();
        }
        self.screen_changed(&before);
    }

    fn tick(&mut self, dt_ms: u32) -> bool {
        self.now = self.now.saturating_add(dt_ms);
        let moving = self.tree.advance(&self.theme, dt_ms);
        // Timers.
        for i in 0..TIMERS {
            if let Some(t) = &mut self.timers[i] {
                t.left = t.left.saturating_sub(dt_ms);
                if t.left == 0 {
                    let msg = t.msg;
                    self.timers[i] = None;
                    self.push(msg);
                }
            }
        }
        // Subscriptions.
        for i in 0..self.active.len() {
            let Some(a) = self.active[i] else { continue };
            match a.recipe {
                Recipe::Frames(_, f) => self.push(f(dt_ms)),
                Recipe::Key(..) | Recipe::Resize(..) => {}
                Recipe::Once(_, _, m) => {
                    if a.spent {
                        continue;
                    }
                    let left = a.left.saturating_sub(dt_ms);
                    if let Some(slot) = &mut self.active[i] {
                        slot.left = left;
                        slot.spent = left == 0;
                    }
                    if left == 0 {
                        self.push(m);
                    }
                }
                Recipe::Every(_, ms, m) => {
                    let mut left = a.left;
                    let mut due = 0;
                    // A long frame can carry it over several periods.
                    let mut carry = dt_ms;
                    while carry >= left && ms > 0 && due < 4 {
                        carry -= left;
                        left = ms;
                        due += 1;
                    }
                    left = left.saturating_sub(carry);
                    if let Some(slot) = &mut self.active[i] {
                        slot.left = left;
                    }
                    for _ in 0..due {
                        self.push(m);
                    }
                }
            }
        }
        self.collect_actions();
        self.drain();
        moving || self.tree.needs_frame() || self.frames_wanted()
    }

    fn deadline(&self) -> Option<u32> {
        let timers = self.timers.iter().flatten().map(|t| t.left);
        let every = self.active.iter().flatten().filter_map(|a| match a.recipe {
            Recipe::Every(..) => Some(a.left),
            Recipe::Once(..) if !a.spent => Some(a.left),
            Recipe::Once(..) | Recipe::Frames(..) | Recipe::Key(..) | Recipe::Resize(..) => None,
        });
        timers.chain(every).chain(self.tree.deadline()).min()
    }

    fn frame<R: Renderer>(&mut self, renderer: &mut R) -> Result<Option<Rect>, R::Error> {
        self.sync();
        let bounds = self.screen.bounds();
        self.tree.update(renderer, &self.theme, bounds);
        self.tree.paint_frame(renderer, &self.theme, bounds)
    }

    fn frame_rects(&self) -> &[Rect] {
        self.tree.frame_rects()
    }

    fn changed(&self) -> Option<Rect> {
        self.tree.changed()
    }

    fn host_surfaces(&mut self, on: bool) {
        self.tree.set_host_surfaces(on);
    }

    fn paint_over<R: Renderer>(
        &mut self,
        renderer: &mut R,
        id: u32,
        region: Rect,
    ) -> Result<(), R::Error> {
        self.tree.paint_over(renderer, &self.theme, id, region)
    }

    fn paint_overlays<R: Renderer>(
        &mut self,
        renderer: &mut R,
        region: Rect,
    ) -> Result<(), R::Error> {
        self.tree.paint_overlays(renderer, &self.theme, region)
    }

    fn paint_topmost<R: Renderer>(
        &mut self,
        renderer: &mut R,
        region: Rect,
    ) -> Result<(), R::Error> {
        self.tree.paint_topmost(renderer, &self.theme, region)
    }

    fn layers(&self) -> Option<fusuma_ui::LayerPlan> {
        Some(self.tree.layers())
    }

    fn semantics(&self, visit: &mut dyn FnMut(&fusuma_ui::Semantic<'_>)) {
        for s in self.tree.semantics() {
            visit(&s);
        }
    }

    fn assist(&mut self, request: fusuma_ui::Assist<'_>) -> fusuma_ui::Assisted {
        let r = self.tree.assist(request);
        // What was asked changed the tree (a press makes a message): handle it as any input is.
        self.collect_actions();
        self.drain();
        r
    }
}
