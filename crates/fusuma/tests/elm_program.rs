#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]
//! A whole Elm program run by the runner: a timer subscription, a toast task, a delayed message, a frame subscription
//! for an animation, a message log that replays, and a session that allocates nothing.

use allocation_counter::measure;
use fusuma::{
    Children, Program, Runner, Sub, Task, button, card, column, named, row, show, text_fmt, toast,
};
use fusuma_render::{Point, Rect};
use fusuma_render_integer::{Embedded, Frame};
use fusuma_ui::{App, Id, Pointer, Screen, Shell};

const TOAST: Id = Id::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Msg {
    ToggleClock,
    Tick,
    Save,
    SavedDone,
    Boost,
    Frame(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct State {
    clock_on: bool,
    seconds: u32,
    saves: u32,
    done: u32,
    /// An animated value that runs to 100 while frames are subscribed.
    level: u32,
}

struct App2;

impl Program for App2 {
    type State = State;
    type Msg = Msg;

    fn init() -> State {
        State::default()
    }

    fn update(s: &mut State, m: Msg) -> Task<Msg> {
        match m {
            Msg::ToggleClock => {
                s.clock_on = !s.clock_on;
                Task::none()
            }
            Msg::Tick => {
                s.seconds += 1;
                Task::none()
            }
            Msg::Save => {
                s.saves += 1;
                Task::toast(TOAST, "Saved", 800).and(Task::after(2000, Msg::SavedDone))
            }
            Msg::SavedDone => {
                s.done += 1;
                Task::none()
            }
            Msg::Boost => {
                s.level = 1;
                Task::none()
            }
            Msg::Frame(dt) => {
                s.level = (s.level + dt / 8).min(100);
                if s.level >= 100 {
                    s.level = 0;
                }
                Task::none()
            }
        }
    }

    fn view<'a>(s: &'a State, _: &Screen) -> impl Children<Msg> + 'a {
        (
            card((
                text_fmt(format_args!("{} s", s.seconds)),
                row((
                    button(if s.clock_on { "Stop" } else { "Start" }).on_press(Msg::ToggleClock),
                    button("Save").on_press(Msg::Save),
                    button("Boost").on_press(Msg::Boost),
                ))
                .gap(6),
                text_fmt(format_args!("saved {} done {}", s.saves, s.done)).caption(),
                show(s.level > 0, text_fmt(format_args!("level {}", s.level))),
            ))
            .padding(8)
            .gap(6),
            column((named(TOAST, toast()),)),
        )
    }

    fn subscription(s: &State) -> Sub<Msg> {
        let clock = if s.clock_on {
            Sub::every(1, 1000, Msg::Tick)
        } else {
            Sub::none()
        };
        let anim = if s.level > 0 {
            Sub::frames(2, Msg::Frame)
        } else {
            Sub::none()
        };
        clock.and(anim)
    }
}

type Screen2 = Embedded<Frame<Vec<u16>>>;
type Run = Runner<App2, 32, 16, 64>;

fn screen() -> Screen2 {
    Embedded::new(Frame::new(240, 200, vec![0u16; 240 * 200]).unwrap())
}

fn press(run: &mut Run, label: &str) {
    let id = run
        .tree()
        .semantics()
        .find(|s| s.name == label)
        .map(|s| s.id)
        .unwrap_or_else(|| panic!("no widget called {label}"));
    let r = run.tree().rect(id).unwrap();
    let p = Point::new(r.x + 3, r.y + 3);
    for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
        run.pointer(e);
    }
}

fn started() -> (Run, Screen2) {
    let mut run = Run::new(240, 200);
    let mut r = screen();
    run.frame(&mut r).unwrap();
    (run, r)
}

fn shows(run: &Run, text: &str) -> bool {
    run.tree().semantics().any(|s| s.name == text)
}

#[test]
fn a_program_that_is_idle_sleeps_until_an_input_comes() {
    let (mut run, _) = started();
    assert_eq!(run.deadline(), None);
    assert!(!run.tick(500), "nothing moves, no frame wanted");
    assert!(shows(&run, "0 s"));
    let mut shell = Shell::new(run);
    let mut r = screen();
    shell.frame(0, &mut r).unwrap();
    shell.frame(16, &mut r).unwrap();
    assert_eq!(shell.wake(16), fusuma_ui::Wake::Never);
}

