#![doc = include_str!("../README.md")]

use std::fmt;

use fusuma::{Program, Runner};
use fusuma_render::Point;
use fusuma_render_vello_cpu::VelloCpu;
use fusuma_ui::{App, Id, Key, NodeId, Pointer, Role, Semantic};

/// How a widget is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector<'a> {
    /// By the text it shows (its name in the semantic tree).
    Text(&'a str),
    /// By the name the app gave it.
    Id(Id),
    /// By its role, the first one in reading order.
    Role(Role),
}

impl<'a> From<&'a str> for Selector<'a> {
    fn from(text: &'a str) -> Self {
        Self::Text(text)
    }
}

impl From<Id> for Selector<'_> {
    fn from(id: Id) -> Self {
        Self::Id(id)
    }
}

impl From<Role> for Selector<'_> {
    fn from(role: Role) -> Self {
        Self::Role(role)
    }
}

impl fmt::Display for Selector<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(t) => write!(f, "the widget that says {t:?}"),
            Self::Id(i) => write!(f, "the widget called {i:?}"),
            Self::Role(r) => write!(f, "the first {r:?}"),
        }
    }
}

/// A widget that was found, as it is on the screen now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    /// Its handle.
    pub node: NodeId,
    /// What it is.
    pub role: Role,
    /// Its centre, in pixels.
    pub centre: Point,
}

/// Why a step of a test could not be done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Nothing on the screen matches (the message says what, and what is there).
    NotFound(String),
    /// It is on the screen but disabled.
    Disabled(String),
    /// A script line that is not understood.
    BadLine(usize, String),
    /// A check in a script failed.
    Failed(usize, String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(m) | Self::Disabled(m) => f.write_str(m),
            Self::BadLine(n, m) => write!(f, "line {n}: {m}"),
            Self::Failed(n, m) => write!(f, "line {n}: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// How long one frame is, in milliseconds, when time passes.
pub const FRAME_MS: u32 = 16;

/// A program run like a user would: headless, on Vello's CPU renderer, one click or key at a time.
pub struct Sim<P: Program, const N: usize, const K: usize, const L: usize = 64> {
    run: Runner<P, N, K, L>,
    screen: VelloCpu,
    steps: Vec<String>,
    shots: Vec<(String, Vec<u8>)>,
}

impl<P: Program, const N: usize, const K: usize, const L: usize> fmt::Debug for Sim<P, N, K, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sim")
            .field("steps", &self.steps.len())
            .finish_non_exhaustive()
    }
}

