//! Building blocks made of the basic widgets: a card, a divider, a section heading, a badge, an avatar and a stepper.
//! They add no new kind of node, only a sensible arrangement of rows, columns and labels, so they cost nothing extra in
//! the tree and look the same on every renderer.

use crate::theme::{ButtonVariant, Round, Surface, TextRole};
use crate::tree::{BuildError, CrossAlign, Length, NodeId, Style, Tree};

/// The three parts of a [`stepper`](Tree::stepper).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stepper {
    /// The minus button.
    pub minus: NodeId,
    /// The label that shows the value.
    pub value: NodeId,
    /// The plus button.
    pub plus: NodeId,
}

impl<const N: usize> Tree<N> {
    /// Adds a card: a column on the raised surface (it stands out from a page's panel) with padding and rounded corners. Fill it with rows and labels.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn card(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.column(
            parent,
            style.background(Surface::Raised).round(Round::Medium),
        )
    }

    /// Adds a thin line across its parent (a column's width, a row's height needs a `Fixed` length).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn divider(&mut self, parent: Option<NodeId>) -> Result<NodeId, BuildError> {
        self.row(
            parent,
            Style::new()
                .width(Length::Fill(1))
                .height(Length::Fixed(1))
                .background(Surface::Border),
        )
    }

    /// Adds a small heading that starts a group of widgets.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn section(&mut self, parent: Option<NodeId>, title: &str) -> Result<NodeId, BuildError> {
        self.label(parent, title, TextRole::Caption)
    }

    /// Adds a badge: a short text in a pill on the main action's colour (`primary` false: on the page colour, which stands out from a card).
    ///
    /// # Errors
    /// [`BuildError`] when the nodes cannot be added there.
    pub fn badge(
        &mut self,
        parent: Option<NodeId>,
        text: &str,
        primary: bool,
    ) -> Result<NodeId, BuildError> {
        let pill = self.row(
            parent,
            Style::new()
                .padding(4)
                .align(CrossAlign::Center)
                .background(if primary {
                    Surface::Primary
                } else {
                    Surface::Background
                })
                .round(Round::Full),
        )?;
        self.label(Some(pill), text, TextRole::Caption)?;
        Ok(pill)
    }

    /// Adds an avatar: a circle on the main action's colour with up to two initials in it.
    ///
    /// # Errors
    /// [`BuildError`] when the nodes cannot be added there.
    pub fn avatar(
        &mut self,
        parent: Option<NodeId>,
        initials: &str,
        size: u32,
    ) -> Result<NodeId, BuildError> {
        let c = self.row(
            parent,
            Style::new()
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .align(CrossAlign::Center)
                .background(Surface::Primary)
                .round(Round::Full),
        )?;
        // Springs on both sides keep the initials in the middle.
        self.spacer(Some(c), Style::new().width(Length::Fill(1)))?;
        self.label(Some(c), initials, TextRole::Body)?;
        self.spacer(Some(c), Style::new().width(Length::Fill(1)))?;
        Ok(c)
    }

    /// Adds a stepper: `[-] value [+]`. The buttons send [`ActionKind::Clicked`](crate::ActionKind::Clicked); the app
    /// changes the value and sets the label with [`set_text`](Self::set_text).
    ///
    /// # Errors
    /// [`BuildError`] when the nodes cannot be added there.
    pub fn stepper(&mut self, parent: Option<NodeId>, value: &str) -> Result<Stepper, BuildError> {
        let row = self.row(parent, Style::new().gap(8).align(CrossAlign::Center))?;
        let minus = self.button(Some(row), "-", ButtonVariant::Secondary)?;
        let value = self.label(Some(row), value, TextRole::Body)?;
        let plus = self.button(Some(row), "+", ButtonVariant::Secondary)?;
        Ok(Stepper { minus, value, plus })
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::theme::Theme;
    use crate::tree::ActionKind;

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(240, 200).unwrap()
    }

    fn draw(t: &mut Tree<32>) -> VelloCpu {
        let mut s = screen();
        t.update(&s, &THEME, Rect::new(0, 0, 240, 200));
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        s
    }

    fn page() -> (Tree<32>, NodeId) {
        let mut t = Tree::<32>::new();
        let root = t
            .column(
                None,
                Style::new().padding(8).gap(6).align(CrossAlign::Start),
            )
            .unwrap();
        (t, root)
    }

    #[test]
    fn a_card_is_a_raised_rounded_column_and_a_divider_is_a_one_pixel_line() {
        let (mut t, root) = page();
        let card = t
            .card(
                Some(root),
                Style::new().padding(10).width(Length::Fixed(160)),
            )
            .unwrap();
        t.label(Some(card), "In a card", TextRole::Body).unwrap();
        let line = t.divider(Some(card)).unwrap();
        t.label(Some(card), "Below", TextRole::Body).unwrap();
        let s = draw(&mut t);
        let c = t.rect(card).unwrap();
        assert_eq!(
            s.pixel(c.x + 4, c.y + c.h as i32 / 2),
            Some(THEME.colors.surface_raised)
        );
        // Rounded: the very corner shows the page.
        assert_eq!(s.pixel(c.x, c.y), Some(THEME.colors.background));
        let l = t.rect(line).unwrap();
        assert_eq!(
            (l.h, l.w),
            (1, c.w - 20),
            "one pixel high, as wide as the card inside its padding"
        );
        assert_eq!(s.pixel(l.x + 5, l.y), Some(THEME.colors.border));
    }

    #[test]
    fn a_badge_is_a_pill_and_text_on_the_main_colour_reads() {
        let (mut t, root) = page();
        let hot = t.badge(Some(root), "New", true).unwrap();
        let calm = t.badge(Some(root), "Old", false).unwrap();
        let s = draw(&mut t);
        let (a, b) = (t.rect(hot).unwrap(), t.rect(calm).unwrap());
        assert_eq!(
            s.pixel(a.x + 1, a.y + (a.h / 2) as i32),
            Some(THEME.colors.primary)
        );
        assert_eq!(
            s.pixel(a.x, a.y),
            Some(THEME.colors.background),
            "a pill has no corners"
        );
        // The calm badge is the page colour inside, so it shows on a card: it is not the primary fill.
        assert_ne!(
            s.pixel(b.x + 1, b.y + (b.h / 2) as i32),
            Some(THEME.colors.primary)
        );
        // The text of the primary badge is drawn in the colour made for it, not the muted one.
        let on = (a.y..a.bottom())
            .flat_map(|y| (a.x..a.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| s.pixel(x, y) == Some(THEME.colors.on_primary))
            .count();
        let muted = (a.y..a.bottom())
            .flat_map(|y| (a.x..a.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| s.pixel(x, y) == Some(THEME.colors.text_muted))
            .count();
        assert!(on > 4 && muted == 0, "{on} {muted}");
    }

    #[test]
    fn an_avatar_is_a_circle_with_its_initials_in_the_middle() {
        let (mut t, root) = page();
        let av = t.avatar(Some(root), "AB", 40).unwrap();
        let s = draw(&mut t);
        let r = t.rect(av).unwrap();
        assert_eq!((r.w, r.h), (40, 40));
        assert_eq!(s.pixel(r.x + 3, r.y + 20), Some(THEME.colors.primary));
        assert_eq!(s.pixel(r.x, r.y), Some(THEME.colors.background), "round");
        // The initials' ink is centred, within a few pixels.
        let ink: Vec<(i32, i32)> = (r.y..r.bottom())
            .flat_map(|y| (r.x..r.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| s.pixel(x, y) == Some(THEME.colors.on_primary))
            .collect();
        assert!(!ink.is_empty());
        let mid = ink.iter().map(|p| p.0).sum::<i32>() / ink.len() as i32 - r.x;
        assert!((mid - 20).abs() <= 4, "centre of the ink at {mid} of 40");
    }

    #[test]
    fn a_stepper_has_two_buttons_and_a_value_the_app_sets() {
        let (mut t, root) = page();
        let st = t.stepper(Some(root), "2").unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 240, 200));
        assert_eq!(t.text(st.value), Some("2"));
        let (m, v, p) = (
            t.rect(st.minus).unwrap(),
            t.rect(st.value).unwrap(),
            t.rect(st.plus).unwrap(),
        );
        assert!(
            m.right() <= v.x && v.right() <= p.x,
            "minus, value, plus in a row"
        );
        let tap = |t: &mut Tree<32>, id| {
            let r = t.rect(id).unwrap();
            let at = fusuma_render::Point::new(r.x + 3, r.y + 3);
            for e in [Pointer::Down(at), Pointer::Up(at), Pointer::Leave] {
                t.pointer(e);
            }
        };
        tap(&mut t, st.plus);
        let a = t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (st.plus, ActionKind::Clicked));
        tap(&mut t, st.minus);
        assert_eq!(t.pop_action().map(|a| a.source), Some(st.minus));
        t.set_text(st.value, "3");
        assert_eq!(t.text(st.value), Some("3"));
    }
}