#[test]
fn a_timer_subscription_runs_while_the_state_asks_for_it_and_stops_when_it_does_not() {
    let (mut run, mut r) = started();
    press(&mut run, "Start");
    assert!(run.state().clock_on);
    run.frame(&mut r).unwrap();
    // The press's own fade finishes first; after it only the clock is left.
    run.tick(300);
    run.frame(&mut r).unwrap();
    assert_eq!(
        run.deadline(),
        Some(700),
        "a deadline, not polling: the next tick in 700 ms"
    );
    assert!(!run.tick(100), "no frames while only a clock runs");
    assert_eq!(run.deadline(), Some(600));
    run.tick(600);
    assert_eq!(run.state().seconds, 1);
    run.frame(&mut r).unwrap();
    assert!(shows(&run, "1 s"), "the view follows the state");
    // A long frame carries it over several periods.
    run.tick(3500);
    assert_eq!(run.state().seconds, 4);
    assert_eq!(run.deadline(), Some(500));
    run.frame(&mut r).unwrap();
    // Stopping cancels the recipe: no more ticks, nothing to wait for.
    press(&mut run, "Stop");
    assert_eq!(run.deadline(), None);
    run.tick(10_000);
    assert_eq!(run.state().seconds, 4);
}

#[test]
fn a_task_shows_a_toast_and_sends_a_message_later_and_the_toast_goes_by_itself() {
    let (mut run, mut r) = started();
    press(&mut run, "Save");
    assert_eq!(run.state().saves, 1);
    run.frame(&mut r).unwrap();
    let toast_node = run.tree().find(TOAST).expect("named in the view");
    assert!(run.tree().is_toast_shown(toast_node));
    assert_eq!(run.state().done, 0);
    run.tick(1999);
    assert_eq!(run.state().done, 0, "a millisecond early");
    run.tick(1);
    assert_eq!(run.state().done, 1, "the delayed message arrived once");
    run.tick(5000);
    assert_eq!(run.state().done, 1);
    // The toast faded in, held 800 ms and faded out on its own.
    for _ in 0..200 {
        run.tick(16);
    }
    run.frame(&mut r).unwrap();
    assert!(!run.tree().is_toast_shown(toast_node));
    assert!(shows(&run, "saved 1 done 1"));
}

#[test]
fn a_frame_subscription_keeps_frames_running_only_while_something_moves() {
    let (mut run, mut r) = started();
    assert!(!run.tick(16));
    press(&mut run, "Boost");
    assert_eq!(run.state().level, 1);
    let mut frames = 0;
    while run.tick(16) {
        run.frame(&mut r).unwrap();
        frames += 1;
        assert!(frames < 200, "it must stop");
    }
    assert!(frames > 3, "it ran for a while: {frames}");
    assert_eq!(
        run.state().level,
        0,
        "it ended, so the subscription went and the frames stopped"
    );
    run.frame(&mut r).unwrap();
    assert!(!shows(&run, "level 0"), "the level line is hidden again");
    assert!(!run.tick(16));
}

#[test]
fn the_log_replays_to_the_same_state_with_the_tasks_dropped() {
    let (mut run, mut r) = started();
    press(&mut run, "Start");
    run.tick(2300);
    press(&mut run, "Save");
    run.tick(2100);
    press(&mut run, "Boost");
    for _ in 0..30 {
        run.tick(16);
    }
    run.frame(&mut r).unwrap();
    let messages: Vec<(u32, Msg)> = run.log().collect();
    assert!(messages.len() > 6, "{messages:?}");
    assert!(
        messages.windows(2).all(|w| w[0].0 <= w[1].0),
        "in time order"
    );
    assert_eq!(messages[0].1, Msg::ToggleClock);
    let replayed = Run::replay(State::default(), messages.iter().map(|m| m.1));
    assert_eq!(
        replayed,
        *run.state(),
        "the same state, with no clock and no screen"
    );
    // A runner started from the replayed state shows the same page.
    let mut again = Run::with_state(replayed, 240, 200);
    let mut r2 = screen();
    again.frame(&mut r2).unwrap();
    let names =
        |r: &Run| -> Vec<String> { r.tree().semantics().map(|s| s.name.to_owned()).collect() };
    assert_eq!(names(&again), names(&run));
}

#[test]
fn a_running_program_asks_the_allocator_for_nothing() {
    let (mut run, mut r) = started();
    for _ in 0..3 {
        run.tick(16);
        run.frame(&mut r).unwrap();
    }
    let info = measure(|| {
        for k in 0..40 {
            match k % 8 {
                0 if k % 16 == 0 => press(&mut run, "Start"),
                0 => press(&mut run, "Stop"),
                2 => press(&mut run, "Save"),
                5 => press(&mut run, "Boost"),
                _ => {}
            }
            run.tick(250);
            run.frame(&mut r).unwrap();
        }
    });
    assert_eq!(info.count_total, 0, "{info:?}");
    assert!(run.state().saves > 2);
    let _ = Rect::new(0, 0, 1, 1);
}

mod animated {
    use super::*;
    use fusuma::animate;
    use fusuma_anim::SpringConfig;

    #[derive(Clone, Copy, Debug)]
    enum M2 {
        Toggle,
    }

    struct Panel;

