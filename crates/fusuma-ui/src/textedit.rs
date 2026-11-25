//! What a text field adds beyond typing: undo and redo, selecting by dragging and by double click (a word), moving
//! by word, and a placeholder. Typing and deleting are grouped, so one undo takes back a run of typed characters or a
//! run of deletions, not one letter.

use crate::textinput::InputText;
use crate::tree::{Kind, NodeId, Tree};

/// How many edits are kept for undo (and, separately, for redo).
pub const UNDO_DEPTH: usize = 4;

/// Two presses on a field closer than this many milliseconds and this many pixels are a double click.
const DOUBLE_MS: u32 = 400;
const DOUBLE_PX: i32 = 8;

/// What a press on a field does to the selection, at the next layout (which has a renderer to measure with).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Click {
    /// A press: the cursor goes there, the selection is gone.
    Place,
    /// A drag from the press: the cursor goes there, the other end of the selection stays.
    Extend,
    /// A double click: the word there is selected.
    Word,
}

/// The kind of edit that ended the last undo step, so the next edit of the same kind joins it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    /// None, or something that breaks a run (a cursor move, a focus change).
    None,
    Typing,
    Deleting,
}

/// The state of a field at some moment.
#[derive(Clone, Copy, Debug)]
struct Snap {
    id: NodeId,
    text: InputText,
    cursor: u8,
    anchor: u8,
}

/// The undo and redo steps of the text fields, a few of each.
#[derive(Clone, Debug)]
pub(crate) struct History {
    undo: [Option<Snap>; UNDO_DEPTH],
    redo: [Option<Snap>; UNDO_DEPTH],
    last: Group,
}

fn push(list: &mut [Option<Snap>; UNDO_DEPTH], s: Snap) {
    if let Some(slot) = list.iter_mut().find(|s| s.is_none()) {
        *slot = Some(s);
    } else {
        // Full: the oldest goes.
        list.rotate_left(1);
        list[UNDO_DEPTH - 1] = Some(s);
    }
}

fn pop(list: &mut [Option<Snap>; UNDO_DEPTH]) -> Option<Snap> {
    list.iter_mut().rev().find_map(Option::take)
}

impl History {
    pub(crate) const fn new() -> Self {
        Self {
            undo: [None; UNDO_DEPTH],
            redo: [None; UNDO_DEPTH],
            last: Group::None,
        }
    }

    /// Drops what is remembered about field `id` (its text was set from code, or it is gone).
    pub(crate) fn forget(&mut self, id: NodeId) {
        for list in [&mut self.undo, &mut self.redo] {
            for s in list.iter_mut() {
                if s.is_some_and(|s| s.id == id) {
                    *s = None;
                }
            }
            // Keep the used ones together at the front, in order.
            let mut to = 0;
            for from in 0..UNDO_DEPTH {
                if list[from].is_some() {
                    list.swap(from, to);
                    to += 1;
                }
            }
        }
        self.last = Group::None;
    }

    /// A move or anything else that ends a run of typing.
    pub(crate) const fn break_run(&mut self) {
        self.last = Group::None;
    }
}

/// Whether `c` belongs to a word.
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The start of the word (or of the run of spaces and symbols) around byte `at` of `s`.
pub(crate) fn word_start(s: &str, at: usize) -> usize {
    let at = at.min(s.len());
    let class = s[at..]
        .chars()
        .next()
        .or_else(|| s[..at].chars().next_back())
        .map(word_char);
    let mut i = at;
    for (j, c) in s[..at].char_indices().rev() {
        if Some(word_char(c)) != class {
            break;
        }
        i = j;
    }
    i
}

/// The end of the word (or run) around byte `at` of `s`.
pub(crate) fn word_end(s: &str, at: usize) -> usize {
    let at = at.min(s.len());
    let class = s[at..]
        .chars()
        .next()
        .or_else(|| s[..at].chars().next_back())
        .map(word_char);
    let mut i = at;
    for (j, c) in s[at..].char_indices() {
        if Some(word_char(c)) != class {
            break;
        }
        i = at + j + c.len_utf8();
    }
    i
}

