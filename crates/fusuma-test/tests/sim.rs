#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]
//! The simulator driving a small program: find, click, activate, type, time, messages, scripts, screenshots.

use fusuma::{
    Children, Preset, Program, Sub, Task, button, card, column, named, row, show, text, text_field,
    text_fmt, toast,
};
use fusuma_test::{Error, Sim};
use fusuma_ui::{Id, Key, Role, Screen};

const TOAST: Id = Id::new(1);
const FIELD: Id = Id::new(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Msg {
    Inc,
    Dec,
    Reset,
    Typed(u8),
    Save,
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    n: i32,
    letters: u8,
    ticks: u32,
    running: bool,
}

struct Counter;

impl Program for Counter {
    type State = State;
    type Msg = Msg;

    fn init() -> State {
        State {
            n: 0,
            letters: 0,
            ticks: 0,
            running: false,
        }
    }

    fn presets() -> &'static [Preset<State>] {
        &[Preset {
            name: "high",
            state: || State {
                n: 90,
                letters: 0,
                ticks: 0,
                running: false,
            },
        }]
    }

    fn update(s: &mut State, m: Msg) -> Task<Msg> {
        match m {
            Msg::Inc => s.n += 1,
            Msg::Dec => s.n -= 1,
            Msg::Reset => s.n = 0,
            Msg::Typed(n) => s.letters = n,
            Msg::Save => {
                s.running = true;
                return Task::toast(TOAST, "Saved", 300);
            }
            Msg::Tick => s.ticks += 1,
        }
        Task::none()
    }

    fn view<'a>(s: &'a State, _: &Screen) -> impl Children<Msg> + 'a {
        (
            card((
                text_fmt(format_args!("Count: {}", s.n)),
                row((
                    button("-").on_press(Msg::Dec),
                    button("+").on_press(Msg::Inc),
                    button("Reset").enabled(s.n != 0).on_press(Msg::Reset),
                ))
                .gap(6),
                named(
                    FIELD,
                    text_field("").placeholder("name").on_input(|t| {
                        Msg::Typed(u8::try_from(t.chars().count()).unwrap_or(u8::MAX))
                    }),
                ),
                button("Save").on_press(Msg::Save),
                show(s.n > 99, text("Big number")),
                text_fmt(format_args!("typed {} ticks {}", s.letters, s.ticks)).caption(),
            ))
            .padding(8)
            .gap(6),
            column((named(TOAST, toast()),)),
        )
    }

    fn subscription(s: &State) -> Sub<Msg> {
        if s.running {
            Sub::every(1, 100, Msg::Tick)
        } else {
            Sub::none()
        }
    }
}

type S = Sim<Counter, 32, 16, 64>;

fn sim() -> S {
    Sim::new(260, 240)
}

#[test]
fn widgets_are_found_by_text_by_name_and_by_role_and_a_miss_says_what_is_there() {
    let s = sim();
    assert!(s.find("Save").is_ok());
    assert_eq!(s.find(FIELD).unwrap().role, Role::TextField);
    assert!(matches!(s.find(Role::Progress), Err(Error::NotFound(_))));
    let Err(Error::NotFound(m)) = s.find("Nope") else {
        panic!()
    };
    assert!(m.contains("Nope") && m.contains("Count: 0"), "{m}");
    assert!(s.shows("Count: 0") && !s.shows("Big number"));
}

#[test]
fn clicks_become_messages_the_state_changes_and_the_screen_follows() {
    let mut s = sim();
    s.click("+").unwrap();
    s.click("+").unwrap();
    s.click("-").unwrap();
    assert_eq!(s.state().n, 1);
    assert!(s.shows("Count: 1"));
    let msgs: Vec<Msg> = s.messages().iter().map(|m| m.1).collect();
    assert_eq!(msgs, [Msg::Inc, Msg::Inc, Msg::Dec]);
    assert!(s.messages().windows(2).all(|w| w[0].0 <= w[1].0));
}

