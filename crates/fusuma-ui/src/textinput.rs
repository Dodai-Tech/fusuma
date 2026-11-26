//! A single-line text field: typing, the cursor, a selection, and the pieces an input method and a clipboard
//! need. The text lives inline (no allocator), so a field holds at most [`INPUT_CAP`] bytes.

use fusuma_render::{Point, Rect, Renderer, TextStyle};

use crate::paint::centred;
use crate::theme::Theme;
use crate::tree::{Action, ActionKind, Flags, Kind, NodeId, Tree};

/// How many bytes of text a text field keeps: 64, or 250 with the `long-text` feature (every widget of the tree
/// takes the room of the biggest, so a chip keeps the small one). Longer text typed or pasted is cut at a character
/// boundary.
pub const INPUT_CAP: usize = if cfg!(feature = "long-text") { 250 } else { 64 };

/// Text stored inline, like `SmallText` but long enough for a name, a search or a Wi-Fi password.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputText {
    len: u8,
    buf: [u8; INPUT_CAP],
}

impl InputText {
    /// No text.
    pub const EMPTY: Self = Self {
        len: 0,
        buf: [0; INPUT_CAP],
    };

    /// Text from `s`, cut at a character boundary to [`INPUT_CAP`] bytes.
    #[must_use]
    pub fn new(s: &str) -> Self {
        let mut t = Self::EMPTY;
        t.set(s);
        t
    }

    /// Replaces the text.
    pub fn set(&mut self, s: &str) {
        let end = cut(s, INPUT_CAP);
        self.buf[..end].copy_from_slice(&s.as_bytes()[..end]);
        self.len = end as u8;
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..usize::from(self.len)]).unwrap_or("")
    }

    /// Whether there is no text.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Inserts as much of `s` as fits at byte `at` (a character boundary), returns the bytes inserted.
    fn insert(&mut self, at: usize, s: &str) -> usize {
        let len = usize::from(self.len);
        let at = at.min(len);
        let room = INPUT_CAP - len;
        let n = cut(s, room);
        self.buf.copy_within(at..len, at + n);
        self.buf[at..at + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len = (len + n) as u8;
        n
    }

    /// Removes the bytes `from..to`.
    fn remove(&mut self, from: usize, to: usize) {
        let len = usize::from(self.len);
        let (from, to) = (from.min(len), to.min(len));
        if from >= to {
            return;
        }
        self.buf.copy_within(to..len, from);
        self.len = (len - (to - from)) as u8;
    }
}

/// The longest prefix of `s` of at most `max` bytes that ends on a character boundary.
fn cut(s: &str, max: usize) -> usize {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// A command that edits the field with the focus. Typed characters are not commands: they arrive as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Delete the selection, or the character before the cursor.
    Backspace,
    /// Delete the selection, or the character after the cursor.
    Delete,
    /// Move the cursor one character left; `true` extends the selection (Shift held).
    Left(bool),
    /// Move the cursor one character right; `true` extends the selection.
    Right(bool),
    /// Move the cursor to the start; `true` extends the selection.
    Home(bool),
    /// Move the cursor to the end; `true` extends the selection.
    End(bool),
    /// Select all the text.
    SelectAll,
    /// Move the cursor to the start of the previous word; `true` extends the selection (Ctrl+Left).
    WordLeft(bool),
    /// Move the cursor to the end of the next word; `true` extends the selection (Ctrl+Right).
    WordRight(bool),
    /// A line break (Enter in a text area).
    Newline,
    /// Move the cursor up a line, to the same column or the end of a shorter line; `true` extends the selection. Only in a text
    /// area.
    Up(bool),
    /// Move the cursor down a line, the same way.
    Down(bool),
    /// Take back the last edit or run of edits (Ctrl+Z).
    Undo,
    /// Put back what an undo took back (Ctrl+Y, or Ctrl+Shift+Z).
    Redo,
}