/// Where Ctrl+Left goes from byte `at`: the start of the previous word.
pub(crate) fn prev_word(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    // Skip what is not a word, then the word.
    while let Some(c) = s[..i].chars().next_back() {
        if word_char(c) {
            break;
        }
        i -= c.len_utf8();
    }
    while let Some(c) = s[..i].chars().next_back() {
        if !word_char(c) {
            break;
        }
        i -= c.len_utf8();
    }
    i
}

/// Where Ctrl+Right goes from byte `at`: the end of the next word.
pub(crate) fn next_word(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    while let Some(c) = s[i..].chars().next() {
        if word_char(c) {
            break;
        }
        i += c.len_utf8();
    }
    while let Some(c) = s[i..].chars().next() {
        if !word_char(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

impl<const N: usize> Tree<N> {
    /// Remembers the state of field `i` before an edit of kind `group`, unless that edit continues a run.
    pub(crate) fn record(&mut self, i: usize, group: Group) {
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            ..
        } = self.nodes[i].kind
        else {
            return;
        };
        let id = self.id_of(i);
        let continues = self.history.last == group
            && self
                .history
                .undo
                .iter()
                .rev()
                .find_map(|s| *s)
                .is_some_and(|s| s.id == id);
        // A new edit ends what could have been redone.
        self.history.redo = [None; UNDO_DEPTH];
        if !continues {
            push(
                &mut self.history.undo,
                Snap {
                    id,
                    text,
                    cursor,
                    anchor,
                },
            );
        }
        self.history.last = group;
    }

    /// Takes back the last edit of the focused field, or puts back what was taken back. Returns whether anything
    /// changed.
    pub(crate) fn undo_redo(&mut self, redo: bool) -> bool {
        let Some(i) = self.focused_input_public() else {
            return false;
        };
        let id = self.id_of(i);
        let h = &mut self.history;
        let (from, to) = if redo {
            (&mut h.redo, &mut h.undo)
        } else {
            (&mut h.undo, &mut h.redo)
        };
        // Only the focused field's own step: the newest one must be this field's.
        let Some(newest) = from.iter().rev().find_map(|s| *s) else {
            return false;
        };
        if newest.id != id {
            return false;
        }
        let Some(snap) = pop(from) else { return false };
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            ..
        } = &mut self.nodes[i].kind
        else {
            return false;
        };
        push(
            to,
            Snap {
                id,
                text: *text,
                cursor: *cursor,
                anchor: *anchor,
            },
        );
        (*text, *cursor, *anchor) = (snap.text, snap.cursor, snap.anchor);
        h.last = Group::None;
        true
    }

    /// Sets the text shown, muted, in field `id` while it is empty (an empty string for none).
    pub fn set_placeholder(&mut self, id: NodeId, text: &'static str) {
        let Some(i) = self.slot(id) else { return };
        if let Kind::TextInput { placeholder, .. } = &mut self.nodes[i].kind
            && *placeholder != text
        {
            *placeholder = text;
            self.request(i, crate::tree::Flags::PAINT);
        }
    }

    /// A press on field `i` at `x` is a double click if the last press was on it, near, a moment ago.
    pub(crate) fn click_kind(&mut self, i: usize, x: i32) -> Click {
        let id = self.id_of(i);
        let double = self.last_press.is_some_and(|(p, px, at)| {
            p == id
                && (i64::from(px) - i64::from(x)).abs() <= i64::from(DOUBLE_PX)
                && self.clock.saturating_sub(at) <= DOUBLE_MS
        });
        // The second press of a pair is the end of it: a third starts over.
        self.last_press = if double {
            None
        } else {
            Some((id, x, self.clock))
        };
        if double { Click::Word } else { Click::Place }
    }

    /// The pointer moved to `x` while pressing field `i`: the selection follows.
    pub(crate) fn input_drag(&mut self, i: usize, x: i32) {
        if matches!(self.nodes[i].kind, Kind::TextInput { .. }) {
            self.pending_click = Some((self.id_of(i), x, Click::Extend));
            self.request(i, crate::tree::Flags::LAYOUT);
        }
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Point, Probe, Rect, Renderer, TextStyle};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::textinput::{Edit, INPUT_CAP};
    use crate::theme::Theme;
    use crate::tree::{ActionKind, CrossAlign, Style};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(400, 120).unwrap()
    }

    struct Ui {
        t: Tree<8>,
        field: NodeId,
        other: NodeId,
    }

    fn ui(initial: &str) -> Ui {
        let mut t = Tree::<8>::new();
        let root = t
            .column(
                None,
                Style::new().gap(8).padding(8).align(CrossAlign::Stretch),
            )
            .unwrap();
        let field = t.text_input(Some(root), initial).unwrap();
        let other = t.text_input(Some(root), "other").unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 400, 120));
        assert!(t.focus(field));
        Ui { t, field, other }
    }

    fn state(u: &Ui) -> (String, usize, usize) {
        let (c, a) = u.t.input_cursor(u.field).unwrap();
        (u.t.text(u.field).unwrap().to_owned(), c, a)
    }

    fn relayout(u: &mut Ui) {
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 400, 120));
    }

    #[test]
    fn words_are_runs_of_letters_digits_and_underscores_and_so_are_the_runs_between() {
        let s = "hello big_world, ok";
        assert_eq!((word_start(s, 2), word_end(s, 2)), (0, 5));
        assert_eq!((word_start(s, 0), word_end(s, 0)), (0, 5));
        assert_eq!(
            (word_start(s, 5), word_end(s, 5)),
            (5, 6),
            "the space after the word"
        );
        assert_eq!(
            (word_start(s, 8), word_end(s, 8)),
            (6, 15),
            "underscore joins"
        );
        assert_eq!(
            (word_start(s, 15), word_end(s, 15)),
            (15, 17),
            "the comma and the space"
        );
        assert_eq!(
            (word_start(s, s.len()), word_end(s, s.len())),
            (17, 19),
            "at the end: the last word"
        );
        assert_eq!((word_start("", 0), word_end("", 0)), (0, 0));
        let u = "héllo wörld";
        assert_eq!(
            (word_start(u, 3), word_end(u, 3)),
            (0, 6),
            "multi-byte letters are letters"
        );
        assert_eq!(prev_word(s, 19), 17);
        assert_eq!(prev_word(s, 17), 6);
        assert_eq!(prev_word(s, 6), 0);
        assert_eq!(prev_word(s, 0), 0);
        assert_eq!(next_word(s, 0), 5);
        assert_eq!(next_word(s, 5), 15);
        assert_eq!(next_word(s, 15), 19);
        assert_eq!(next_word(s, 19), 19);
    }

    #[test]
    fn word_moves_and_selections_by_word() {
        let mut u = ui("one two three");
        u.t.edit(Edit::WordLeft(false));
        assert_eq!(state(&u).1, 8);
        u.t.edit(Edit::WordLeft(true));
        assert_eq!(state(&u), ("one two three".into(), 4, 8));
        assert_eq!(u.t.selected_text(), Some("two "));
        u.t.edit(Edit::WordRight(false));
        assert_eq!(
            state(&u).1,
            7,
            "a move collapses and goes on from the cursor"
        );
    }

    #[test]
    fn a_run_of_typing_is_one_undo_and_redo_puts_it_back() {
        let mut u = ui("");
        for c in ["h", "e", "l", "l", "o"] {
            u.t.type_text(c);
        }
        assert_eq!(state(&u).0, "hello");
        let _ = u.t.pop_action();
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u), (String::new(), 0, 0), "the whole run");
        assert!(
            u.t.pop_action()
                .iter()
                .any(|a| a.kind == ActionKind::TextChanged),
            "the app is told"
        );
        u.t.edit(Edit::Redo);
        assert_eq!(state(&u), ("hello".into(), 5, 5));
        u.t.edit(Edit::Undo);
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u).0, "", "nothing more to undo is no change");
    }

    #[test]
    fn typing_and_deleting_are_separate_steps_and_a_move_breaks_a_run() {
        let mut u = ui("");
        u.t.type_text("abc");
        u.t.type_text("d");
        u.t.edit(Edit::Backspace);
        u.t.edit(Edit::Backspace);
        assert_eq!(state(&u).0, "ab");
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u).0, "abcd", "both deletions at once");
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u).0, "", "then the typing");
        u.t.edit(Edit::Redo);
        u.t.edit(Edit::Redo);
        assert_eq!(state(&u).0, "ab");
        // A move in the middle of typing splits it.
        let mut u = ui("");
        u.t.type_text("ab");
        u.t.edit(Edit::Left(false));
        u.t.type_text("X");
        assert_eq!(state(&u).0, "aXb");
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u).0, "ab");
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u).0, "");
    }

    #[test]
    fn a_new_edit_ends_what_could_be_redone_and_only_a_few_steps_are_kept() {
        let mut u = ui("");
        u.t.type_text("a");
        u.t.edit(Edit::Undo);
        u.t.type_text("b");
        u.t.edit(Edit::Redo);
        assert_eq!(state(&u).0, "b", "nothing to redo after a new edit");
        // More edits than the history holds: the oldest are lost, the newest come back.
        let mut u = ui("");
        for k in 0..(UNDO_DEPTH + 3) {
            u.t.type_text("x");
            u.t.edit(Edit::Left(false)); // each separate: a move ends the run
            let _ = k;
        }
        for _ in 0..UNDO_DEPTH + 3 {
            u.t.edit(Edit::Undo);
        }
        assert_eq!(
            state(&u).0.len(),
            3,
            "{UNDO_DEPTH} undone of {}",
            UNDO_DEPTH + 3
        );
    }

    #[test]
    fn undo_is_for_the_focused_field_and_code_changes_forget_it() {
        let mut u = ui("");
        u.t.type_text("abc");
        assert!(u.t.focus(u.other));
        u.t.edit(Edit::Undo);
        assert_eq!(
            u.t.text(u.other),
            Some("other"),
            "the other field's text is untouched"
        );
        assert!(u.t.focus(u.field));
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u).0, "", "back in the field it works");
        u.t.type_text("zzz");
        u.t.set_text(u.field, "from code");
        u.t.edit(Edit::Undo);
        assert_eq!(state(&u).0, "from code", "a change from code is not undone");
        // A selection replaced by typing is restored whole.
        u.t.edit(Edit::SelectAll);
        u.t.type_text("new");
        u.t.edit(Edit::Undo);
        assert_eq!(
            state(&u),
            ("from code".into(), 9, 0),
            "the text and the selection"
        );
    }

    #[test]
    fn long_text_is_kept_up_to_the_capacity() {
        let mut u = ui("");
        let s = "0123456789".repeat(30);
        u.t.type_text(&s);
        assert_eq!(state(&u).0.len(), s.len().min(INPUT_CAP));
        const { assert!(INPUT_CAP >= 64 && INPUT_CAP <= 250) };
        #[cfg(feature = "long-text")]
        const { assert!(INPUT_CAP >= 240) };
    }

    fn press_x(u: &Ui, n: usize, text: &str) -> (i32, i32) {
        let r = u.t.rect(u.field).unwrap();
        let w = screen()
            .measure_text(
                &text[..n],
                &TextStyle::new(THEME.text.body, THEME.colors.text).font(THEME.fonts.body),
            )
            .w as i32;
        (
            r.x + THEME.spacing.sm as i32 + w + 1,
            r.y + (r.h / 2) as i32,
        )
    }

    #[test]
    fn dragging_across_text_selects_it_and_the_far_end_follows_the_pointer() {
        let text = "abcdefgh";
        let mut u = ui(text);
        let (x1, y) = press_x(&u, 2, text);
        u.t.advance(&THEME, 1000);
        u.t.pointer(Pointer::Down(Point::new(x1, y)));
        relayout(&mut u);
        assert_eq!(state(&u).1, 2);
        let (x4, _) = press_x(&u, 5, text);
        u.t.pointer(Pointer::Move(Point::new(x4, y)));
        relayout(&mut u);
        assert_eq!(
            state(&u),
            (text.into(), 5, 2),
            "the cursor moved, the anchor stayed"
        );
        assert_eq!(u.t.selected_text(), Some("cde"));
        let (x0, _) = press_x(&u, 0, text);
        u.t.pointer(Pointer::Move(Point::new(x0 - 20, y)));
        relayout(&mut u);
        assert_eq!(
            state(&u),
            (text.into(), 0, 2),
            "dragged back across the anchor"
        );
        u.t.pointer(Pointer::Up(Point::new(x0, y)));
        u.t.pointer(Pointer::Move(Point::new(x4, y)));
        relayout(&mut u);
        assert_eq!(
            state(&u).1,
            0,
            "after the lift the pointer no longer selects"
        );
    }

    #[test]
    fn a_double_click_selects_the_word_and_a_slow_or_far_second_press_does_not() {
        let text = "alpha beta gamma";
        let mut u = ui(text);
        let (x, y) = press_x(&u, 7, text); // inside "beta"
        let click = |u: &mut Ui, x: i32| {
            u.t.pointer(Pointer::Down(Point::new(x, y)));
            u.t.pointer(Pointer::Up(Point::new(x, y)));
            relayout(u);
        };
        u.t.advance(&THEME, 1000);
        click(&mut u, x);
        assert_eq!(state(&u).1, 7, "one press places the cursor");
        u.t.advance(&THEME, 150);
        click(&mut u, x + 2);
        assert_eq!(
            u.t.selected_text(),
            Some("beta"),
            "two quick ones select the word"
        );
        // A third quick one starts over.
        u.t.advance(&THEME, 100);
        click(&mut u, x);
        assert_eq!(
            state(&u).1,
            state(&u).2,
            "a third press is a plain press again"
        );
        // Slow.
        u.t.advance(&THEME, 2000);
        click(&mut u, x);
        u.t.advance(&THEME, 900);
        click(&mut u, x);
        assert_eq!(state(&u).1, state(&u).2, "too slow");
        // Far apart.
        u.t.advance(&THEME, 2000);
        click(&mut u, x);
        u.t.advance(&THEME, 100);
        let (far, _) = press_x(&u, 14, text);
        click(&mut u, far);
        assert_eq!(state(&u).1, state(&u).2, "not the same place");
    }

    #[test]
    fn a_placeholder_shows_while_the_field_is_empty_and_never_over_text() {
        let mut u = ui("");
        let mut plain = ui("");
        u.t.set_placeholder(u.field, "Search");
        assert!(u.t.focus(u.other));
        assert!(plain.t.focus(plain.other));
        let draw = |u: &mut Ui| {
            let mut s = screen();
            u.t.update(&s, &THEME, Rect::new(0, 0, 400, 120));
            s.clear(THEME.colors.background).unwrap();
            u.t.paint(&mut s, &THEME).unwrap();
            (s, u.t.rect(u.field).unwrap())
        };
        let ink = |s: &VelloCpu, r: Rect| {
            let inner = r.inset(6);
            (inner.y..inner.bottom())
                .flat_map(|y| (inner.x..inner.right()).map(move |x| (x, y)))
                .filter(|&(x, y)| s.pixel(x, y) != s.pixel(inner.x, inner.y))
                .count()
        };
        let (s1, r1) = draw(&mut u);
        let (s2, r2) = draw(&mut plain);
        assert!(ink(&s1, r1) > 20, "the placeholder is there");
        assert_eq!(ink(&s2, r2), 0, "none without one");
        u.t.type_text("");
        assert!(u.t.focus(u.field));
        u.t.type_text("x");
        let (s3, r3) = draw(&mut u);
        let with_text = ink(&s3, r3);
        u.t.set_placeholder(u.field, "");
        let (s4, r4) = draw(&mut u);
        assert_eq!(
            ink(&s4, r4),
            with_text,
            "a placeholder is not drawn over text"
        );
    }
}