impl<P: Program, const N: usize, const K: usize, const L: usize> Sim<P, N, K, L>
where
    P::Msg: fmt::Debug,
{
    /// The program on a `width` x `height` screen, and its first frame.
    ///
    /// # Panics
    /// If the screen cannot be made (a side of zero).
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self::from_runner(Runner::new(width, height), width, height)
    }

    /// The program started from the preset called `name`.
    #[must_use]
    pub fn from_preset(name: &str, width: u32, height: u32) -> Option<Self> {
        Runner::from_preset(name, width, height).map(|r| Self::from_runner(r, width, height))
    }

    fn from_runner(run: Runner<P, N, K, L>, width: u32, height: u32) -> Self {
        let screen = fusuma_render_vello_cpu::new(width, height).expect("a screen of this size");
        let mut s = Self {
            run,
            screen,
            steps: Vec::new(),
            shots: Vec::new(),
        };
        s.frame();
        s
    }

    /// The program's state.
    #[must_use]
    pub const fn state(&self) -> &P::State {
        self.run.state()
    }

    /// The runner, to read.
    #[must_use]
    pub const fn runner(&self) -> &Runner<P, N, K, L> {
        &self.run
    }

    /// The messages the program handled, oldest first, with the time of each in milliseconds.
    #[must_use]
    pub fn messages(&self) -> Vec<(u32, P::Msg)> {
        self.run.log().collect()
    }

    /// Everything the screen says, in reading order.
    #[must_use]
    pub fn texts(&self) -> Vec<String> {
        self.run
            .tree()
            .semantics()
            .filter(|s| !s.name.is_empty())
            .map(|s| s.name.to_owned())
            .collect()
    }

    /// Whether the screen says `text` somewhere (a widget's text, exactly).
    #[must_use]
    pub fn shows(&self, text: &str) -> bool {
        self.texts().iter().any(|t| t == text)
    }

    /// Draws a frame (what changed since the last).
    pub fn frame(&mut self) {
        let _ = self.run.frame(&mut self.screen);
    }

    fn matches(s: &Semantic<'_>, sel: Selector<'_>) -> bool {
        match sel {
            Selector::Text(t) => s.name == t,
            Selector::Id(i) => s.app_id == Some(i),
            Selector::Role(r) => s.role == r,
        }
    }

    /// Finds the widget `sel` names, as it is on the screen now.
    ///
    /// # Errors
    /// [`Error::NotFound`], saying what the screen does show.
    pub fn find<'a>(&self, sel: impl Into<Selector<'a>>) -> Result<Found, Error> {
        let sel = sel.into();
        let tree = self.run.tree();
        let hit = tree.semantics().find(|s| Self::matches(s, sel));
        match hit {
            Some(s) => Ok(Found {
                node: s.id,
                role: s.role,
                centre: Point::new(
                    s.rect.x + i32::try_from(s.rect.w / 2).unwrap_or(0),
                    s.rect.y + i32::try_from(s.rect.h / 2).unwrap_or(0),
                ),
            }),
            None => Err(Error::NotFound(format!(
                "no {sel}; the screen says {:?}",
                self.texts()
            ))),
        }
    }

    fn enabled<'a>(&self, sel: impl Into<Selector<'a>>) -> Result<Found, Error> {
        let sel = sel.into();
        let found = self.find(sel)?;
        if self
            .run
            .tree()
            .semantic(found.node)
            .is_some_and(|s| s.disabled)
        {
            return Err(Error::Disabled(format!("{sel} is disabled")));
        }
        Ok(found)
    }

    /// Lets `ms` milliseconds pass, a frame at a time, drawing each.
    pub fn advance(&mut self, ms: u32) {
        self.steps.push(format!("wait {ms}"));
        self.advance_quiet(ms);
    }

    fn advance_quiet(&mut self, ms: u32) {
        let mut left = ms;
        while left > 0 {
            let dt = left.min(FRAME_MS);
            self.run.tick(dt);
            self.frame();
            left -= dt;
        }
    }

    /// Presses and releases the pointer over `sel`, as a mouse or a finger does.
    ///
    /// # Errors
    /// [`Error`] when it is not there or is disabled.
    pub fn click<'a>(&mut self, sel: impl Into<Selector<'a>>) -> Result<(), Error> {
        let sel = sel.into();
        let at = self.enabled(sel)?.centre;
        self.steps.push(format!("click {}", label(sel)));
        for e in [
            Pointer::Move(at),
            Pointer::Down(at),
            Pointer::Up(at),
            Pointer::Leave,
        ] {
            self.run.pointer(e);
            self.frame();
        }
        self.advance_quiet(FRAME_MS);
        Ok(())
    }

    /// Activates `sel` the way a keyboard or an assistive tool does: the focus goes to it and Enter is pressed. A widget
    /// that cannot take the focus cannot be activated this way.
    ///
    /// # Errors
    /// [`Error`] when it is not there, is disabled, or cannot take the focus.
    pub fn activate<'a>(&mut self, sel: impl Into<Selector<'a>>) -> Result<(), Error> {
        let sel = sel.into();
        let node = self.enabled(sel)?.node;
        if !self.run.tree_mut().focus(node) {
            return Err(Error::Disabled(format!("{sel} cannot take the focus")));
        }
        self.steps.push(format!("activate {}", label(sel)));
        self.run.key(Key::Activate);
        self.frame();
        self.advance_quiet(FRAME_MS);
        Ok(())
    }

    /// Moves the pointer over `sel` (a hover).
    ///
    /// # Errors
    /// [`Error`] when it is not there.
    pub fn hover<'a>(&mut self, sel: impl Into<Selector<'a>>) -> Result<(), Error> {
        let sel = sel.into();
        let at = self.find(sel)?.centre;
        self.steps.push(format!("hover {}", label(sel)));
        self.run.pointer(Pointer::Move(at));
        self.advance_quiet(FRAME_MS);
        Ok(())
    }

    /// Types `text` into the field that has the focus (click one first).
    pub fn type_text(&mut self, text: &str) {
        self.steps.push(format!("type {text}"));
        self.run.text(text);
        self.advance_quiet(FRAME_MS);
    }

    /// Presses a key.
    pub fn key(&mut self, key: Key) {
        self.steps.push(format!("key {}", key_name(key)));
        self.run.key(key);
        self.advance_quiet(FRAME_MS);
    }

    /// Sends `message` as if a widget had, for a step a test cannot reach through the screen.
    pub fn send(&mut self, message: P::Msg) {
        self.run.send(message);
        self.advance_quiet(FRAME_MS);
    }

    /// The screen as a PNG.
    ///
    /// # Panics
    /// If the encoder fails.
    #[must_use]
    pub fn screenshot(&mut self) -> Vec<u8> {
        self.frame();
        fusuma_render_vello_cpu::png(&self.screen).expect("a PNG of the screen")
    }

    /// The pictures a script took with `shot`, by name.
    #[must_use]
    pub fn shots(&self) -> &[(String, Vec<u8>)] {
        &self.shots
    }

    /// What was done so far, as a script that [`script`](Self::script) can run again.
    #[must_use]
    pub fn recorded(&self) -> String {
        self.steps.join("\n")
    }

    /// Runs a script: one command a line, `#` for a comment. `click T`, `activate T`, `hover T`, `type TEXT`,
    /// `key Tab|Enter|Escape|Up|Down|Left|Right|Next|Prev`, `wait MS`, `expect TEXT`, `expect-not TEXT`,
    /// `expect-msg DEBUG` (some handled message prints as this), and `shot NAME`.
    ///
    /// # Errors
    /// [`Error`] naming the line that failed.
    pub fn script(&mut self, text: &str) -> Result<(), Error> {
        for (n, raw) in text.lines().enumerate() {
            let n = n + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (cmd, arg) = line
                .split_once(' ')
                .map_or((line, ""), |(c, a)| (c, a.trim()));
            let bad = |m: &str| Error::BadLine(n, m.to_owned());
            match cmd {
                "click" => self.click(script_selector(arg))?,
                "activate" => self.activate(script_selector(arg))?,
                "hover" => self.hover(script_selector(arg))?,
                "type" => self.type_text(arg),
                "key" => self.key(parse_key(arg).ok_or_else(|| bad(&format!("no key {arg:?}")))?),
                "wait" => self.advance(arg.parse().map_err(|_| bad("wait needs milliseconds"))?),
                "expect" => {
                    if !self.shows(arg) {
                        return Err(Error::Failed(
                            n,
                            format!("expected {arg:?}; the screen says {:?}", self.texts()),
                        ));
                    }
                }
                "expect-not" => {
                    if self.shows(arg) {
                        return Err(Error::Failed(n, format!("did not expect {arg:?}")));
                    }
                }
                "expect-msg" => {
                    if !self.messages().iter().any(|m| format!("{:?}", m.1) == arg) {
                        return Err(Error::Failed(
                            n,
                            format!("no message {arg:?} in {:?}", self.messages()),
                        ));
                    }
                }
                "shot" => {
                    let png = self.screenshot();
                    self.shots.push((arg.to_owned(), png));
                }
                _ => return Err(bad(&format!("no command {cmd:?}"))),
            }
        }
        Ok(())
    }
}