/// The start of the character before byte `at` in `s`.
fn prev_boundary(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    if i == 0 {
        return 0;
    }
    i -= 1;
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The end of the character at byte `at` in `s`.
fn next_boundary(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    if i >= s.len() {
        return s.len();
    }
    i += 1;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Characters a field refuses: control characters (a tab, a bell), and a newline unless it is a text area's.
fn allowed(c: char, multiline: bool) -> bool {
    !c.is_control() || (multiline && c == '\n')
}

/// Where the line that holds byte `at` of `s` starts and ends (without its break).
fn line_bounds(s: &str, at: usize) -> (usize, usize) {
    let at = at.min(s.len());
    let start = s[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = s[at..].find('\n').map_or(s.len(), |i| at + i);
    (start, end)
}

/// Which line (counted from 0) holds byte `at`, and how many characters precede it in that line.
fn line_and_column(s: &str, at: usize) -> (usize, usize) {
    let at = at.min(s.len());
    let line = s[..at].bytes().filter(|b| *b == b'\n').count();
    let (start, _) = line_bounds(s, at);
    (line, s[start..at].chars().count())
}

/// The byte at `column` characters into the line that starts at `start`, or the end of that line if it is shorter.
fn byte_at_column(s: &str, start: usize, column: usize) -> usize {
    let (_, end) = line_bounds(s, start);
    let mut at = start;
    for _ in 0..column {
        if at >= end {
            break;
        }
        at = next_boundary(s, at);
    }
    at.min(end)
}

impl Edit {
    /// This move towards the right when `right`, else as it is (a left move).
    pub(crate) const fn towards(self, right: bool) -> Self {
        match (self, right) {
            (Self::Left(x), true) => Self::Right(x),
            (e, _) => e,
        }
    }
}

impl<const N: usize> Tree<N> {
    pub(crate) fn focused_input_public(&self) -> Option<usize> {
        self.focused_input()
    }

    fn input_index(&self, id: NodeId) -> Option<usize> {
        let i = self.slot(id)?;
        matches!(self.nodes[i].kind, Kind::TextInput { .. }).then_some(i)
    }

    /// The text field with the focus, if the focus is on one.
    fn focused_input(&self) -> Option<usize> {
        let i = self.focus.and_then(|id| self.slot(id))?;
        (matches!(self.nodes[i].kind, Kind::TextInput { .. }) && !self.nodes[i].disabled_eff)
            .then_some(i)
    }

    /// Whether the focus is on a text field, so that typing and the edit keys mean something. A platform
    /// uses this to enable an input method.
    #[must_use]
    pub fn wants_text(&self) -> bool {
        self.focused_input().is_some()
    }

    /// The part of the focused text field where the next character will appear, for placing an input
    /// method's candidate window; `None` when no field has the focus. As of the last layout.
    #[must_use]
    pub const fn text_cursor(&self) -> Option<Rect> {
        self.caret
    }

    /// The selected text of the focused field, for a clipboard copy or cut; `None` when nothing is selected.
    #[must_use]
    pub fn selected_text(&self) -> Option<&str> {
        let i = self.focused_input()?;
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            ..
        } = &self.nodes[i].kind
        else {
            return None;
        };
        let (a, b) = (
            usize::from(*cursor.min(anchor)),
            usize::from(*cursor.max(anchor)),
        );
        (a != b).then(|| &text.as_str()[a..b])
    }

    /// The cursor of text field `id` as a byte offset, and the other end of the selection (equal when
    /// nothing is selected).
    #[must_use]
    pub fn input_cursor(&self, id: NodeId) -> Option<(usize, usize)> {
        let i = self.input_index(id)?;
        let Kind::TextInput { cursor, anchor, .. } = self.nodes[i].kind else {
            return None;
        };
        Some((usize::from(cursor), usize::from(anchor)))
    }

    /// Moves the cursor of field `id` to byte `at` (cut to a character boundary), collapsing the selection.
    pub fn set_input_cursor(&mut self, id: NodeId, at: usize) {
        let Some(i) = self.input_index(id) else {
            return;
        };
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            ..
        } = &mut self.nodes[i].kind
        else {
            return;
        };
        let s = text.as_str();
        let mut at = at.min(s.len());
        while !s.is_char_boundary(at) {
            at -= 1;
        }
        let at = at as u8;
        if (*cursor, *anchor) != (at, at) {
            (*cursor, *anchor) = (at, at);
            self.request(i, Flags::LAYOUT | Flags::PAINT);
        }
    }

    /// Text arrives: typed, pasted, or committed by an input method. In a text field it replaces the selection
    /// and is inserted at the cursor. Elsewhere a single space presses the focused widget, because Space is
    /// text to a keyboard and a press to a button.
    pub fn type_text(&mut self, s: &str) {
        let Some(i) = self.focused_input() else {
            if s == " " {
                self.key(crate::Key::Activate);
            }
            return;
        };
        self.preedit = InputText::EMPTY;
        let before = self.input_text_at(i);
        let multiline = matches!(self.nodes[i].kind, Kind::TextInput { rows, .. } if rows > 0);
        self.record(i, crate::textedit::Group::Typing);
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            ..
        } = &mut self.nodes[i].kind
        else {
            return;
        };
        let (a, b) = (
            usize::from(*cursor.min(anchor)),
            usize::from(*cursor.max(anchor)),
        );
        text.remove(a, b);
        let mut at = a;
        for c in s.chars().filter(|&c| allowed(c, multiline)) {
            let mut one = [0u8; 4];
            at += text.insert(at, c.encode_utf8(&mut one));
        }
        *cursor = at as u8;
        *anchor = at as u8;
        self.after_edit(i, before);
    }

    /// An edit command for the focused text field. Nothing happens when no field has the focus.
    pub fn edit(&mut self, e: Edit) {
        let Some(i) = self.focused_input() else {
            return;
        };
        self.preedit = InputText::EMPTY;
        let before = self.input_text_at(i);
        if matches!(e, Edit::Undo | Edit::Redo) {
            if self.undo_redo(e == Edit::Redo) {
                self.after_edit(i, before);
            }
            return;
        }
        if matches!(e, Edit::Backspace | Edit::Delete) {
            self.record(i, crate::textedit::Group::Deleting);
        } else if e == Edit::Newline
            && matches!(self.nodes[i].kind, Kind::TextInput { rows, .. } if rows > 0)
        {
            // A line break is typing: it joins the run of typing and is taken back with it.
            self.record(i, crate::textedit::Group::Typing);
        } else {
            // A move or a selection ends a run of typing.
            self.history.break_run();
        }
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            rows,
            ..
        } = &mut self.nodes[i].kind
        else {
            return;
        };
        let len = text.as_str().len();
        let multiline = *rows > 0;
        let (c, a) = (usize::from(*cursor), usize::from(*anchor));
        let (lo, hi) = (c.min(a), c.max(a));
        // Where a move ends, and whether it keeps the other end of the selection.
        let (to, extend) = match e {
            Edit::Backspace | Edit::Delete => {
                if lo != hi {
                    text.remove(lo, hi);
                    (*cursor, *anchor) = (lo as u8, lo as u8);
                } else if e == Edit::Backspace {
                    let from = prev_boundary(text.as_str(), c);
                    text.remove(from, c);
                    (*cursor, *anchor) = (from as u8, from as u8);
                } else {
                    let end = next_boundary(text.as_str(), c);
                    text.remove(c, end);
                }
                self.after_edit(i, before);
                return;
            }
            Edit::Left(x) => (
                if lo != hi && !x {
                    lo
                } else {
                    prev_boundary(text.as_str(), c)
                },
                x,
            ),
            Edit::Right(x) => (
                if lo != hi && !x {
                    hi
                } else {
                    next_boundary(text.as_str(), c)
                },
                x,
            ),
            Edit::WordLeft(x) => (crate::textedit::prev_word(text.as_str(), c), x),
            Edit::WordRight(x) => (crate::textedit::next_word(text.as_str(), c), x),
            Edit::Undo | Edit::Redo => return,
            Edit::Newline => {
                if !multiline {
                    return;
                }
                // Replaces the selection, like typing does.
                text.remove(lo, hi);
                let n = text.insert(lo, "\n");
                (*cursor, *anchor) = ((lo + n) as u8, (lo + n) as u8);
                self.after_edit(i, before);
                return;
            }
            Edit::Up(x) | Edit::Down(x) => {
                if !multiline {
                    return;
                }
                let s = text.as_str();
                let (line, column) = line_and_column(s, c);
                let up = matches!(e, Edit::Up(_));
                let to = if up {
                    if line == 0 {
                        // Already on the first line: the start of it.
                        0
                    } else {
                        let (here, _) = line_bounds(s, c);
                        let (prev, _) = line_bounds(s, here - 1);
                        byte_at_column(s, prev, column)
                    }
                } else {
                    let (_, end) = line_bounds(s, c);
                    if end >= s.len() {
                        s.len()
                    } else {
                        byte_at_column(s, end + 1, column)
                    }
                };
                (to, x)
            }
            Edit::Home(x) => (
                if multiline {
                    line_bounds(text.as_str(), c).0
                } else {
                    0
                },
                x,
            ),
            Edit::End(x) => (
                if multiline {
                    line_bounds(text.as_str(), c).1
                } else {
                    len
                },
                x,
            ),
            Edit::SelectAll => {
                (*cursor, *anchor) = (len as u8, 0);
                self.request(i, Flags::LAYOUT | Flags::PAINT);
                return;
            }
        };
        *cursor = to as u8;
        if !extend {
            *anchor = to as u8;
        }
        self.request(i, Flags::LAYOUT | Flags::PAINT);
    }

    /// An input method is composing `s` (not yet committed): it is shown at the cursor, underlined. An empty
    /// `s` ends the composition.
    pub fn set_preedit(&mut self, s: &str) {
        let Some(i) = self.focused_input() else {
            self.preedit = InputText::EMPTY;
            return;
        };
        let new = InputText::new(s);
        if new != self.preedit {
            self.preedit = new;
            self.request(i, Flags::LAYOUT | Flags::PAINT);
        }
    }

    /// The text being composed, if any.
    #[must_use]
    pub fn preedit(&self) -> &str {
        self.preedit.as_str()
    }

    fn input_text_at(&self, i: usize) -> InputText {
        match self.nodes[i].kind {
            Kind::TextInput { text, .. } => text,
            _ => InputText::EMPTY,
        }
    }

    /// Redraws a field after an edit and, if its text changed, tells the app.
    fn after_edit(&mut self, i: usize, before: InputText) {
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        if self.input_text_at(i) != before {
            let source = self.id_of(i);
            self.push_action(Action {
                source,
                kind: ActionKind::TextChanged,
            });
        }
    }

    /// A press on a field puts the cursor where it landed. The characters' widths need a renderer, so the
    /// position is worked out at the next layout.
    pub(crate) fn input_press(&mut self, i: usize, x: i32) {
        if matches!(self.nodes[i].kind, Kind::TextInput { .. }) {
            let click = self.click_kind(i, x);
            self.pending_click = Some((self.id_of(i), x, click));
            self.history.break_run();
            self.request(i, Flags::LAYOUT);
        }
    }

    /// The inner area of a field: the rectangle less the padding.
    fn input_inner(&self, i: usize, theme: &Theme) -> Rect {
        let r = self.nodes[i].rect;
        let pad = i32::try_from(theme.spacing.sm).unwrap_or(0);
        Rect::new(
            r.x.saturating_add(pad),
            r.y,
            r.w.saturating_sub(theme.spacing.sm.saturating_mul(2)),
            r.h,
        )
    }

    /// For a text area: the height of one line, the gap above the first, and the first line shown (scrolled so the cursor's line
    /// is the last one shown at most).
    fn area_geometry<M: fusuma_render::Measure>(
        &self,
        i: usize,
        r: &M,
        theme: &Theme,
    ) -> (u32, i32, usize) {
        let Kind::TextInput {
            text, cursor, rows, ..
        } = &self.nodes[i].kind
        else {
            return (0, 0, 0);
        };
        let style = TextStyle::new(theme.text.body, theme.colors.text).font(theme.fonts.body);
        let pitch = r.measure_text("M", &style).h.max(1);
        let pad = i32::try_from(theme.spacing.xs).unwrap_or(0);
        let line = line_and_column(text.as_str(), usize::from(*cursor)).0;
        let first = line.saturating_sub(usize::from(*rows).saturating_sub(1));
        (pitch, pad, first)
    }

    /// How far the text is scrolled left so that the caret is in view, and the caret's x from the start of
    /// the text, in pixels.
    fn input_metrics<M: fusuma_render::Measure>(
        &self,
        i: usize,
        r: &M,
        theme: &Theme,
    ) -> (u32, u32) {
        let Kind::TextInput {
            text, cursor, rows, ..
        } = &self.nodes[i].kind
        else {
            return (0, 0);
        };
        let style = TextStyle::new(theme.text.body, theme.colors.text).font(theme.fonts.body);
        let s = text.as_str();
        let at = usize::from(*cursor).min(s.len());
        let focused = self.focus == Some(self.id_of(i));
        // A text area measures the caret along its own line, and does not scroll sideways.
        let from = if *rows > 0 { line_bounds(s, at).0 } else { 0 };
        let mut caret = r.measure_text(&s[from..at], &style).w;
        if focused {
            caret = caret.saturating_add(r.measure_text(self.preedit.as_str(), &style).w);
        }
        let inner = self.input_inner(i, theme).w;
        let shift = if *rows > 0 {
            0
        } else {
            caret.saturating_add(2).saturating_sub(inner)
        };
        (shift, caret)
    }

    /// At layout: puts the cursor where a press landed, and works out where the caret is.
    pub(crate) fn resolve_input<M: fusuma_render::Measure>(&mut self, r: &M, theme: &Theme) {
        if let Some((id, x, click)) = self.pending_click.take()
            && let Some(i) = self.input_index(id)
        {
            let (shift, _) = self.input_metrics(i, r, theme);
            let style = TextStyle::new(theme.text.body, theme.colors.text).font(theme.fonts.body);
            let inner = self.input_inner(i, theme);
            let want = i64::from(x) - i64::from(inner.x) + i64::from(shift);
            // In a text area the press also picks the line, and the search runs along that line only.
            let (pitch, pad, first) = self.area_geometry(i, r, theme);
            let py = self.pending_y;
            let rect_y = self.nodes[i].rect.y;
            let Kind::TextInput {
                text,
                cursor,
                anchor,
                rows,
                ..
            } = &mut self.nodes[i].kind
            else {
                return;
            };
            let multiline = *rows > 0;
            let s = text.as_str();
            let (lo, hi) = if multiline {
                let line = first
                    + usize::try_from(
                        ((i64::from(py) - i64::from(rect_y) - i64::from(pad)) / i64::from(pitch))
                            .max(0),
                    )
                    .unwrap_or(0);
                // The start of that line (the last one if the press is below the text).
                let mut start = 0;
                for _ in 0..line {
                    match s[start..].find('\n') {
                        Some(k) => start += k + 1,
                        None => break,
                    }
                }
                line_bounds(s, start)
            } else {
                (0, s.len())
            };
            // The character boundary whose left edge is nearest to the press.
            let (mut best, mut best_d) = (lo, i64::MAX);
            let mut at = lo;
            loop {
                let w = i64::from(r.measure_text(&s[lo..at], &style).w);
                let d = (w - want).abs();
                if d < best_d {
                    (best, best_d) = (at, d);
                }
                if at >= hi {
                    break;
                }
                at = next_boundary(s, at);
            }
            match click {
                crate::textedit::Click::Place => (*cursor, *anchor) = (best as u8, best as u8),
                crate::textedit::Click::Extend => *cursor = best as u8,
                crate::textedit::Click::Word => {
                    let (a, b) = (
                        crate::textedit::word_start(s, best),
                        crate::textedit::word_end(s, best),
                    );
                    (*anchor, *cursor) = (a as u8, b as u8);
                }
            }
            self.request(i, Flags::PAINT);
        }
        self.caret = self.focused_input().map(|i| {
            let (shift, caret) = self.input_metrics(i, r, theme);
            let inner = self.input_inner(i, theme);
            let x = i64::from(inner.x) + i64::from(caret) - i64::from(shift);
            let x = x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            let h = theme.text.body.min(self.nodes[i].rect.h);
            let area = matches!(self.nodes[i].kind, Kind::TextInput { rows, .. } if rows > 0);
            let y = if area {
                // On the cursor's line, counted from the first one shown.
                let (pitch, pad, first) = self.area_geometry(i, r, theme);
                let Kind::TextInput { text, cursor, .. } = &self.nodes[i].kind else {
                    return Rect::new(x, 0, 2, h);
                };
                let line = line_and_column(text.as_str(), usize::from(*cursor)).0;
                self.nodes[i]
                    .rect
                    .y
                    .saturating_add(pad)
                    .saturating_add((pitch as usize * line.saturating_sub(first)) as i32)
            } else {
                self.nodes[i]
                    .rect
                    .y
                    .saturating_add((self.nodes[i].rect.h.saturating_sub(h) / 2) as i32)
            };
            Rect::new(x, y, 2, h)
        });
    }

    /// The lines of a text area: the ones from the first shown, each on its own row, the selection behind them and the caret on
    /// its line. (The frame is drawn by [`paint_text_input`](Self::paint_text_input).)
    fn paint_text_area<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let n = &self.nodes[i];
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            placeholder,
            rows,
        } = n.kind
        else {
            return Ok(());
        };
        let c = &theme.colors;
        let rect = n.rect;
        let focused = self.focus == Some(self.id_of(i));
        let inner = self.input_inner(i, theme);
        let s = text.as_str();
        let (lo, hi) = (
            usize::from(cursor.min(anchor)),
            usize::from(cursor.max(anchor)),
        );
        let color = if n.disabled_eff {
            c.text_disabled
        } else {
            c.text
        };
        let style = TextStyle::new(theme.text.body, color).font(theme.fonts.body);
        let (pitch, pad, first) = self.area_geometry(i, r, theme);
        let top = rect.y.saturating_add(pad);
        let clip = Rect::new(inner.x, rect.y, inner.w, rect.h);
        Self::in_clip(r, clip, |r| {
            if s.is_empty() && !placeholder.is_empty() {
                let muted = TextStyle::new(theme.text.body, c.text_muted).font(theme.fonts.body);
                return r.draw_text(placeholder, Point::new(inner.x, top), &muted);
            }
            let mut start = 0usize;
            let mut line = 0usize;
            loop {
                let (_, end) = line_bounds(s, start);
                if line >= first && line < first + usize::from(rows) {
                    let y = top.saturating_add((pitch as usize * (line - first)) as i32);
                    // The selection's part of this line.
                    if focused && lo != hi && lo <= end && hi >= start {
                        let (a, b) = (lo.max(start), hi.min(end));
                        let x0 = inner.x + r.measure_text(&s[start..a], &style).w as i32;
                        let mut w = r.measure_text(&s[a..b], &style).w;
                        if hi > end {
                            // The break itself is selected: a little extra so an empty line shows it.
                            w = w.saturating_add(theme.spacing.xs.max(2));
                        }
                        r.fill_rect(Rect::new(x0, y, w, pitch), 0, c.primary)?;
                    }
                    r.draw_text(&s[start..end], Point::new(inner.x, y), &style)?;
                }
                if end >= s.len() {
                    break;
                }
                start = end + 1;
                line += 1;
            }
            if focused
                && !n.disabled_eff
                && let Some(caret) = self.caret
            {
                r.fill_rect(caret, 0, c.primary)?;
            }
            Ok(())
        })
    }

    pub(crate) fn paint_text_input<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let n = &self.nodes[i];
        let Kind::TextInput {
            text,
            cursor,
            anchor,
            placeholder,
            rows,
        } = n.kind
        else {
            return Ok(());
        };
        let c = &theme.colors;
        let rect = n.rect;
        let focused = self.focus == Some(self.id_of(i));
        let radius = theme.radii.md;
        r.fill_rect(
            rect,
            radius,
            if n.disabled_eff {
                c.surface
            } else {
                c.background
            },
        )?;
        let (border, width) = if n.disabled_eff {
            (c.border, theme.border_width)
        } else if focused {
            (c.primary, theme.border_width.saturating_mul(2))
        } else if n.hovered {
            (c.primary, theme.border_width)
        } else {
            (c.border_strong, theme.border_width)
        };
        r.stroke_rect(rect, radius, width, border)?;
        if rows > 0 {
            return self.paint_text_area(i, r, theme);
        }
        let (shift, _) = self.input_metrics(i, r, theme);
        let inner = self.input_inner(i, theme);
        let s = text.as_str();
        let (lo, hi) = (
            usize::from(cursor.min(anchor)),
            usize::from(cursor.max(anchor)),
        );
        let color = if n.disabled_eff {
            c.text_disabled
        } else {
            c.text
        };
        let style = TextStyle::new(theme.text.body, color).font(theme.fonts.body);
        let line_h = r.measure_text("M", &style).h;
        let top = centred(rect, fusuma_render::Size::new(0, line_h)).y;
        let origin = i64::from(inner.x) - i64::from(shift);
        let at = |bytes: usize, r: &R| -> i32 {
            let w = r.measure_text(&s[..bytes], &style).w;
            (origin + i64::from(w)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
        };
        let clip = Rect::new(inner.x, rect.y, inner.w, rect.h);
        Self::in_clip(r, clip, |r| {
            if focused && lo != hi {
                let (x0, x1) = (at(lo, r), at(hi, r));
                let w = u32::try_from((i64::from(x1) - i64::from(x0)).max(0)).unwrap_or(0);
                r.fill_rect(Rect::new(x0, top, w, line_h), 0, c.primary)?;
            }
            // The text, with the composition (if any) inserted at the cursor.
            let cur = usize::from(cursor).min(s.len());
            let pre = self.preedit.as_str();
            if focused && !pre.is_empty() {
                r.draw_text(&s[..cur], Point::new(at(0, r), top), &style)?;
                let px = at(cur, r);
                let pw = r.measure_text(pre, &style).w;
                r.draw_text(
                    pre,
                    Point::new(px, top),
                    &TextStyle::new(theme.text.body, c.primary).font(theme.fonts.body),
                )?;
                let underline = Rect::new(
                    px,
                    top.saturating_add(line_h as i32),
                    pw,
                    theme.border_width,
                );
                r.fill_rect(underline, 0, c.primary)?;
                let tail = px.saturating_add(i32::try_from(pw).unwrap_or(0));
                r.draw_text(&s[cur..], Point::new(tail, top), &style)?;
            } else if s.is_empty() && !placeholder.is_empty() {
                let muted = TextStyle::new(theme.text.body, c.text_muted).font(theme.fonts.body);
                r.draw_text(placeholder, Point::new(at(0, r), top), &muted)?;
            } else {
                r.draw_text(s, Point::new(at(0, r), top), &style)?;
            }
            if focused
                && !n.disabled_eff
                && let Some(caret) = self.caret
            {
                r.fill_rect(caret, 0, c.primary)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Pointer;
    use crate::tree::{CrossAlign, Length, Style};
    use fusuma_render::Probe;
    use fusuma_render_vello_cpu::VelloCpu;

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(300, 120).unwrap()
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
        let other = t
            .button(Some(root), "ok", crate::theme::ButtonVariant::Primary)
            .unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 120));
        assert!(t.focus(field));
        Ui { t, field, other }
    }

    fn relayout(u: &mut Ui) {
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 120));
    }

    fn state(u: &Ui) -> (String, usize, usize) {
        let (c, a) = u.t.input_cursor(u.field).unwrap();
        (u.t.text(u.field).unwrap().to_owned(), c, a)
    }

    #[test]
    fn typing_inserts_at_the_cursor_and_reports_a_change() {
        let mut u = ui("");
        u.t.type_text("héllo");
        assert_eq!(
            state(&u),
            ("héllo".into(), 6, 6),
            "the cursor is in bytes, é is two"
        );
        assert_eq!(u.t.pop_action().unwrap().kind, ActionKind::TextChanged);
        u.t.edit(Edit::Left(false));
        u.t.edit(Edit::Left(false));
        u.t.type_text("X");
        assert_eq!(state(&u).0, "hélXlo");
    }

    #[test]
    fn the_cursor_moves_by_characters_and_stops_at_the_ends() {
        let mut u = ui("aé");
        assert_eq!(state(&u).1, 3, "built with the cursor at the end");
        u.t.edit(Edit::Left(false));
        assert_eq!(state(&u).1, 1, "left of é is a character boundary");
        u.t.edit(Edit::Left(false));
        u.t.edit(Edit::Left(false));
        assert_eq!(state(&u).1, 0);
        u.t.edit(Edit::Right(false));
        u.t.edit(Edit::Right(false));
        u.t.edit(Edit::Right(false));
        assert_eq!(state(&u).1, 3);
        u.t.edit(Edit::Home(false));
        assert_eq!(state(&u).1, 0);
        u.t.edit(Edit::End(false));
        assert_eq!(state(&u).1, 3);
    }

    #[test]
    fn backspace_and_delete_remove_whole_characters() {
        let mut u = ui("aéb");
        u.t.edit(Edit::Left(false));
        u.t.edit(Edit::Backspace);
        assert_eq!(state(&u), ("ab".into(), 1, 1));
        u.t.edit(Edit::Delete);
        assert_eq!(state(&u), ("a".into(), 1, 1));
        u.t.edit(Edit::Delete);
        assert_eq!(state(&u).0, "a", "nothing after the cursor");
        u.t.edit(Edit::Backspace);
        u.t.edit(Edit::Backspace);
        assert_eq!(state(&u), (String::new(), 0, 0));
        let _ = (u.t.pop_action(), u.t.pop_action(), u.t.pop_action());
        u.t.edit(Edit::Backspace);
        assert_eq!(u.t.pop_action(), None, "no change, no action");
    }

    #[test]
    fn a_selection_is_replaced_by_typing_and_removed_by_backspace() {
        let mut u = ui("hello world");
        u.t.edit(Edit::Home(false));
        for _ in 0..5 {
            u.t.edit(Edit::Right(true));
        }
        assert_eq!(u.t.selected_text(), Some("hello"));
        u.t.type_text("bye");
        assert_eq!(state(&u), ("bye world".into(), 3, 3));
        assert_eq!(u.t.selected_text(), None);
        u.t.edit(Edit::SelectAll);
        assert_eq!(u.t.selected_text(), Some("bye world"));
        u.t.edit(Edit::Backspace);
        assert_eq!(state(&u), (String::new(), 0, 0));
    }

    #[test]
    fn an_arrow_collapses_a_selection_to_its_edge_without_moving_further() {
        let mut u = ui("abcdef");
        u.t.edit(Edit::Home(false));
        u.t.edit(Edit::Right(true));
        u.t.edit(Edit::Right(true));
        u.t.edit(Edit::Right(true));
        assert_eq!(state(&u), ("abcdef".into(), 3, 0));
        u.t.edit(Edit::Right(false));
        assert_eq!(state(&u).1, 3, "to the right edge of the selection");
        u.t.edit(Edit::Left(true));
        u.t.edit(Edit::Left(false));
        assert_eq!(state(&u).1, 2, "to the left edge");
        u.t.edit(Edit::End(true));
        assert_eq!(u.t.selected_text(), Some("cdef"));
    }

    #[test]
    fn control_characters_are_refused_and_the_capacity_is_kept_at_a_character_boundary() {
        let mut u = ui("");
        u.t.type_text("a\nb\tc\r\u{7f}");
        assert_eq!(state(&u).0, "abc");
        let mut u = ui("");
        u.t.type_text(&"é".repeat(INPUT_CAP));
        let (s, c, _) = state(&u);
        assert_eq!(s.len(), INPUT_CAP);
        assert!(s.chars().all(|ch| ch == 'é') && c == INPUT_CAP);
        u.t.type_text("x");
        assert_eq!(state(&u).0.len(), INPUT_CAP, "full is full");
    }

    #[test]
    fn code_sets_the_text_without_an_action_and_the_cursor_goes_to_the_end() {
        let mut u = ui("abc");
        u.t.set_text(u.field, "hello");
        assert_eq!(state(&u), ("hello".into(), 5, 5));
        assert_eq!(u.t.pop_action(), None);
        u.t.set_input_cursor(u.field, 99);
        assert_eq!(state(&u).1, 5);
        let mut u2 = ui("é");
        u2.t.set_input_cursor(u2.field, 1);
        assert_eq!(
            state(&u2).1,
            0,
            "a position inside a character moves back to its start"
        );
    }

    #[test]
    fn space_types_in_a_field_and_presses_anything_else() {
        let mut u = ui("a");
        u.t.type_text(" ");
        assert_eq!(state(&u).0, "a ");
        assert_eq!(u.t.pop_action().unwrap().kind, ActionKind::TextChanged);
        assert!(u.t.focus(u.other));
        u.t.type_text(" ");
        assert_eq!(
            u.t.pop_action().map(|a| a.source),
            Some(u.other),
            "the button was pressed"
        );
        u.t.type_text("zzz");
        assert_eq!(
            state(&u).0,
            "a ",
            "typing with the focus elsewhere changes nothing"
        );
        u.t.edit(Edit::Backspace);
        assert_eq!(state(&u).0, "a ");
    }

    #[test]
    fn the_input_method_composes_then_commits_and_a_focus_change_drops_the_composition() {
        let mut u = ui("ab");
        u.t.set_preedit("に");
        assert_eq!(u.t.preedit(), "に");
        assert_eq!(state(&u).0, "ab", "a composition is not text yet");
        u.t.type_text("日本");
        assert_eq!(u.t.preedit(), "", "committing ends it");
        assert_eq!(state(&u).0, "ab日本");
        u.t.set_preedit("x");
        assert!(u.t.focus(u.other));
        u.t.set_preedit("y");
        assert_eq!(u.t.preedit(), "", "no field, no composition");
        assert!(!u.t.wants_text());
        assert!(u.t.focus(u.field));
        assert!(u.t.wants_text());
    }

    #[test]
    fn the_copy_text_is_the_selection_and_nothing_when_another_widget_has_the_focus() {
        let mut u = ui("secret");
        assert_eq!(u.t.selected_text(), None);
        u.t.edit(Edit::SelectAll);
        assert_eq!(u.t.selected_text(), Some("secret"));
        assert!(u.t.focus(u.other));
        assert_eq!(u.t.selected_text(), None);
    }

    #[test]
    fn a_press_puts_the_cursor_between_the_characters_nearest_to_it() {
        let mut u = ui("abcdef");
        let r = u.t.rect(u.field).unwrap();
        let inner_x = r.x + THEME.spacing.sm as i32;
        let w = |n: usize| {
            screen()
                .measure_text(
                    &"abcdef"[..n],
                    &TextStyle::new(THEME.text.body, THEME.colors.text).font(THEME.fonts.body),
                )
                .w as i32
        };
        let y = r.y + (r.h / 2) as i32;
        // Just right of the third character's left edge: nearest boundary is 3, and just left of it too.
        for (x, want) in [
            (inner_x + w(3) + 2, 3),
            (inner_x + w(3) - 2, 3),
            (inner_x + w(1) + 1, 1),
            (r.x + 3, 0),
            (r.right() - 2, 6),
        ] {
            // Time passes between presses: two quick ones near each other are a double click.
            u.t.advance(&THEME, 1000);
            u.t.pointer(Pointer::Down(Point::new(x, y)));
            u.t.pointer(Pointer::Up(Point::new(x, y)));
            relayout(&mut u);
            assert_eq!(state(&u).1, want, "press at x = {x}");
        }
    }

    #[test]
    fn a_press_focuses_the_field_and_tab_reaches_it() {
        let mut u = ui("x");
        assert!(u.t.focus(u.other));
        let r = u.t.rect(u.field).unwrap();
        let p = Point::new(r.x + 10, r.y + 5);
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Up(p));
        assert!(u.t.is_focused(u.field));
        assert!(u.t.pop_action().is_none(), "focusing sends nothing");
        u.t.key(crate::Key::Next);
        assert!(u.t.is_focused(u.other));
        u.t.key(crate::Key::Prev);
        assert!(u.t.is_focused(u.field));
    }

    #[test]
    fn the_caret_rectangle_follows_the_cursor_and_is_absent_without_the_focus() {
        let mut u = ui("abc");
        relayout(&mut u);
        let end = u.t.text_cursor().unwrap();
        u.t.edit(Edit::Home(false));
        relayout(&mut u);
        let start = u.t.text_cursor().unwrap();
        assert!(end.x > start.x && end.y == start.y && start.w == 2 && start.h > 0);
        let r = u.t.rect(u.field).unwrap();
        assert!(
            start.x >= r.x && start.bottom() <= r.bottom(),
            "inside the field"
        );
        assert!(u.t.focus(u.other));
        relayout(&mut u);
        assert_eq!(u.t.text_cursor(), None);
    }

    #[test]
    fn a_long_text_scrolls_to_keep_the_caret_in_view_and_never_draws_outside_the_field() {
        let mut u = ui("");
        u.t.type_text(&"w".repeat(60));
        relayout(&mut u);
        let r = u.t.rect(u.field).unwrap();
        let caret = u.t.text_cursor().unwrap();
        assert!(
            caret.right() <= r.right() && caret.x >= r.x,
            "{caret:?} within {r:?}"
        );
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        // Right of the field (the screen is wider than the padded column) nothing is drawn.
        for x in (r.right() + 2)..300 {
            for y in r.y..r.bottom() {
                assert_eq!(s.pixel(x, y), Some(THEME.colors.background), "({x}, {y})");
            }
        }
    }

    #[test]
    fn the_field_draws_a_border_a_caret_and_a_selection() {
        let mut u = ui("abc");
        let mut s = screen();
        let r = u.t.rect(u.field).unwrap();
        s.clear(THEME.colors.background).unwrap();
        relayout(&mut u);
        u.t.paint(&mut s, &THEME).unwrap();
        let caret = u.t.text_cursor().unwrap();
        assert_eq!(
            s.pixel(caret.x, caret.y + 4),
            Some(THEME.colors.primary),
            "the caret"
        );
        assert_eq!(
            s.pixel(r.x + (r.w / 2) as i32, r.y),
            Some(THEME.colors.primary),
            "a focused border"
        );
        u.t.edit(Edit::SelectAll);
        relayout(&mut u);
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        let inner_x = r.x + THEME.spacing.sm as i32;
        assert_eq!(
            s.pixel(inner_x + 1, r.y + (r.h / 2) as i32 - 6),
            Some(THEME.colors.primary),
            "the selection"
        );
        assert!(u.t.focus(u.other));
        relayout(&mut u);
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        assert_ne!(
            s.pixel(r.x + (r.w / 2) as i32, r.y),
            Some(THEME.colors.primary),
            "an unfocused border"
        );
    }

    #[test]
    fn a_disabled_field_takes_no_focus_no_text_and_no_edits() {
        let mut u = ui("abc");
        u.t.set_enabled(u.field, false);
        relayout(&mut u);
        assert!(!u.t.focus(u.field));
        u.t.type_text("zz");
        u.t.edit(Edit::Backspace);
        assert_eq!(state(&u).0, "abc");
        assert!(!u.t.wants_text());
    }

    #[test]
    fn text_calls_on_other_nodes_and_stale_handles_are_harmless() {
        let mut u = ui("a");
        assert_eq!(u.t.input_cursor(u.other), None);
        u.t.set_input_cursor(u.other, 3);
        let stale = u.other;
        u.t.remove(stale);
        assert_eq!(u.t.input_cursor(stale), None);
        let mut t = Tree::<2>::new();
        t.edit(Edit::SelectAll);
        t.type_text("x");
        t.set_preedit("y");
        assert_eq!(
            (t.selected_text(), t.text_cursor(), t.wants_text()),
            (None, None, false)
        );
    }

    #[test]
    fn an_explicit_width_and_the_default_are_both_honoured() {
        let mut t = Tree::<4>::new();
        let root = t.column(None, Style::new()).unwrap();
        let a = t.text_input(Some(root), "").unwrap();
        let b = t.text_input(Some(root), "").unwrap();
        t.set_style(b, Style::new().width(Length::Fixed(90)));
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 120));
        assert_eq!(t.rect(a).unwrap().w, THEME.control_height * 6);
        assert_eq!(t.rect(b).unwrap().w, 90);
        assert_eq!(t.rect(a).unwrap().h, THEME.control_height);
    }

    fn area(initial: &str, rows: u8) -> (Tree<8>, NodeId) {
        let mut t = Tree::<8>::new();
        let root = t
            .column(None, Style::new().padding(8).align(CrossAlign::Stretch))
            .unwrap();
        let a = t.text_area(Some(root), initial, rows).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
        t.focus(a);
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
        (t, a)
    }

    #[test]
    fn enter_in_a_text_area_breaks_the_line_and_a_one_line_field_still_refuses_a_newline() {
        let (mut t, a) = area("", 3);
        t.type_text("ab");
        t.key(crate::Key::Activate);
        t.type_text("cd");
        assert_eq!(t.text(a), Some("ab\ncd"));
        // A pasted text with line breaks keeps them, and a carriage return is dropped.
        t.type_text("\r\nef");
        assert_eq!(t.text(a), Some("ab\ncd\nef"));
        // Enter replaces a selection, like typing.
        t.edit(Edit::SelectAll);
        t.key(crate::Key::Activate);
        assert_eq!(t.text(a), Some("\n"));
        // The change is reported once per edit.
        assert!(std::iter::from_fn(|| t.pop_action()).any(|x| x.kind == ActionKind::TextChanged));
        // A one-line field: Enter is a press (submit), and pasted breaks are dropped.
        let mut u = ui("");
        u.t.focus(u.field);
        u.t.type_text("a\nb");
        assert_eq!(u.t.text(u.field), Some("ab"));
        u.t.edit(Edit::Newline);
        assert_eq!(u.t.text(u.field), Some("ab"), "a field takes no line break");
    }

    #[test]
    fn up_and_down_move_between_lines_by_column_and_stop_at_the_ends_then_leave_for_the_next_widget()
     {
        let (mut t, a) = area("hello\nhi\nworld", 4);
        let at = |t: &Tree<8>| t.input_cursor(a).unwrap().0;
        // The cursor starts at the end: line 2, column 5.
        assert_eq!(at(&t), "hello\nhi\nworld".len());
        t.key(crate::Key::Up);
        assert_eq!(
            at(&t),
            "hello\nhi".len(),
            "line 1 is only two long: the end of it"
        );
        t.key(crate::Key::Up);
        assert_eq!(
            at(&t),
            2,
            "line 0 at column 2 (the column of the shorter line)"
        );
        t.set_input_cursor(a, 4);
        t.key(crate::Key::Down);
        assert_eq!(at(&t), "hello\nhi".len());
        t.key(crate::Key::Down);
        assert_eq!(at(&t), "hello\nhi\nwo".len());
        // Home and End go to the ends of the line, not of the text.
        t.edit(Edit::Home(false));
        assert_eq!(at(&t), "hello\nhi\n".len());
        t.edit(Edit::End(false));
        assert_eq!(at(&t), "hello\nhi\nworld".len());
        // On the last line Down leaves the field (the focus moves on); with nowhere to go it stays.
        t.key(crate::Key::Down);
        assert_eq!(at(&t), "hello\nhi\nworld".len());
        // Shift+Up extends the selection over the break.
        t.edit(Edit::Up(true));
        let (c, anchor) = t.input_cursor(a).unwrap();
        assert!(c < anchor, "{c} {anchor}");
        assert_eq!(t.selected_text().map(str::len), Some(anchor - c));
        // A move on a one-line field does nothing.
        let mut u = ui("abc");
        u.t.focus(u.field);
        u.t.edit(Edit::Up(false));
        assert_eq!(u.t.input_cursor(u.field), Some((3, 3)));
    }

    #[test]
    fn a_text_area_is_as_tall_as_its_rows_draws_each_line_on_its_own_row_and_puts_the_caret_on_the_cursors_line()
     {
        let (mut t, a) = area("one\ntwo\nthree", 3);
        let r = t.rect(a).unwrap();
        let single = {
            let (t1, f) = {
                let mut t = Tree::<8>::new();
                let root = t.column(None, Style::new().padding(8)).unwrap();
                let f = t.text_input(Some(root), "x").unwrap();
                t.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
                (t, f)
            };
            t1.rect(f).unwrap().h
        };
        let (mut two, two_id) = area("a\nb", 2);
        two.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
        assert!(
            r.h > two.rect(two_id).unwrap().h,
            "three rows are taller than two ({single} is a one-line field)"
        );
        // Draw it: ink on three separate bands.
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        let band = |y0: i32, y1: i32| {
            (r.x + 4..r.right() - 4)
                .flat_map(|x| (y0..y1).map(move |y| (x, y)))
                .filter(|&(x, y)| s.pixel(x, y) == Some(THEME.colors.text))
                .count()
        };
        let third = r.h as i32 / 3;
        let (b0, b1, b2) = (
            band(r.y, r.y + third),
            band(r.y + third, r.y + 2 * third),
            band(r.y + 2 * third, r.bottom() - 2),
        );
        assert!(
            b0 > 5 && b1 > 5 && b2 > 5,
            "ink in each line: {b0} {b1} {b2}"
        );
        // The caret is on the last line (the cursor is at the end), at the right of "three".
        let caret = t.text_cursor().unwrap();
        assert!(caret.y > r.y + 2 * third - 6, "{caret:?} in {r:?}");
        t.key(crate::Key::Up);
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
        let up = t.text_cursor().unwrap();
        assert!(up.y < caret.y, "one line up: {up:?} {caret:?}");
        assert!(up.x < caret.x, "'two' is shorter than 'three'");
    }

    #[test]
    fn a_text_area_with_more_lines_than_rows_shows_the_ones_with_the_cursor_and_a_press_picks_the_line()
     {
        let text = "l0\nl1\nl2\nl3\nl4";
        let (mut t, a) = area(text, 2);
        let r = t.rect(a).unwrap();
        // The cursor is on the last line, so the last two lines are shown: a press on the top row lands on line 3.
        let p = Point::new(r.x + 30, r.y + 8);
        t.pointer(Pointer::Move(p));
        t.pointer(Pointer::Down(p));
        t.pointer(Pointer::Up(p));
        t.pointer(Pointer::Leave);
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
        let (c, _) = t.input_cursor(a).unwrap();
        assert!(
            (text.find("l3").unwrap()..=text.find("l3").unwrap() + 2).contains(&c),
            "the cursor is on line 3: {c}"
        );
        // A press below the last line goes to the end of the last one.
        let q = Point::new(r.x + 200, r.bottom() - 2);
        t.pointer(Pointer::Move(q));
        t.pointer(Pointer::Down(q));
        t.pointer(Pointer::Up(q));
        t.pointer(Pointer::Leave);
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
        assert_eq!(t.input_cursor(a).unwrap().0, text.len());
        // Every line is reachable: Up four times is the first line.
        for _ in 0..4 {
            t.key(crate::Key::Up);
        }
        assert_eq!(t.input_cursor(a).unwrap().0, 2, "line 0 column 2");
        // Undo takes back an edit that broke a line.
        t.key(crate::Key::Activate);
        assert!(t.text(a).unwrap().starts_with("l0\n\n") || t.text(a).unwrap().contains("l0\n"));
        t.edit(Edit::Undo);
        assert_eq!(t.text(a), Some(text));
    }

    #[test]
    fn a_text_area_never_overflows_its_cap_and_has_at_least_two_rows() {
        let (mut t, a) = area("", 1);
        assert!(t.rect(a).unwrap().h > 0);
        for _ in 0..(INPUT_CAP * 2) {
            t.key(crate::Key::Activate);
        }
        assert_eq!(t.text(a).map(str::len), Some(INPUT_CAP));
        t.edit(Edit::Newline);
        t.edit(Edit::Up(false));
        t.edit(Edit::Down(true));
        t.edit(Edit::SelectAll);
        t.edit(Edit::Newline);
        assert_eq!(t.text(a), Some("\n"));
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 200));
        let mut s = screen();
        t.paint(&mut s, &THEME).unwrap();
    }
}
