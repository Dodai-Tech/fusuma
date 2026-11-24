//! The bytes a terminal sends, turned into keys one byte at a time: for a serial console with no terminal library.

/// A key a terminal sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKey {
    /// A printable character (a letter, a digit, a space, any Unicode character).
    Char(char),
    /// Enter or Return.
    Enter,
    /// Tab.
    Tab,
    /// Shift+Tab.
    BackTab,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Escape on its own (see [`Decoder::flush`]).
    Escape,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
}

/// How many bytes of an unfinished sequence are kept; longer ones are dropped.
const PENDING: usize = 8;

/// Turns bytes into [`CellKey`]s. Escape is also how every special key's sequence starts, so a lone Escape is only
/// known when nothing follows: call [`flush`](Self::flush) when input has paused (a few tens of milliseconds).
/// Unknown sequences and stray bytes are dropped, never turned into text.
#[derive(Clone, Copy, Debug, Default)]
pub struct Decoder {
    buf: [u8; PENDING],
    len: usize,
    /// An escape sequence too long to keep is being skipped to its final byte, so its parameters never turn into
    /// text.
    skip: bool,
}

impl Decoder {
    /// A decoder with nothing pending.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [0; PENDING],
            len: 0,
            skip: false,
        }
    }

    /// Feeds one byte; the key it completes, if any.
    pub fn push(&mut self, byte: u8) -> Option<CellKey> {
        if self.skip {
            self.skip = !(0x40..=0x7e).contains(&byte);
            return None;
        }
        if self.len == 0 {
            return match byte {
                b'\r' | b'\n' => Some(CellKey::Enter),
                b'\t' => Some(CellKey::Tab),
                0x7f | 0x08 => Some(CellKey::Backspace),
                0x20..=0x7e => Some(CellKey::Char(char::from(byte))),
                0x1b | 0xc2..=0xf4 => {
                    self.keep(byte);
                    None
                }
                // Other control characters and stray continuation bytes.
                _ => None,
            };
        }
        // A second Escape: the first one was a key of its own.
        if byte == 0x1b && self.buf[0] == 0x1b {
            self.len = 1;
            return Some(CellKey::Escape);
        }
        // A byte that cannot continue a character ends it unfinished: it is dropped and the byte read on its own.
        if self.buf[0] != 0x1b && byte & 0xc0 != 0x80 {
            self.len = 0;
            return self.push(byte);
        }
        if !self.keep(byte) {
            return None;
        }
        let pending = &self.buf[..self.len];
        let done = if pending[0] == 0x1b {
            escape(pending)
        } else {
            utf8(pending)
        };
        match done {
            Step::More => None,
            Step::Key(k) => {
                self.len = 0;
                Some(k)
            }
            Step::Drop => {
                self.len = 0;
                None
            }
        }
    }

    /// Input has paused: a lone Escape that was waiting is the Escape key; anything else unfinished is dropped.
    pub fn flush(&mut self) -> Option<CellKey> {
        let lone = self.len == 1 && self.buf[0] == 0x1b;
        self.len = 0;
        self.skip = false;
        lone.then_some(CellKey::Escape)
    }

    /// Keeps `byte` as part of an unfinished sequence. False when the sequence is too long to keep: it is dropped,
    /// and an escape sequence is skipped to its end.
    fn keep(&mut self, byte: u8) -> bool {
        if self.len == PENDING {
            self.skip = self.buf[0] == 0x1b && !(0x40..=0x7e).contains(&byte);
            self.len = 0;
            return false;
        }
        self.buf[self.len] = byte;
        self.len += 1;
        true
    }
}

/// Where an unfinished sequence stands.
enum Step {
    More,
    Key(CellKey),
    Drop,
}

/// An escape sequence: `ESC [ ...` (CSI) or `ESC O x` (SS3).
fn escape(s: &[u8]) -> Step {
    match s {
        [0x1b] | [0x1b, b'[' | b'O'] => Step::More,
        [0x1b, b'O', last] => arrow(*last).map_or(Step::Drop, Step::Key),
        [0x1b, b'[', rest @ ..] => {
            let Some((&last, params)) = rest.split_last() else {
                return Step::More;
            };
            match last {
                // Parameters and separators: the sequence goes on.
                b'0'..=b'9' | b';' => Step::More,
                b'~' => match params {
                    b"1" | b"7" => Step::Key(CellKey::Home),
                    b"3" => Step::Key(CellKey::Delete),
                    b"4" | b"8" => Step::Key(CellKey::End),
                    _ => Step::Drop,
                },
                b'Z' => Step::Key(CellKey::BackTab),
                // A final byte: an arrow, Home or End (modifiers such as `1;5` are ignored), or something unknown.
                0x40..=0x7e => arrow(last).map_or(Step::Drop, Step::Key),
                _ => Step::Drop,
            }
        }
        // Escape and then something that starts no sequence (Alt and a key): not handled yet.
        _ => Step::Drop,
    }
}

