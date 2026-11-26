#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

use fusuma_render::Rect;
use fusuma_ui::Key;

/// How far a host has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The system's values are translated and tested; no window, surface or event loop runs an app yet.
    Scaffold,
    /// An app runs on a device or an emulator.
    Running,
}

/// What a host is, for a tool that lists the platforms and for a test that keeps the claims honest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostInfo {
    /// Its name.
    pub name: &'static str,
    /// The Rust targets it builds for.
    pub targets: &'static [&'static str],
    /// How it gets a surface to draw on.
    pub surface: &'static str,
    /// How text reaches a text field.
    pub text_input: &'static str,
    /// How far it has got.
    pub status: Status,
}

/// What the system tells the app about its own life and its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The app came to the foreground and can be seen and used.
    Resumed,
    /// The app is leaving the foreground: stop drawing and animating.
    Paused,
    /// The window exists and has a surface to draw on (Android `surfaceCreated`, an iOS view with a layer).
    SurfaceCreated,
    /// The surface is gone (Android `surfaceDestroyed`): it must not be drawn to, and the GPU resources tied to it are lost.
    SurfaceDestroyed,
    /// The system wants memory back.
    LowMemory,
    /// The app is about to be killed.
    Destroyed,
}

/// What a host does after an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// Make the GPU surface and the renderer on it.
    pub make_surface: bool,
    /// Drop the renderer and the surface.
    pub drop_surface: bool,
    /// Draw a frame now (the app is visible again).
    pub redraw: bool,
    /// Give back memory: caches, glyph atlases, the spare frame.
    pub trim_memory: bool,
    /// Stop: the app is being destroyed.
    pub quit: bool,
}

/// Where an app is in its life, from the events the system sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Session {
    foreground: bool,
    surface: bool,
    rendering: bool,
    gone: bool,
}

impl Session {
    /// A session before anything has happened: no surface, in the background.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            foreground: false,
            surface: false,
            rendering: false,
            gone: false,
        }
    }

    /// Whether frames should be drawn: the app is in the foreground and has a surface.
    #[must_use]
    pub const fn should_draw(&self) -> bool {
        self.foreground && self.surface && !self.gone
    }

    /// Whether time should pass for the app: in the foreground. A paused app does not animate or poll.
    #[must_use]
    pub const fn should_run(&self) -> bool {
        self.foreground && !self.gone
    }

    /// Whether a renderer is live (made and not yet dropped).
    #[must_use]
    pub const fn has_renderer(&self) -> bool {
        self.rendering
    }

    /// Takes in an event and says what to do. A surface is made when the window has one and the app is in the
    /// foreground, dropped as soon as the window loses it, and never made twice.
    pub fn on(&mut self, event: Event) -> Step {
        let mut step = Step::default();
        match event {
            Event::Resumed => self.foreground = true,
            Event::Paused => self.foreground = false,
            Event::SurfaceCreated => self.surface = true,
            Event::SurfaceDestroyed => self.surface = false,
            Event::LowMemory => step.trim_memory = true,
            Event::Destroyed => {
                self.gone = true;
                step.quit = true;
            }
        }
        // A surface that is gone must go, always: the system has already taken the window.
        if self.rendering && (!self.surface || self.gone) {
            self.rendering = false;
            step.drop_surface = true;
        }
        // A surface is made when there is a window and the app can be seen.
        if !self.rendering && self.should_draw() {
            self.rendering = true;
            step.make_surface = true;
            step.redraw = true;
        } else if matches!(event, Event::Resumed) && self.should_draw() {
            step.redraw = true;
        }
        step
    }
}

/// The screen as the system describes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Display {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Display scale in percent (100 is 160 dpi on Android, 1x on iOS: a 3x phone says 300).
    pub scale_percent: u32,
    /// Pixels the status bar, notch or camera cut-out takes at the top.
    pub inset_top: u32,
    /// Pixels the gesture bar or home indicator takes at the bottom.
    pub inset_bottom: u32,
    /// Pixels at the left (a landscape notch).
    pub inset_left: u32,
    /// Pixels at the right.
    pub inset_right: u32,
    /// Height of the soft keyboard while it is up, else 0.
    pub keyboard: u32,
}