/// A script's `#7` is the widget the app called `Id::new(7)`; anything else is the text a widget shows.
fn script_selector(arg: &str) -> Selector<'_> {
    match arg.strip_prefix('#').and_then(|n| n.parse::<u16>().ok()) {
        Some(n) => Selector::Id(Id::new(n)),
        None => Selector::Text(arg),
    }
}

fn label(sel: Selector<'_>) -> String {
    match sel {
        Selector::Text(t) => t.to_owned(),
        Selector::Id(i) => format!("#{}", i.0),
        Selector::Role(r) => format!("{r:?}"),
    }
}

fn key_name(key: Key) -> &'static str {
    match key {
        Key::Next => "Tab",
        Key::Prev => "Prev",
        Key::Activate => "Enter",
        Key::Cancel => "Escape",
        Key::Up => "Up",
        Key::Down => "Down",
        Key::Left => "Left",
        Key::Right => "Right",
    }
}

fn parse_key(name: &str) -> Option<Key> {
    Some(match name {
        "Tab" | "Next" => Key::Next,
        "Prev" | "BackTab" => Key::Prev,
        "Enter" | "Activate" => Key::Activate,
        "Escape" | "Cancel" => Key::Cancel,
        "Up" => Key::Up,
        "Down" => Key::Down,
        "Left" => Key::Left,
        "Right" => Key::Right,
        _ => return None,
    })
}
