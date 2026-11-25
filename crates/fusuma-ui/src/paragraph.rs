//! Wrapped text: a paragraph breaks its text into lines that fit the width it is given, at spaces (a word longer than
//! the line is cut where it no longer fits) and at a newline, and a text view is a paragraph in a scroll container.
//! The lines are worked out with the renderer's own measuring, so they fit on Vello (Parley) and on the integer
//! renderer (glyph atlases) alike; the same code draws them.

use fusuma_render::{Point, Renderer, Size};

use crate::theme::{TextRole, Theme};
use crate::tree::{BuildError, Flags, Kind, NodeId, SmallText, Style, Tree};

/// The text of a paragraph: short text stored inline, or a longer `'static` string (a constant, or a leaked one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Para {
    Inline(SmallText),
    Static(&'static str),
}

impl Para {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Inline(t) => t.as_str(),
            Self::Static(s) => s,
        }
    }
}

/// The lines of `text` that fit `width` pixels, as byte ranges, in order. A line is as many words as fit; a newline
/// ends one; a word wider than `width` is broken after the characters that fit (at least one).
pub(crate) struct Lines<'a, F: Fn(&str) -> u32> {
    text: &'a str,
    at: usize,
    width: u32,
    measure: F,
}

impl<'a, F: Fn(&str) -> u32> Lines<'a, F> {
    /// The lines of `text` at `width`, measured with `measure`.
    pub(crate) fn new(text: &'a str, width: u32, measure: F) -> Self {
        Self::resume(text, 0, width, measure)
    }

    /// The lines of `text` from byte `at` (what [`at`](Self::at) said after a line), so a caller can put the measuring
    /// away between lines.
    pub(crate) fn resume(text: &'a str, at: usize, width: u32, measure: F) -> Self {
        Self {
            text,
            at,
            width,
            measure,
        }
    }

    /// Where the next line starts.
    pub(crate) const fn at(&self) -> usize {
        self.at
    }
}

impl<F: Fn(&str) -> u32> Iterator for Lines<'_, F> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        let text = self.text;
        if self.at > text.len() {
            return None;
        }
        let start = self.at;
        let rest = &text[start..];
        // A hard line break ends the line; what comes after it starts the next.
        let (hard, after) = rest
            .find('\n')
            .map_or((rest.len(), None), |n| (n, Some(n + 1)));
        let line = &rest[..hard];
        let fits = |s: &str| (self.measure)(s) <= self.width;
        let end = if fits(line) {
            hard
        } else {
            // The last space at which the line still fits.
            let mut best = None;
            for (i, c) in line.char_indices() {
                if c == ' ' && i > 0 {
                    if fits(&line[..i]) {
                        best = Some(i);
                    } else {
                        break;
                    }
                }
            }
            match best {
                // Not the spaces before the break: they are not part of the line.
                Some(i) => line[..i].trim_end_matches(' ').len(),
                None => {
                    // One word that is too wide: as many characters as fit, and at least one.
                    let mut cut = line.chars().next().map_or(0, char::len_utf8);
                    for (i, c) in line.char_indices() {
                        let e = i + c.len_utf8();
                        if fits(&line[..e]) {
                            cut = e;
                        } else {
                            break;
                        }
                    }
                    cut
                }
            }
        };
        let end = end
            .max(line.chars().next().map_or(0, char::len_utf8))
            .min(line.len());
        // Where the next line starts: after the spaces that the break ate, or after the newline.
        let mut next = start + end;
        if end == line.len() {
            next = after.map_or(text.len() + 1, |n| start + n);
        } else {
            while text[next..].starts_with(' ') {
                next += 1;
            }
        }
        self.at = next;
        Some((start, start + end))
    }
}