impl Display {
    /// The rectangle an app may lay out in: the screen less the system insets, with the soft keyboard counted as part of the
    /// bottom inset when it is taller than the gesture bar. Never smaller than nothing, never outside the screen.
    #[must_use]
    pub fn content(&self) -> Rect {
        let bottom = self.inset_bottom.max(self.keyboard);
        let x = self.inset_left.min(self.width);
        let y = self.inset_top.min(self.height);
        let w = self
            .width
            .saturating_sub(self.inset_left.saturating_add(self.inset_right));
        let h = self
            .height
            .saturating_sub(self.inset_top.saturating_add(bottom));
        Rect::new(
            i32::try_from(x).unwrap_or(i32::MAX),
            i32::try_from(y).unwrap_or(i32::MAX),
            w,
            h,
        )
    }

    /// The scale, held between 50 and 800 percent (a host that reports 0 gets 100).
    #[must_use]
    pub const fn scale(&self) -> u32 {
        if self.scale_percent == 0 {
            100
        } else if self.scale_percent < 50 {
            50
        } else if self.scale_percent > 800 {
            800
        } else {
            self.scale_percent
        }
    }

    /// The width in logical pixels, which is what picks the layout (a phone, a tablet).
    #[must_use]
    pub const fn logical_width(&self) -> u32 {
        let w = self.width as u64 * 100 / self.scale() as u64;
        if w > u32::MAX as u64 {
            u32::MAX
        } else {
            w as u32
        }
    }
}

/// What to do with the soft keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardChange {
    /// Nothing: it is as it should be.
    None,
    /// Show it: a text field took the focus.
    Show,
    /// Hide it: the text field lost the focus.
    Hide,
}

/// Remembers whether the soft keyboard is up, so a host is told to change it once and not on every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoftKeyboard {
    up: bool,
}

impl SoftKeyboard {
    /// A keyboard that is down.
    #[must_use]
    pub const fn new() -> Self {
        Self { up: false }
    }

    /// Whether it is up, as far as this knows.
    #[must_use]
    pub const fn is_up(&self) -> bool {
        self.up
    }

    /// What the host must do now that a text field does or does not have the focus (`App::wants_text`).
    pub fn wanted(&mut self, wants_text: bool) -> KeyboardChange {
        match (wants_text, self.up) {
            (true, false) => {
                self.up = true;
                KeyboardChange::Show
            }
            (false, true) => {
                self.up = false;
                KeyboardChange::Hide
            }
            _ => KeyboardChange::None,
        }
    }

    /// The system hid the keyboard itself (the user swiped it away): forget that it was up, so the next focus shows it again.
    pub fn dismissed_by_system(&mut self) {
        self.up = false;
    }
}

/// A coordinate the system reports as a float, as whole pixels: rounded to the nearest (halves go up), `NaN` and the
/// infinities are 0, and the value is held to a billion so nothing downstream can overflow.
#[must_use]
pub fn pixel(v: f32) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    let f = v.clamp(-1.0e9, 1.0e9) + 0.5;
    let t = f as i32;
    // `as` truncates towards zero: take the floor.
    if (t as f32) > f { t - 1 } else { t }
}

