//! Effects as values. `update` returns a [`Task`]: what should happen next, described, not done. The runtime does it and,
//! where there is an answer, sends it back as a message. The set is closed (a chip has nothing else to offer), small and
//! fixed in size, so a task needs no allocation and a test can read it.

use fusuma_ui::Id;

/// How many effects one task can carry.
pub const TASK_CAP: usize = 4;

/// One effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect<M> {
    /// Send this message at once (after the current one is finished).
    Send(M),
    /// Send this message after this many milliseconds of the app's time.
    After(u32, M),
    /// Give the focus to the node called this.
    Focus(Id),
    /// Scroll the container called this to this many pixels from the top.
    ScrollTo(Id, i32),
    /// Show this text in the toast called this, for this many milliseconds once it is in.
    Toast(Id, &'static str, u32),
    /// Hide the toast called this now.
    HideToast(Id),
}

/// What `update` asks the runtime to do: nothing, or up to [`TASK_CAP`] effects in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task<M> {
    effects: [Option<Effect<M>>; TASK_CAP],
    len: usize,
}

impl<M: Copy> Task<M> {
    /// Nothing to do.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            effects: [None; TASK_CAP],
            len: 0,
        }
    }

    /// One effect.
    #[must_use]
    pub const fn of(effect: Effect<M>) -> Self {
        let mut t = Self::none();
        t.effects[0] = Some(effect);
        t.len = 1;
        t
    }

    /// Send `message` right after this one.
    #[must_use]
    pub const fn send(message: M) -> Self {
        Self::of(Effect::Send(message))
    }

    /// Send `message` after `ms` milliseconds.
    #[must_use]
    pub const fn after(ms: u32, message: M) -> Self {
        Self::of(Effect::After(ms, message))
    }

    /// Give the focus to the node called `id`.
    #[must_use]
    pub const fn focus(id: Id) -> Self {
        Self::of(Effect::Focus(id))
    }

    /// Scroll the container called `id` to `offset` pixels.
    #[must_use]
    pub const fn scroll_to(id: Id, offset: i32) -> Self {
        Self::of(Effect::ScrollTo(id, offset))
    }

    /// Show `text` in the toast called `id` for `hold_ms`.
    #[must_use]
    pub const fn toast(id: Id, text: &'static str, hold_ms: u32) -> Self {
        Self::of(Effect::Toast(id, text, hold_ms))
    }

    /// This task, then `other`. Effects past [`TASK_CAP`] are dropped (and [`Task::overflowed`] says so).
    #[must_use]
    pub fn and(mut self, other: Self) -> Self {
        for e in other.effects().copied() {
            if self.len < TASK_CAP {
                self.effects[self.len] = Some(e);
                self.len += 1;
            } else {
                break;
            }
        }
        self
    }

    /// Whether the task is at its cap, so a further `and` drops effects. A program that needs more sends a message that
    /// continues.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.len == TASK_CAP
    }

    /// The effects, in order.
    pub fn effects(&self) -> impl Iterator<Item = &Effect<M>> + '_ {
        self.effects[..self.len].iter().flatten()
    }

    /// Whether there is nothing to do.
    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.len == 0
    }
}

impl<M: Copy> Default for Task<M> {
    fn default() -> Self {
        Self::none()
    }
}

/// How many recipes one subscription can carry.
pub const SUB_CAP: usize = 4;

/// What a program wants to hear about while its state is what it is.
#[derive(Clone, Copy, Debug)]
pub enum Recipe<M> {
    /// A message made from the milliseconds since the last frame, every frame. It keeps frames running: ask for it only
    /// while something is moving.
    Frames(u16, fn(u32) -> M),
    /// This message every so many milliseconds, from when it first appears.
    Every(u16, u32, M),
}