impl<const N: usize> Tree<N> {
    /// Adds a paragraph: `text` (up to [`TEXT_CAP`](crate::TEXT_CAP) bytes) wrapped to the width it gets. Give it a
    /// width (`Length::Fixed`, or a `Fill` or a `Stretch` in its parent) or it takes the width of the screen.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn paragraph(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        role: TextRole,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Paragraph {
                text: Para::Inline(SmallText::new(text)),
                role,
                wrap_w: 0,
            },
            Style::DEFAULT,
        )
    }

    /// Adds a paragraph of a longer text that lives for ever (a constant, or one leaked on purpose).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn paragraph_static(
        &mut self,
        parent: Option<NodeId>,
        text: &'static str,
        role: TextRole,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Paragraph {
                text: Para::Static(text),
                role,
                wrap_w: 0,
            },
            Style::DEFAULT,
        )
    }

    /// Adds a text view: a scroll container (`style` should give it a height) with one paragraph in it, which
    /// scrolls by drag, wheel and keys when the text is taller than the view. Returns the container and the paragraph.
    ///
    /// # Errors
    /// [`BuildError`] when a node cannot be added there.
    pub fn text_view(
        &mut self,
        parent: Option<NodeId>,
        text: &'static str,
        style: Style,
    ) -> Result<(NodeId, NodeId), BuildError> {
        let view = self.scroll(parent, style.align(crate::tree::CrossAlign::Stretch))?;
        let para = self.paragraph_static(Some(view), text, TextRole::Body)?;
        Ok((view, para))
    }

    /// Replaces the text of a paragraph with a `'static` one; the layout is redone.
    pub fn set_paragraph_static(&mut self, id: NodeId, to: &'static str) {
        let Some(i) = self.slot(id) else { return };
        if let Kind::Paragraph { text, .. } = &mut self.nodes[i].kind
            && text.as_str() != to
        {
            *text = Para::Static(to);
            self.request(i, Flags::LAYOUT | Flags::PAINT);
        }
    }

    /// The height of one line of `role`'s text and the space added between lines.
    fn line_metrics<R: Renderer>(r: &R, theme: &Theme, role: TextRole) -> (u32, u32) {
        let h = r.measure_text("Ag", &theme.text_style(role)).h.max(1);
        (h, h / 5)
    }

    /// The size a paragraph wants when wrapped to `wrap` pixels: the widest line and all the lines.
    pub(crate) fn measure_paragraph<R: Renderer>(
        r: &R,
        theme: &Theme,
        text: &str,
        role: TextRole,
        wrap: u32,
    ) -> Size {
        let style = theme.text_style(role);
        let (h, gap) = Self::line_metrics(r, theme, role);
        let measure = |s: &str| r.measure_text(s, &style).w;
        let (mut lines, mut widest) = (0u32, 0u32);
        for (a, b) in Lines::new(text, wrap, measure) {
            lines = lines.saturating_add(1);
            widest = widest.max(measure(&text[a..b]));
        }
        let lines = lines.max(1);
        Size::new(
            widest,
            lines
                .saturating_mul(h)
                .saturating_add((lines - 1).saturating_mul(gap)),
        )
    }

    /// Draws a paragraph, line by line, clipped to its rectangle.
    pub(crate) fn paint_paragraph<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let n = &self.nodes[i];
        let Kind::Paragraph { text, role, .. } = n.kind else {
            return Ok(());
        };
        let inner = n.rect.inset(n.style.padding);
        let style = theme.text_style(role);
        let (h, gap) = Self::line_metrics(r, theme, role);
        let text = text.as_str();
        let mut y = i64::from(inner.y);
        let mut at = 0;
        let clip = n.rect;
        Self::in_clip(r, clip, |r| {
            loop {
                // One line at a time: the renderer is borrowed to measure, then to draw.
                let line = {
                    let measure = |s: &str| r.measure_text(s, &style).w;
                    let mut lines = Lines::resume(text, at, inner.w, measure);
                    let line = lines.next();
                    at = lines.at();
                    line
                };
                let Some((a, b)) = line else { return Ok(()) };
                let top = y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
                // Lines above or below the rectangle are not drawn (a long text in a scroll container).
                let bottom = y + i64::from(h);
                if bottom > i64::from(clip.y) && y < i64::from(clip.bottom()) && b > a {
                    r.draw_text(&text[a..b], Point::new(inner.x, top), &style)?;
                }
                y += i64::from(h) + i64::from(gap);
                if y >= i64::from(clip.bottom()) {
                    return Ok(());
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Color, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::theme::Theme;
    use crate::tree::{CrossAlign, Length};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    /// Six pixels a character, whatever it is: the tests then count in characters.
    fn six(s: &str) -> u32 {
        s.chars().count() as u32 * 6
    }

    fn lines(text: &str, chars: u32) -> Vec<&str> {
        Lines::new(text, chars * 6, six)
            .map(|(a, b)| &text[a..b])
            .collect()
    }

    #[test]
    fn words_fill_a_line_and_the_rest_goes_on_the_next() {
        assert_eq!(lines("the quick brown fox", 10), ["the quick", "brown fox"]);
        assert_eq!(lines("the quick brown fox", 9), ["the quick", "brown fox"]);
        assert_eq!(
            lines("the quick brown fox", 8),
            ["the", "quick", "brown", "fox"]
        );
        assert_eq!(lines("the quick brown fox", 100), ["the quick brown fox"]);
        assert_eq!(lines("", 10), [""], "no text is one empty line");
        // The spaces that a break eats are gone, the ones inside a line stay.
        assert_eq!(lines("aa   bb  cc", 5), ["aa", "bb", "cc"]);
        assert_eq!(lines("aa  bb", 10), ["aa  bb"]);
    }

    #[test]
    fn a_newline_always_ends_a_line_and_a_trailing_one_leaves_an_empty_line() {
        assert_eq!(lines("one\ntwo", 20), ["one", "two"]);
        assert_eq!(lines("one\n\nthree", 20), ["one", "", "three"]);
        assert_eq!(lines("a\n", 20), ["a", ""]);
        assert_eq!(lines("\n", 20), ["", ""]);
        assert_eq!(
            lines("long words wrap\nand so do these", 6),
            ["long", "words", "wrap", "and so", "do", "these"]
        );
    }

    #[test]
    fn a_word_wider_than_the_line_is_cut_and_nothing_loops() {
        assert_eq!(lines("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(
            lines("ab abcdefghij cd", 4),
            ["ab", "abcd", "efgh", "ij", "cd"]
        );
        // No room for even one character: one character a line, and it ends.
        assert_eq!(lines("abc", 0), ["a", "b", "c"]);
        assert_eq!(Lines::new("abc", 0, six).count(), 3);
        // Multi-byte characters are never split.
        assert_eq!(
            lines("éééé 日本語日本語", 3),
            ["ééé", "é", "日本語", "日本語"]
        );
    }

    #[test]
    fn whatever_the_text_and_width_the_lines_cover_it_in_order_and_fit() {
        let mut seed = 99u64;
        let mut next = |n: u32| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((seed >> 33) as u32) % n
        };
        let alphabet = ['a', 'b', 'é', ' ', ' ', '日', '\n', 'x'];
        for _ in 0..300 {
            let text: String = (0..next(60)).map(|_| alphabet[next(8) as usize]).collect();
            let chars = next(12);
            let got: Vec<(usize, usize)> = Lines::new(&text, chars * 6, six).collect();
            assert!(got.len() <= text.chars().count() + 2, "{text:?}: {got:?}");
            let mut last = 0;
            for &(a, b) in &got {
                assert!(a >= last && a <= b && b <= text.len(), "{text:?}: {got:?}");
                last = b;
                let line = &text[a..b];
                assert!(!line.contains('\n'));
                assert!(six(line) <= chars.max(1) * 6, "{line:?} at {chars}");
            }
            // Every character that is not a space or a newline is on some line, in order.
            let kept: String = got
                .iter()
                .flat_map(|&(a, b)| text[a..b].chars())
                .filter(|c| !matches!(c, ' ' | '\n'))
                .collect();
            let want: String = text.chars().filter(|c| !matches!(c, ' ' | '\n')).collect();
            assert_eq!(kept, want, "{text:?} at {chars}");
        }
    }

    fn screen(w: u32, h: u32) -> VelloCpu {
        fusuma_render_vello_cpu::new(w, h).unwrap()
    }

    const TEXT: &str =
        "Fusuma draws the same text on a watch and on a desktop, wrapping it to the room it has";

    #[test]
    fn a_paragraph_is_as_tall_as_its_lines_and_taller_when_narrower() {
        let mut t = Tree::<8>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let p = t
            .paragraph(Some(root), "a few words to wrap onto lines", TextRole::Body)
            .unwrap();
        let line = screen(10, 10)
            .measure_text("Ag", &THEME.text_style(TextRole::Body))
            .h;
        t.update(&screen(10, 10), &THEME, Rect::new(0, 0, 400, 300));
        assert_eq!(t.rect(p).unwrap().h, line, "one line when it fits");
        let mut last = 0;
        for w in [300, 200, 120, 80, 40] {
            t.update(&screen(10, 10), &THEME, Rect::new(0, 0, w, 300));
            let r = t.rect(p).unwrap();
            assert_eq!(r.w, w, "stretched across the column");
            assert!(
                r.h >= last,
                "narrower is never shorter: {} after {last}",
                r.h
            );
            let n = (r.h + line / 5) / (line + line / 5);
            assert!(
                n >= 1 && r.h == n * line + (n - 1) * (line / 5),
                "whole lines: {}",
                r.h
            );
            last = r.h;
        }
        assert!(last > line * 3, "40 px is many lines: {last}");
        assert_eq!(t.text(p), Some("a few words to wrap onto lines"));
        t.set_text(p, "short");
        t.update(&screen(10, 10), &THEME, Rect::new(0, 0, 100, 300));
        assert_eq!(
            t.rect(p).unwrap().h,
            line,
            "the text changed, so did the height"
        );
    }

    #[test]
    fn a_fixed_width_wraps_to_that_width_whatever_the_screen() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let p = t
            .paragraph_static(Some(root), TEXT, TextRole::Body)
            .unwrap();
        t.set_style(p, Style::new().width(Length::Fixed(150)).padding(4));
        t.update(&screen(10, 10), &THEME, Rect::new(0, 0, 600, 600));
        let r = t.rect(p).unwrap();
        assert_eq!(r.w, 150);
        let (line, gap) = (
            screen(10, 10)
                .measure_text("Ag", &THEME.text_style(TextRole::Body))
                .h,
            0,
        );
        let _ = gap;
        assert!(r.h > line * 3, "{TEXT:?} wraps at 142 px: {}", r.h);
    }

    #[test]
    fn it_draws_every_line_inside_its_rectangle_and_nothing_outside() {
        let mut t = Tree::<8>::new();
        let root = t
            .column(None, Style::new().padding(10).align(CrossAlign::Stretch))
            .unwrap();
        let p = t
            .paragraph_static(Some(root), TEXT, TextRole::Body)
            .unwrap();
        let (w, h) = (140u32, 200u32);
        let mut s = screen(w, h);
        t.update(&s, &THEME, Rect::new(0, 0, w, h));
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        let r = t.rect(p).unwrap();
        let bg = THEME.colors.background;
        let inked = |y: i32| (0..w as i32).any(|x| s.pixel(x, y) != Some(bg));
        let line = screen(10, 10)
            .measure_text("Ag", &THEME.text_style(TextRole::Body))
            .h as i32;
        let lines = (r.h as i32 + line / 5) / (line + line / 5);
        assert!(lines >= 3);
        for k in 0..lines {
            let y0 = r.y + k * (line + line / 5);
            assert!((y0..y0 + line).any(inked), "line {k} has ink");
        }
        for y in (0..r.y).chain(r.bottom()..h as i32) {
            assert!(!inked(y), "row {y} outside the paragraph is clean");
        }
        for y in r.y..r.bottom() {
            for x in (0..r.x).chain(r.right()..w as i32) {
                assert_eq!(s.pixel(x, y), Some(bg), "({x}, {y}) is outside it");
            }
        }
    }

    #[test]
    fn a_text_view_scrolls_text_that_is_taller_than_it() {
        let mut t = Tree::<8>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let (view, para) = t
            .text_view(Some(root), TEXT, Style::new().height(Length::Fixed(40)))
            .unwrap();
        let mut s = screen(120, 120);
        t.update(&s, &THEME, Rect::new(0, 0, 120, 120));
        assert!(
            t.scroll_max(view).unwrap() > 20,
            "taller than the view: {:?}",
            t.scroll_max(view)
        );
        let before = t.rect(para).unwrap();
        t.pointer(Pointer::Wheel(fusuma_render::Point::new(10, 10), 25));
        t.update(&s, &THEME, Rect::new(0, 0, 120, 120));
        assert_eq!(t.scroll_offset(view), Some(25));
        assert_eq!(t.rect(para).unwrap().y, before.y - 25);
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        // Nothing is drawn below the view.
        let bg = THEME.colors.background;
        let v = t.rect(view).unwrap();
        for y in v.bottom()..120 {
            assert!((0..120).all(|x| s.pixel(x, y) == Some(bg)), "row {y}");
        }
        let _ = Color::BLACK;
    }

    #[test]
    fn a_stale_handle_and_the_wrong_kind_are_ignored() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let p = t.paragraph(Some(root), "x", TextRole::Body).unwrap();
        let l = t.label(Some(root), "y", TextRole::Body).unwrap();
        t.set_paragraph_static(l, "no");
        assert_eq!(t.text(l), Some("y"));
        t.remove(p);
        t.set_paragraph_static(p, "gone");
        assert_eq!(t.text(p), None);
    }
}