    impl Program for Panel {
        type State = bool;
        type Msg = M2;
        fn init() -> bool {
            false
        }
        fn update(s: &mut bool, m: M2) -> Task<M2> {
            match m {
                M2::Toggle => *s = !*s,
            }
            Task::none()
        }
        fn view<'a>(open: &'a bool, _: &Screen) -> impl Children<M2> + 'a {
            (
                button("toggle").on_press(M2::Toggle),
                animate(
                    card((text_fmt(format_args!("panel")),)).padding(6),
                    (if *open { 0 } else { 120 }, 0),
                    SpringConfig::STIFF,
                ),
            )
        }
    }

    #[test]
    fn a_view_moves_a_node_on_a_spring_without_running_the_view_every_frame() {
        let mut run = Runner::<Panel, 16, 8>::new(240, 200);
        let mut r = screen();
        run.frame(&mut r).unwrap();
        let panel = |run: &Runner<Panel, 16, 8>| {
            let id = run
                .tree()
                .semantics()
                .find(|s| s.name == "panel")
                .unwrap()
                .id;
            run.tree().rect(id).unwrap().x
        };
        let away = panel(&run);
        // A node that was just built is at its target: nothing moves at the start.
        assert!(!run.tick(16));
        let id = run
            .tree()
            .semantics()
            .find(|s| s.name == "toggle")
            .unwrap()
            .id;
        let rect = run.tree().rect(id).unwrap();
        let p = Point::new(rect.x + 3, rect.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            run.pointer(e);
        }
        run.frame(&mut r).unwrap();
        let mut frames = 0;
        let mut xs = vec![panel(&run)];
        while run.tick(16) {
            run.frame(&mut r).unwrap();
            xs.push(panel(&run));
            frames += 1;
            assert!(frames < 300);
        }
        run.frame(&mut r).unwrap();
        assert!(frames > 3, "it travelled over frames: {xs:?}");
        assert_eq!(panel(&run), away - 120, "and ended on its target");
        assert!(
            xs.windows(2).all(|w| w[1] <= w[0] + 1),
            "towards it, with no jumps back: {xs:?}"
        );
        assert_eq!(run.state(), &true);
        // The view ran for the one message, not once a frame: the log holds the one message.
        assert_eq!(
            run.log().count(),
            0,
            "this runner keeps no log, and nothing else was sent"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sm {
    Screen(u32),
}

struct Watcher;

impl Program for Watcher {
    type State = u32;
    type Msg = Sm;

    fn init() -> u32 {
        0
    }

    fn update(s: &mut u32, m: Sm) -> Task<Sm> {
        let Sm::Screen(n) = m;
        *s += n;
        Task::none()
    }

    fn view<'a>(s: &'a u32, _: &Screen) -> impl Children<Sm> + 'a {
        (text_fmt(format_args!("{s}")),)
    }

    fn on_screen(screen: &Screen) -> Option<Sm> {
        // 1 for a phone or smaller, 10 for a tablet or bigger: the form of the layout the view should use.
        Some(Sm::Screen(if screen.class().is_compact() { 1 } else { 10 }))
    }
}

#[test]
fn a_program_hears_about_a_change_of_screen_that_a_layout_cares_about_and_only_that() {
    let mut r = Runner::<Watcher, 16, 16, 0>::new(800, 600);
    assert_eq!(
        *r.state(),
        0,
        "nothing yet: the program started with this screen"
    );
    // A resize within the same class, size class and orientation is not news (800 to 830 is a tablet, medium, all the way).
    App::resized(&mut r, 820, 610, 100);
    App::resized(&mut r, 830, 700, 100);
    assert_eq!(*r.state(), 0);
    // Crossing the size class (840 and up is expanded) is.
    App::resized(&mut r, 900, 700, 100);
    assert_eq!(*r.state(), 10);
    // A window dragged narrow crosses into the compact classes: one message, and still compact is not another.
    App::resized(&mut r, 400, 700, 100);
    assert_eq!(*r.state(), 11);
    App::resized(&mut r, 390, 700, 100);
    assert_eq!(*r.state(), 11, "still compact");
    // A rotation to landscape is news even though the class by width is no longer compact: this one is a tablet again.
    App::resized(&mut r, 700, 390, 100);
    assert_eq!(*r.state(), 21);
    // A scale change that moves the logical width across a limit is too: 700 px at 2x is 350 logical.
    App::resized(&mut r, 700, 390, 200);
    assert_eq!(*r.state(), 22);
    // What the device is: a declared form factor, sent by a platform, is news, and the same again is not.
    App::screen(
        &mut r,
        Screen::new(700, 390).with_form_factor(fusuma_ui::FormFactor::Desktop),
    );
    let after = *r.state();
    App::screen(
        &mut r,
        Screen::new(700, 390).with_form_factor(fusuma_ui::FormFactor::Desktop),
    );
    assert_eq!(*r.state(), after, "the same device again says nothing");
}