/// The key a back gesture or button stands for: cancel, which closes a dialog or a menu first.
#[must_use]
pub const fn back() -> Key {
    Key::Cancel
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(events: &[Event]) -> (Session, Vec<Step>) {
        let mut s = Session::new();
        let steps = events.iter().map(|e| s.on(*e)).collect();
        (s, steps)
    }

    #[test]
    fn the_surface_is_made_when_the_window_has_one_and_the_app_is_in_front_and_not_before() {
        let (s, steps) = run(&[Event::SurfaceCreated, Event::Resumed]);
        assert!(
            !steps[0].make_surface,
            "a window while in the background: not yet"
        );
        assert!(steps[1].make_surface && steps[1].redraw);
        assert!(s.should_draw() && s.has_renderer());
        let (_, steps) = run(&[Event::Resumed, Event::SurfaceCreated]);
        assert!(
            !steps[0].make_surface && steps[1].make_surface,
            "either order"
        );
    }

    #[test]
    fn a_lost_surface_is_dropped_at_once_and_a_new_one_is_made_on_return() {
        let (mut s, _) = run(&[Event::Resumed, Event::SurfaceCreated]);
        let gone = s.on(Event::SurfaceDestroyed);
        assert!(gone.drop_surface && !gone.make_surface);
        assert!(
            !s.should_draw() && !s.has_renderer(),
            "nothing may draw to a dead surface"
        );
        let back = s.on(Event::SurfaceCreated);
        assert!(back.make_surface && back.redraw);
        // Going to the background and back keeps the surface, and redraws.
        let paused = s.on(Event::Paused);
        assert_eq!(
            paused,
            Step::default(),
            "pausing drops nothing the system did not take"
        );
        assert!(!s.should_draw() && !s.should_run());
        let resumed = s.on(Event::Resumed);
        assert!(
            resumed.redraw && !resumed.make_surface,
            "the renderer is still live: only redraw"
        );
    }

    #[test]
    fn a_surface_is_never_made_twice_and_events_that_change_nothing_do_nothing() {
        let (mut s, _) = run(&[Event::Resumed, Event::SurfaceCreated]);
        assert_eq!(s.on(Event::SurfaceCreated), Step::default());
        assert!(!s.on(Event::Resumed).make_surface);
        assert!(s.on(Event::SurfaceDestroyed).drop_surface);
        assert_eq!(
            s.on(Event::SurfaceDestroyed),
            Step::default(),
            "already gone"
        );
        assert_eq!(s.on(Event::Paused), Step::default());
        assert_eq!(s.on(Event::Paused), Step::default());
    }

    #[test]
    fn low_memory_trims_and_destroyed_quits_and_drops_and_nothing_comes_back() {
        let (mut s, _) = run(&[Event::Resumed, Event::SurfaceCreated]);
        assert!(s.on(Event::LowMemory).trim_memory);
        assert!(s.should_draw(), "trimming does not stop the app");
        let end = s.on(Event::Destroyed);
        assert!(end.quit && end.drop_surface);
        for e in [Event::Resumed, Event::SurfaceCreated, Event::Paused] {
            let step = s.on(e);
            assert!(
                !step.make_surface && !step.redraw,
                "a destroyed app stays down: {e:?}"
            );
        }
        assert!(!s.should_run() && !s.should_draw());
    }

    #[test]
    fn a_random_run_of_events_never_draws_without_a_surface_or_in_the_background() {
        let all = [
            Event::Resumed,
            Event::Paused,
            Event::SurfaceCreated,
            Event::SurfaceDestroyed,
            Event::LowMemory,
        ];
        let mut seed = 5u64;
        for _ in 0..200 {
            let mut s = Session::new();
            let (mut window, mut front) = (false, false);
            for _ in 0..40 {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let e = all[((seed >> 33) as usize) % all.len()];
                match e {
                    Event::SurfaceCreated => window = true,
                    Event::SurfaceDestroyed => window = false,
                    Event::Resumed => front = true,
                    Event::Paused => front = false,
                    _ => {}
                }
                let step = s.on(e);
                assert_eq!(s.should_draw(), window && front, "{e:?}");
                assert_eq!(
                    s.has_renderer(),
                    window && front || (s.has_renderer() && window),
                    "a renderer needs a window"
                );
                if !window {
                    assert!(!s.has_renderer(), "no window, no renderer: {e:?}");
                }
                assert!(!(step.make_surface && step.drop_surface));
            }
        }
    }

    #[test]
    fn the_content_rectangle_is_the_screen_less_the_insets_and_the_keyboard_counts_as_a_bottom_inset()
     {
        let d = Display {
            width: 1080,
            height: 2400,
            scale_percent: 300,
            inset_top: 120,
            inset_bottom: 60,
            inset_left: 0,
            inset_right: 0,
            keyboard: 0,
        };
        assert_eq!(d.content(), Rect::new(0, 120, 1080, 2220));
        // The keyboard is taller than the gesture bar: it replaces it, it does not add to it.
        let up = Display { keyboard: 900, ..d };
        assert_eq!(up.content(), Rect::new(0, 120, 1080, 1380));
        // A short keyboard under the gesture bar changes nothing.
        assert_eq!(Display { keyboard: 40, ..d }.content(), d.content());
        // Landscape with a notch on one side.
        let wide = Display {
            width: 2400,
            height: 1080,
            inset_left: 120,
            inset_right: 0,
            inset_top: 0,
            inset_bottom: 40,
            ..d
        };
        assert_eq!(wide.content(), Rect::new(120, 0, 2280, 1040));
    }

    #[test]
    fn insets_bigger_than_the_screen_leave_nothing_and_never_wrap_round() {
        let d = Display {
            width: 100,
            height: 100,
            scale_percent: 100,
            inset_top: 400,
            inset_bottom: 400,
            inset_left: 400,
            inset_right: 400,
            keyboard: u32::MAX,
        };
        let c = d.content();
        assert!(c.w == 0 && c.h == 0 && c.x <= 100 && c.y <= 100, "{c:?}");
        assert_eq!(Display::default().content(), Rect::new(0, 0, 0, 0));
    }

    #[test]
    fn the_scale_is_held_to_a_sane_range_and_picks_the_layout_by_logical_width() {
        let phone = Display {
            width: 1080,
            scale_percent: 300,
            ..Display::default()
        };
        assert_eq!(
            phone.logical_width(),
            360,
            "a 3x phone is 360 logical px wide"
        );
        assert_eq!(
            Display {
                scale_percent: 0,
                ..phone
            }
            .scale(),
            100
        );
        assert_eq!(
            Display {
                scale_percent: 5,
                ..phone
            }
            .scale(),
            50
        );
        assert_eq!(
            Display {
                scale_percent: 9000,
                ..phone
            }
            .scale(),
            800
        );
        assert_eq!(
            Display {
                width: u32::MAX,
                scale_percent: 50,
                ..Display::default()
            }
            .logical_width(),
            u32::MAX
        );
        // A known gap, written down so it is not forgotten: the toolkit's classes go by logical width alone, and a 360 dp phone is
        // narrower than the watch limit (480), so it would get the watch's tile layout. A phone and a watch differ in physical
        // size, which `Display` does not know yet (see the roadmap: size classes need the dpi).
        use fusuma_ui::Class;
        assert_eq!(Class::of(phone.logical_width()), Class::Watch);
        assert_eq!(
            Class::of(
                Display {
                    width: 1200,
                    scale_percent: 200,
                    ..Display::default()
                }
                .logical_width()
            ),
            Class::Phone,
            "600 logical: a phone in landscape or a small tablet"
        );
    }

    #[test]
    fn the_keyboard_is_told_to_change_once_and_comes_back_after_the_system_hides_it() {
        let mut k = SoftKeyboard::new();
        assert_eq!(k.wanted(false), KeyboardChange::None);
        assert_eq!(k.wanted(true), KeyboardChange::Show);
        assert_eq!(k.wanted(true), KeyboardChange::None, "not on every frame");
        assert!(k.is_up());
        k.dismissed_by_system();
        assert_eq!(
            k.wanted(true),
            KeyboardChange::Show,
            "the field still has the focus: show it again"
        );
        assert_eq!(k.wanted(false), KeyboardChange::Hide);
        assert_eq!(k.wanted(false), KeyboardChange::None);
    }

    #[test]
    fn coordinates_are_rounded_to_the_nearest_pixel_and_never_overflow() {
        for (v, want) in [
            (0.0, 0),
            (0.49, 0),
            (0.5, 1),
            (10.4, 10),
            (20.6, 21),
            (-0.4, 0),
            (-0.5, 0),
            (-0.6, -1),
            (-10.5, -10),
            (-10.6, -11),
        ] {
            assert_eq!(pixel(v), want, "{v}");
        }
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(pixel(v), 0);
        }
        assert_eq!(pixel(f32::MAX), 1_000_000_000);
        assert_eq!(pixel(f32::MIN), -1_000_000_000);
    }

    #[test]
    fn back_is_cancel() {
        assert_eq!(back(), Key::Cancel);
    }
}
