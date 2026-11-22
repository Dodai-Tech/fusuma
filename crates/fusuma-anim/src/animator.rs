//! A fixed-size set of running animations driven by one frame clock.

use crate::{Decay, Fx, Spring, Tween};

/// A handle to a running animation. A handle goes stale when its animation is removed: the slot may
/// be reused, but the old handle then finds nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id {
    index: u8,
    generation: u8,
}

/// The animator has no free slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Tween(Tween),
    Spring(Spring),
    Decay(Decay),
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    generation: u8,
    kind: Option<Kind>,
}

/// Up to `N` animations (tweens, springs and fling decays) advanced by one clock.
///
/// Call [`step`](Self::step) once per frame with the current time in milliseconds; it returns
/// whether another frame is needed, so a screen that is not moving costs nothing. Finished
/// animations keep their final value until they are [`remove`](Self::remove)d.
#[derive(Clone, Copy, Debug)]
pub struct Animator<const N: usize> {
    slots: [Slot; N],
    last_ms: Option<u32>,
}

impl<const N: usize> Default for Animator<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Animator<N> {
    /// An animator with no animations.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [Slot {
                generation: 0,
                kind: None,
            }; N],
            last_ms: None,
        }
    }

    fn add(&mut self, kind: Kind) -> Result<Id, Full> {
        let (i, slot) = self
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.kind.is_none())
            .ok_or(Full)?;
        slot.kind = Some(kind);
        Ok(Id {
            index: u8::try_from(i).map_err(|_| Full)?,
            generation: slot.generation,
        })
    }

    fn slot(&self, id: Id) -> Option<&Kind> {
        let s = self.slots.get(usize::from(id.index))?;
        if s.generation == id.generation {
            s.kind.as_ref()
        } else {
            None
        }
    }

    fn slot_mut(&mut self, id: Id) -> Option<&mut Kind> {
        let s = self.slots.get_mut(usize::from(id.index))?;
        if s.generation == id.generation {
            s.kind.as_mut()
        } else {
            None
        }
    }

    /// Starts a tween (it carries its own start time).
    ///
    /// # Errors
    /// [`Full`] when every slot is in use.
    pub fn start_tween(&mut self, tween: Tween) -> Result<Id, Full> {
        self.add(Kind::Tween(tween))
    }

    /// Starts a spring.
    ///
    /// # Errors
    /// [`Full`] when every slot is in use.
    pub fn start_spring(&mut self, spring: Spring) -> Result<Id, Full> {
        self.add(Kind::Spring(spring))
    }

    /// Starts a fling.
    ///
    /// # Errors
    /// [`Full`] when every slot is in use.
    pub fn start_decay(&mut self, decay: Decay) -> Result<Id, Full> {
        self.add(Kind::Decay(decay))
    }

    /// Stops and forgets an animation; its handle goes stale. Returns false for a stale handle.
    pub fn remove(&mut self, id: Id) -> bool {
        let Some(s) = self.slots.get_mut(usize::from(id.index)) else {
            return false;
        };
        if s.generation != id.generation || s.kind.is_none() {
            return false;
        }
        s.kind = None;
        s.generation = s.generation.wrapping_add(1);
        true
    }

    /// The value of an animation at `now_ms`, or `None` for a stale handle.
    #[must_use]
    pub fn value(&self, id: Id, now_ms: u32) -> Option<Fx> {
        Some(match self.slot(id)? {
            Kind::Tween(t) => t.value(now_ms),
            Kind::Spring(s) => s.position(),
            Kind::Decay(d) => d.position(),
        })
    }

    /// Whether the animation has finished (a tween reached its end, a spring or fling came to
    /// rest). A stale handle counts as finished.
    #[must_use]
    pub fn is_done(&self, id: Id, now_ms: u32) -> bool {
        match self.slot(id) {
            None => true,
            Some(Kind::Tween(t)) => t.is_done(now_ms),
            Some(Kind::Spring(s)) => s.is_at_rest(),
            Some(Kind::Decay(d)) => d.is_at_rest(),
        }
    }

    /// The spring behind a handle, to pull it to a new target or give it an impulse. Springs only
    /// run while they are not at rest, and setting a target wakes them.
    pub fn spring_mut(&mut self, id: Id) -> Option<&mut Spring> {
        match self.slot_mut(id)? {
            Kind::Spring(s) => Some(s),
            _ => None,
        }
    }

    /// The fling behind a handle, to stop it when a finger touches it.
    pub fn decay_mut(&mut self, id: Id) -> Option<&mut Decay> {
        match self.slot_mut(id)? {
            Kind::Decay(d) => Some(d),
            _ => None,
        }
    }

    /// The tween behind a handle, to replace it, for instance with a retargeted one.
    pub fn tween_mut(&mut self, id: Id) -> Option<&mut Tween> {
        match self.slot_mut(id)? {
            Kind::Tween(t) => Some(t),
            _ => None,
        }
    }

    /// How many animations exist (finished ones included until they are removed).
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.kind.is_some()).count()
    }

    /// Whether there are no animations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Advances springs and flings to `now_ms`. Returns true when something is still moving, so
    /// another frame is needed. The first call only sets the clock, and a clock that runs backwards
    /// counts as no time passing.
    pub fn step(&mut self, now_ms: u32) -> bool {
        let dt = self.last_ms.map_or(0, |last| {
            let d = now_ms.wrapping_sub(last);
            if d > u32::MAX / 2 { 0 } else { d }
        });
        if self.last_ms.is_none() || dt > 0 {
            self.last_ms = Some(now_ms);
        }
        let mut moving = false;
        for slot in &mut self.slots {
            match slot.kind.as_mut() {
                Some(Kind::Tween(t)) => moving |= !t.is_done(now_ms),
                Some(Kind::Spring(s)) => moving |= s.step(dt),
                Some(Kind::Decay(d)) => moving |= d.step(dt),
                None => {}
            }
        }
        moving
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Easing, SpringConfig};

    fn tween(start: u32) -> Tween {
        Tween::new(Fx::ZERO, Fx::from_int(100), start, 100, Easing::Linear)
    }

    fn spring_to(target: i32) -> Spring {
        let mut s = Spring::new(SpringConfig::SNAPPY, Fx::ZERO);
        s.set_target(Fx::from_int(target));
        s
    }

    #[test]
    fn an_empty_animator_needs_no_frames() {
        let mut a = Animator::<4>::new();
        assert!(a.is_empty());
        assert!(!a.step(0));
        assert!(!a.step(1000));
    }

    #[test]
    fn a_tween_runs_until_its_end_and_keeps_its_value() {
        let mut a = Animator::<2>::new();
        let id = a.start_tween(tween(1000)).unwrap();
        assert!(a.step(1000));
        assert_eq!(a.value(id, 1050), Some(Fx::from_int(50)));
        assert!(a.step(1099));
        assert!(!a.step(1100), "the last frame needs no further one");
        assert!(a.is_done(id, 1100));
        assert_eq!(a.value(id, 5000), Some(Fx::from_int(100)));
        assert_eq!(a.len(), 1, "finished animations stay until removed");
    }

    #[test]
    fn springs_and_flings_advance_with_the_clock_and_settle() {
        let mut a = Animator::<4>::new();
        let s = a.start_spring(spring_to(100)).unwrap();
        let d = a
            .start_decay(Decay::new(Fx::ZERO, Fx::from_int(1000), Fx::from_int(4)))
            .unwrap();
        let mut now = 0;
        a.step(now);
        let mut frames = 0;
        while a.step({
            now += 16;
            now
        }) {
            frames += 1;
            assert!(frames < 2000);
        }
        assert_eq!(a.value(s, now), Some(Fx::from_int(100)));
        assert!(a.value(d, now).unwrap() > Fx::from_int(240));
        assert!(a.is_done(s, now) && a.is_done(d, now));
    }

    #[test]
    fn the_first_step_only_sets_the_clock() {
        let mut a = Animator::<1>::new();
        let s = a.start_spring(spring_to(100)).unwrap();
        assert!(
            a.step(9_999_999),
            "a spring is moving, but no time has passed"
        );
        assert_eq!(a.value(s, 0), Some(Fx::ZERO));
    }

    #[test]
    fn a_clock_that_stalls_runs_backwards_or_wraps_is_harmless() {
        let mut a = Animator::<1>::new();
        let s = a.start_spring(spring_to(100)).unwrap();
        a.step(1000);
        a.step(1000);
        a.step(500);
        assert_eq!(a.value(s, 0), Some(Fx::ZERO), "no time passed");
        a.step(u32::MAX - 10);
        a.step(5);
        let _ = a.value(s, 5);
    }

    #[test]
    fn a_full_animator_refuses_and_a_removed_slot_is_reused() {
        let mut a = Animator::<2>::new();
        let first = a.start_tween(tween(0)).unwrap();
        let _second = a.start_tween(tween(0)).unwrap();
        assert_eq!(a.start_tween(tween(0)), Err(Full));
        assert!(a.remove(first));
        let third = a.start_tween(tween(0)).unwrap();
        assert_ne!(first, third);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn stale_handles_find_nothing_even_when_the_slot_is_reused() {
        let mut a = Animator::<1>::new();
        let old = a.start_tween(tween(0)).unwrap();
        assert!(a.remove(old));
        assert!(!a.remove(old), "removing twice");
        let new = a.start_spring(spring_to(5)).unwrap();
        assert_eq!(a.value(old, 0), None);
        assert!(a.is_done(old, 0));
        assert!(a.spring_mut(old).is_none());
        assert!(a.value(new, 0).is_some());
        // An out-of-range handle from another animator is just as harmless.
        let mut big = Animator::<8>::new();
        let ids: [Id; 8] = core::array::from_fn(|_| big.start_tween(tween(0)).unwrap());
        assert_eq!(a.value(ids[7], 0), None);
        assert!(!a.remove(ids[7]));
    }

    #[test]
    fn typed_access_matches_the_kind() {
        let mut a = Animator::<3>::new();
        let t = a.start_tween(tween(0)).unwrap();
        let s = a.start_spring(spring_to(10)).unwrap();
        let d = a
            .start_decay(Decay::new(Fx::ZERO, Fx::from_int(500), Fx::from_int(4)))
            .unwrap();
        assert!(a.tween_mut(t).is_some() && a.spring_mut(t).is_none() && a.decay_mut(t).is_none());
        assert!(a.spring_mut(s).is_some() && a.tween_mut(s).is_none());
        assert!(a.decay_mut(d).is_some() && a.spring_mut(d).is_none());
    }

    #[test]
    fn retargeting_a_spring_wakes_it_and_a_touch_stops_a_fling() {
        let mut a = Animator::<2>::new();
        let s = a
            .start_spring(Spring::new(SpringConfig::SNAPPY, Fx::ZERO))
            .unwrap();
        a.step(0);
        assert!(!a.step(16), "a spring at rest needs no frames");
        a.spring_mut(s).unwrap().set_target(Fx::from_int(50));
        assert!(a.step(32), "setting a target wakes it");
        let d = a
            .start_decay(Decay::new(Fx::ZERO, Fx::from_int(1000), Fx::from_int(4)))
            .unwrap();
        a.step(48);
        let p = a.value(d, 48).unwrap();
        a.decay_mut(d).unwrap().stop();
        a.step(64);
        assert_eq!(a.value(d, 64), Some(p));
    }

    #[test]
    fn many_animations_stay_independent() {
        let mut a = Animator::<8>::new();
        let ids: [Id; 8] =
            core::array::from_fn(|i| a.start_spring(spring_to(10 * (i as i32 + 1))).unwrap());
        let mut now = 0;
        a.step(now);
        while a.step({
            now += 16;
            now
        }) {}
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(a.value(*id, now), Some(Fx::from_int(10 * (i as i32 + 1))));
        }
    }
}