impl<M> Recipe<M> {
    /// The recipe's identity: the same id in two subscriptions is the same recipe, which the runtime keeps running
    /// instead of starting again.
    #[must_use]
    pub const fn id(&self) -> u16 {
        match self {
            Self::Frames(id, _) | Self::Every(id, ..) => *id,
        }
    }
}

/// A set of recipes, derived from the state by `subscription` after every `update`. The runtime starts the ones that
/// are new, keeps the ones that are still there and stops the ones that went, so a program says *what it wants now*
/// and never has to remember to cancel.
#[derive(Clone, Copy, Debug)]
pub struct Sub<M> {
    recipes: [Option<Recipe<M>>; SUB_CAP],
    len: usize,
}

impl<M: Copy> Sub<M> {
    /// Nothing to hear: the program sleeps until an input comes.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            recipes: [None; SUB_CAP],
            len: 0,
        }
    }

    /// One recipe.
    #[must_use]
    pub const fn of(recipe: Recipe<M>) -> Self {
        let mut s = Self::none();
        s.recipes[0] = Some(recipe);
        s.len = 1;
        s
    }

    /// `f(dt)` every frame, `id` naming the recipe.
    #[must_use]
    pub const fn frames(id: u16, f: fn(u32) -> M) -> Self {
        Self::of(Recipe::Frames(id, f))
    }

    /// `message` every `ms` milliseconds, `id` naming the recipe.
    #[must_use]
    pub const fn every(id: u16, ms: u32, message: M) -> Self {
        Self::of(Recipe::Every(id, ms, message))
    }

    /// This subscription and `other`; what does not fit is dropped.
    #[must_use]
    pub fn and(mut self, other: Self) -> Self {
        for r in other.recipes().copied() {
            if self.len < SUB_CAP {
                self.recipes[self.len] = Some(r);
                self.len += 1;
            }
        }
        self
    }

    /// The recipes.
    pub fn recipes(&self) -> impl Iterator<Item = &Recipe<M>> + '_ {
        self.recipes[..self.len].iter().flatten()
    }

    /// Whether there is nothing to hear.
    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.len == 0
    }
}

impl<M: Copy> Default for Sub<M> {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        A,
        B,
        Tick(u32),
    }

    #[test]
    fn a_task_is_a_list_of_effects_in_order_and_none_is_nothing() {
        assert!(Task::<Msg>::none().is_none());
        let t = Task::send(Msg::A)
            .and(Task::after(10, Msg::B))
            .and(Task::focus(Id::new(1)));
        let got: Vec<_> = t.effects().copied().collect();
        assert_eq!(
            got,
            [
                Effect::Send(Msg::A),
                Effect::After(10, Msg::B),
                Effect::Focus(Id::new(1))
            ]
        );
        assert_eq!(Task::<Msg>::none().and(Task::none()), Task::none());
    }

    #[test]
    fn a_task_past_its_cap_drops_the_rest_and_says_it_is_full() {
        let mut t = Task::send(Msg::A);
        for _ in 0..10 {
            t = t.and(Task::send(Msg::B));
        }
        assert_eq!(t.effects().count(), TASK_CAP);
        assert!(t.is_full());
        assert_eq!(
            t.effects().next(),
            Some(&Effect::Send(Msg::A)),
            "the first ones are the ones kept"
        );
    }

    #[test]
    fn a_subscription_is_recipes_with_identities() {
        assert!(Sub::<Msg>::none().is_none());
        let s = Sub::frames(1, Msg::Tick).and(Sub::every(2, 500, Msg::A));
        let ids: Vec<u16> = s.recipes().map(Recipe::id).collect();
        assert_eq!(ids, [1, 2]);
        let mut many = Sub::<Msg>::none();
        for k in 0..10 {
            many = many.and(Sub::every(k, 1, Msg::B));
        }
        assert_eq!(many.recipes().count(), SUB_CAP);
        let Some(Recipe::Frames(_, f)) = s.recipes().next().copied() else {
            panic!()
        };
        assert_eq!(f(16), Msg::Tick(16));
    }
}