#[test]
fn a_disabled_widget_cannot_be_clicked_and_says_so() {
    let mut s = sim();
    let Err(Error::Disabled(m)) = s.click("Reset") else {
        panic!("Reset is disabled at zero")
    };
    assert!(m.contains("Reset"), "{m}");
    s.click("+").unwrap();
    s.click("Reset").unwrap();
    assert_eq!(s.state().n, 0);
}

#[test]
fn activating_goes_through_the_focus_and_enter_like_a_keyboard_or_an_assistive_tool() {
    let mut s = sim();
    s.activate("+").unwrap();
    s.activate("+").unwrap();
    assert_eq!(s.state().n, 2);
    s.key(Key::Next);
    assert_eq!(s.state().n, 2, "a key that moves the focus is no message");
    assert_eq!(s.messages().len(), 2);
}

#[test]
fn typing_goes_to_the_focused_field_and_the_program_hears_every_change() {
    let mut s = sim();
    s.click(FIELD).unwrap();
    s.type_text("abc");
    assert_eq!(s.state().letters, 3);
    assert!(s.shows("typed 3 ticks 0"));
}

#[test]
fn time_passes_in_frames_and_subscriptions_and_toasts_run_on_it() {
    let mut s = sim();
    s.advance(1000);
    assert_eq!(s.state().ticks, 0, "nothing subscribed yet");
    s.click("Save").unwrap();
    let toast = s.runner().tree().find(TOAST).unwrap();
    assert!(s.runner().tree().is_toast_shown(toast));
    s.advance(1000);
    assert!(
        (9..=11).contains(&s.state().ticks),
        "a tick every 100 ms: {}",
        s.state().ticks
    );
    assert!(
        !s.runner().tree().is_toast_shown(toast),
        "the toast went by itself"
    );
}

#[test]
fn a_preset_starts_from_a_state_and_a_missing_one_is_none() {
    let mut s = S::from_preset("high", 260, 240).unwrap();
    assert_eq!(s.state().n, 90);
    for _ in 0..10 {
        s.click("+").unwrap();
    }
    assert!(s.shows("Big number"), "{:?}", s.texts());
    assert!(S::from_preset("nope", 260, 240).is_none());
}

#[test]
fn a_script_runs_checks_and_what_was_done_can_be_recorded_and_run_again() {
    let mut a = sim();
    a.script(
        "# count up twice, name it, save\n\
         click +\nclick +\nexpect Count: 2\nclick #2\ntype hello\nexpect typed 5 ticks 0\n\
         click Save\nwait 500\nexpect-not Count: 0\nexpect-msg Inc\nshot after",
    )
    .unwrap();
    assert_eq!(a.state().n, 2);
    assert_eq!(a.shots().len(), 1);
    assert!(
        a.shots()[0].1.starts_with(&[0x89, b'P', b'N', b'G']),
        "a PNG"
    );
    let recorded = a.recorded();
    assert!(recorded.lines().next() == Some("click +"), "{recorded}");
    // The recording runs again on a fresh program and gives the same state and the same screen.
    let mut b = sim();
    b.script(&recorded).unwrap();
    assert_eq!(b.state(), a.state());
    assert_eq!(b.texts(), a.texts());
    assert!(b.screenshot() == a.screenshot(), "the same picture");
}

#[test]
fn a_script_names_the_line_that_failed() {
    let mut s = sim();
    let e = s.script("click +\nexpect Count: 5").unwrap_err();
    assert!(matches!(e, Error::Failed(2, _)), "{e}");
    assert!(e.to_string().starts_with("line 2"));
    assert!(matches!(s.script("dance"), Err(Error::BadLine(1, _))));
    assert!(matches!(s.script("key Banana"), Err(Error::BadLine(1, _))));
    assert!(matches!(s.script("wait soon"), Err(Error::BadLine(1, _))));
    assert!(matches!(s.script("click Nowhere"), Err(Error::NotFound(_))));
    assert!(s.script("# only a comment\n\n").is_ok());
}

#[test]
fn a_screenshot_changes_when_the_screen_does() {
    let mut s = sim();
    let before = s.screenshot();
    assert!(
        s.screenshot() == before,
        "the same screen, the same picture"
    );
    s.click("+").unwrap();
    assert!(s.screenshot() != before);
}