fn arrow(last: u8) -> Option<CellKey> {
    match last {
        b'A' => Some(CellKey::Up),
        b'B' => Some(CellKey::Down),
        b'C' => Some(CellKey::Right),
        b'D' => Some(CellKey::Left),
        b'H' => Some(CellKey::Home),
        b'F' => Some(CellKey::End),
        _ => None,
    }
}

/// A UTF-8 character of two to four bytes.
fn utf8(s: &[u8]) -> Step {
    let want = match s[0] {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    };
    if s.len() < want {
        return if s[1..].iter().all(|b| b & 0xc0 == 0x80) {
            Step::More
        } else {
            Step::Drop
        };
    }
    match core::str::from_utf8(s).ok().and_then(|t| t.chars().next()) {
        Some(c) if !c.is_control() => Step::Key(CellKey::Char(c)),
        _ => Step::Drop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(bytes: &[u8]) -> Vec<CellKey> {
        let mut d = Decoder::new();
        let mut out: Vec<CellKey> = bytes.iter().filter_map(|&b| d.push(b)).collect();
        out.extend(d.flush());
        out
    }

    #[test]
    fn plain_keys_and_text() {
        use CellKey::*;
        assert_eq!(
            keys(b"a Z\r\t\x7f\x08\n"),
            [
                Char('a'),
                Char(' '),
                Char('Z'),
                Enter,
                Tab,
                Backspace,
                Backspace,
                Enter
            ]
        );
        assert_eq!(
            keys(b"\x01\x02\x80"),
            [],
            "stray control and continuation bytes are dropped"
        );
    }

    #[test]
    fn arrows_and_editing_keys_in_both_forms() {
        use CellKey::*;
        assert_eq!(keys(b"\x1b[A\x1b[B\x1b[C\x1b[D"), [Up, Down, Right, Left]);
        assert_eq!(keys(b"\x1bOA\x1bOH\x1bOF"), [Up, Home, End]);
        assert_eq!(
            keys(b"\x1b[H\x1b[F\x1b[1~\x1b[4~\x1b[7~\x1b[8~"),
            [Home, End, Home, End, Home, End]
        );
        assert_eq!(keys(b"\x1b[3~\x1b[Z"), [Delete, BackTab]);
        assert_eq!(
            keys(b"\x1b[1;5C"),
            [Right],
            "Ctrl+Right is still Right for now"
        );
    }

    #[test]
    fn a_lone_escape_is_known_on_a_pause_or_a_second_escape() {
        let mut d = Decoder::new();
        assert_eq!(d.push(0x1b), None, "it may start a sequence");
        assert_eq!(d.flush(), Some(CellKey::Escape));
        assert_eq!(d.flush(), None);
        assert_eq!(keys(b"\x1b\x1b[A"), [CellKey::Escape, CellKey::Up]);
    }

    #[test]
    fn unicode_arrives_byte_by_byte() {
        assert_eq!(
            keys("șé日🙂".as_bytes()),
            ['ș', 'é', '日', '🙂'].map(CellKey::Char)
        );
        // A cut character is dropped, and the byte that cut it is read on its own.
        assert_eq!(keys(b"\xe6\x97a"), [CellKey::Char('a')]);
        assert_eq!(keys(b"\xc3\x1b[A"), [CellKey::Up]);
        assert_eq!(keys(b"\xe6\x97"), [], "unfinished at a pause: dropped");
    }

    #[test]
    fn unknown_and_overlong_sequences_are_dropped_and_decoding_goes_on() {
        assert_eq!(keys(b"\x1b[99~a"), [CellKey::Char('a')]);
        assert_eq!(
            keys(b"\x1b[1;2;3;4;5;6;7A b"),
            [CellKey::Char(' '), CellKey::Char('b')]
        );
        assert_eq!(
            keys(b"\x1bxq"),
            [CellKey::Char('q')],
            "Alt+x is not handled yet"
        );
        // Every byte value, after every prefix: never a panic.
        for prefix in [&b""[..], b"\x1b", b"\x1b[", b"\x1bO", b"\xe2", b"\xf0\x9f"] {
            for b in 0..=255u8 {
                let mut d = Decoder::new();
                for &p in prefix {
                    let _ = d.push(p);
                }
                let _ = d.push(b);
                let _ = d.flush();
            }
        }
    }
}
