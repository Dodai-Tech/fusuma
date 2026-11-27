//! The paint pass: the tree and the theme draw through a [`Renderer`].

use fusuma_anim::{Easing, Fx};
use fusuma_render::{Color, Point, Rect, Renderer, Size};

use crate::theme::{ButtonState, ButtonStyle, Theme};
use crate::tree::{Flags, Kind, Tree};

/// A colour part-way between two, in whole channel steps.
fn mix(a: Color, b: Color, t: Fx) -> Color {
    let ch = |x: u8, y: u8| {
        Fx::from_int(i32::from(x))
            .lerp(Fx::from_int(i32::from(y)), t)
            .round()
            .clamp(0, 255) as u8
    };
    Color::rgba(ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b), ch(a.a, b.a))
}

/// An outline that may be absent at either end of a fade: it switches half way.
fn mix_border(a: Option<Color>, b: Option<Color>, t: Fx) -> Option<Color> {
    match (a, b) {
        (Some(x), Some(y)) => Some(mix(x, y, t)),
        (None, None) => None,
        _ => {
            if t < Fx::HALF {
                a
            } else {
                b
            }
        }
    }
}

/// The left and top of something `inner` big, centred in `outer`.
pub(crate) fn centred(outer: Rect, inner: Size) -> Point {
    let c = |o: u32, i: u32| (i64::from(o) - i64::from(i)) / 2;
    let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    Point::new(
        clamp(i64::from(outer.x) + c(outer.w, inner.w)),
        clamp(i64::from(outer.y) + c(outer.h, inner.h)),
    )
}

impl<const N: usize> Tree<N> {
    /// Draws every node, parents before children, in the theme's colours. It does not clear the
    /// surface: call [`Renderer::clear`] first when the whole frame is redrawn. Afterwards no node
    /// is waiting for a redraw.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn paint<R: Renderer>(&mut self, renderer: &mut R, theme: &Theme) -> Result<(), R::Error> {
        let Some(root) = self.root.map(usize::from) else {
            return Ok(());
        };
        // Everything is drawn: nothing is left of a shift to make.
        self.shift = None;
        self.paint_walk(root, renderer, theme, None)?;
        self.paint_dialogs(renderer, theme, None)?;
        self.paint_toasts(renderer, theme, None)?;
        self.paint_drag(renderer, theme, None)?;
        self.clear_flag(Flags::PAINT);
        self.damage = None;
        self.pieces.clear();
        self.rest = None;
        Ok(())
    }

    /// Whether node `i` is inside (a descendant of) node `ancestor`.
    pub(crate) fn is_inside(&self, i: usize, ancestor: usize) -> bool {
        let mut cur = self.nodes[i].parent.map(usize::from);
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.nodes[p].parent.map(usize::from);
        }
        false
    }

    /// Draws the nodes from `root` on, parents before children. A scroll container clips what is inside it
    /// and draws its scrollbar over it when its children are done. With a `region`, nodes that do not touch
    /// it are skipped.
    pub(crate) fn paint_walk<R: Renderer>(
        &self,
        root: usize,
        r: &mut R,
        theme: &Theme,
        region: Option<Rect>,
    ) -> Result<(), R::Error> {
        // Scroll containers whose children are being drawn, outermost first. Deeper nesting than this is
        // drawn without its own clip (the outer ones still clip).
        const OPEN: usize = 8;
        let mut open = [0usize; OPEN];
        let mut depth = 0;
        let mut result = Ok(());
        let mut cur = Some(root);
        while let Some(i) = cur {
            while depth > 0 && !self.is_inside(i, open[depth - 1]) {
                depth -= 1;
                let done = self.end_scroll(open[depth], r, theme);
                result = result.and(done);
            }
            // A hidden node, and all it holds, is not drawn.
            if i != root && self.nodes[i].hidden {
                cur = self.after_subtree(i, root);
                continue;
            }
            // A dialog is drawn after everything else, on top (see `paint_dialogs`), not here in its tree order.
            if i != root && self.nodes[i].kind.out_of_flow() {
                cur = self.after_subtree(i, root);
                continue;
            }
            let seen = region.is_none_or(|g| {
                let o = self.nodes[i].rect.intersect(&g);
                o.w > 0 && o.h > 0
            });
            if result.is_ok() && seen {
                result = self.paint_node(i, r, theme);
            }
            if result.is_ok() && depth < OPEN && matches!(self.nodes[i].kind, Kind::Scroll { .. }) {
                result = r.save().and_then(|()| r.clip(self.nodes[i].rect));
                open[depth] = i;
                depth += 1;
            }
            // What lies over a surface is drawn after the host has filled it (see `paint_over`).
            if i != root && self.host_surfaces && matches!(self.nodes[i].kind, Kind::Surface { .. })
            {
                cur = self.after_subtree(i, root);
                continue;
            }
            cur = self.preorder_next(i, root);
        }
        while depth > 0 {
            depth -= 1;
            let done = self.end_scroll(open[depth], r, theme);
            result = result.and(done);
        }
        result
    }

    /// Draws the scrollbar of container `i` (inside its clip) and ends the clip.
    fn end_scroll<R: Renderer>(&self, i: usize, r: &mut R, theme: &Theme) -> Result<(), R::Error> {
        let n = &self.nodes[i];
        let mut result = Ok(());
        if let Kind::Scroll {
            offset,
            content,
            bar: shown,
            ..
        } = n.kind
        {
            let view = n.rect.inset(n.style.padding).h;
            let alpha = crate::scrollbar::bar_alpha(shown, theme);
            let color = crate::scrollbar::bar_color(theme, alpha, r.capabilities().alpha);
            if let Some(color) = color.filter(|_| content > view && view > 0) {
                let track = n.rect;
                let thumb = (u64::from(view) * u64::from(view) / u64::from(content)) as u32;
                let thumb = thumb.max(theme.track_height * 2).min(view);
                let range = u64::from(view - thumb);
                let max = u64::from(content - view);
                let at = (range * offset.max(0) as u64 / max).min(range) as u32;
                let bar = theme.track_height.clamp(2, 6);
                let x = track
                    .right()
                    .saturating_sub(i32::try_from(bar).unwrap_or(0) + 2);
                let y = track.y.saturating_add(
                    i32::try_from(n.style.padding.saturating_add(at)).unwrap_or(i32::MAX),
                );
                result = r.fill_rect(Rect::new(x, y, bar, thumb), bar / 2, color);
            }
        }
        let restored = r.restore();
        result.and(restored)
    }

    /// Draws only the part of the tree that overlaps `region`, limited to it: the way to redraw what
    /// [`take_damage`](Self::take_damage) reported without touching the rest of the screen.
    ///
    /// The caller prepares the surface: the renderer's coordinates are the tree's (translate it first
    /// if the surface is just the region), and the region must be cleared to the background before,
    /// because widgets draw over what is behind them (`Renderer::clear` inside a clip does that).
    /// If `region` covers all the damage (the usual case: it is what `take_damage` returned) nothing
    /// is waiting for a redraw afterwards. If it covers less, the damage and the redraw requests stay,
    /// so nothing is lost.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn paint_region<R: Renderer>(
        &mut self,
        renderer: &mut R,
        theme: &Theme,
        region: Rect,
    ) -> Result<(), R::Error> {
        let Some(root) = self.root.map(usize::from) else {
            return Ok(());
        };
        if region.is_empty() {
            return Ok(());
        }
        renderer.save()?;
        let mut result = renderer.clip(region);
        if result.is_ok() {
            result = self.paint_walk(root, renderer, theme, Some(region));
        }
        if result.is_ok() {
            result = self.paint_dialogs(renderer, theme, Some(region));
        }
        if result.is_ok() {
            result = self.paint_toasts(renderer, theme, Some(region));
        }
        if result.is_ok() {
            result = self.paint_drag(renderer, theme, Some(region));
        }
        let restored = renderer.restore();
        result?;
        restored?;
        // Everything that changed lies inside the region: nothing is left to redraw.
        let covered = self.damage.is_none_or(|d| region.intersect(&d) == d);
        if covered {
            self.clear_flag(Flags::PAINT);
            self.damage = None;
            self.pieces.clear();
            self.rest = None;
        }
        Ok(())
    }

    /// The look of a button right now.
    fn button_look(&self, i: usize, theme: &Theme) -> Option<ButtonStyle> {
        let n = &self.nodes[i];
        let Kind::Button { variant, blend, .. } = n.kind else {
            return None;
        };
        let class = n.class;
        Some(if n.disabled_eff {
            theme.button_in(class, variant, ButtonState::Disabled)
        } else if n.active {
            // A press shows at once; only the hover fades.
            theme.button_in(class, variant, ButtonState::Pressed)
        } else {
            let rest = theme.button_in(class, variant, ButtonState::Idle);
            let hover = theme.button_in(class, variant, ButtonState::Hovered);
            let t = Easing::CubicOut.apply(blend);
            ButtonStyle {
                background: mix(rest.background, hover.background, t),
                border: mix_border(rest.border, hover.border, t),
                text: mix(rest.text, hover.text, t),
                radius: rest.radius,
            }
        })
    }

    fn paint_node<R: Renderer>(&self, i: usize, r: &mut R, theme: &Theme) -> Result<(), R::Error> {
        let n = &self.nodes[i];
        let rect = n.rect;
        if rect.is_empty() {
            return Ok(());
        }
        match n.kind {
            Kind::Stack(_) | Kind::Flow { .. } | Kind::Scroll { .. } => {
                if let Some(color) = theme.surface_in(n.class, n.style.background) {
                    r.fill_rect(rect, theme.radius(n.style.round), color)?;
                    // A card gets an outline and ticks in the corners.
                    if (theme.decor.ticks != 0 || theme.decor.lattice != 0)
                        && matches!(
                            n.style.background,
                            crate::theme::Surface::Surface | crate::theme::Surface::Raised
                        )
                    {
                        r.stroke_rect(
                            rect,
                            theme.radius(n.style.round),
                            theme.border_width,
                            theme.colors.border,
                        )?;
                        crate::decor::ticks(
                            r,
                            rect,
                            theme.decor.ticks,
                            theme.border_width,
                            theme.colors.border_strong,
                        )?;
                        crate::decor::lattice(
                            r,
                            rect,
                            theme.decor.lattice,
                            crate::theme::mix(color, theme.colors.border, 800),
                        )?;
                    }
                }
                Ok(())
            }
            Kind::Spacer | Kind::Surface { .. } | Kind::Toast { .. } => Ok(()),
            Kind::Dialog { open, .. } => {
                if open {
                    self.paint_dialog_panel(i, r, theme)
                } else {
                    Ok(())
                }
            }
            Kind::Checkbox { .. }
            | Kind::Radio { .. }
            | Kind::ListItem { .. }
            | Kind::Progress { .. }
            | Kind::Gauge { .. }
            | Kind::Spinner { .. }
            | Kind::Switch { .. }
            | Kind::Slider { .. }
            | Kind::Splitter { .. }
            | Kind::Image { .. }
            | Kind::Chart { .. }
            | Kind::TextInput { .. } => self.paint_widget(i, r, theme),
            Kind::Label { text, role } => {
                let mut style = theme.text_style(role);
                // On the main action's colour (a badge, an avatar) or a solid tone (an alert, a tag) text is in the colour
                // made to read on it.
                if let Some(c) = n
                    .parent
                    .and_then(|p| theme.text_on(self.nodes[usize::from(p)].style.background))
                {
                    style.color = c;
                }
                let size = r.measure_text(text.as_str(), &style);
                let at = Point::new(rect.inset(n.style.padding).x, centred(rect, size).y);
                Self::in_clip(r, rect, |r| r.draw_text(text.as_str(), at, &style))
            }
            Kind::Tab { .. } => self.paint_tab(i, r, theme),
            Kind::Paragraph { .. } => self.paint_paragraph(i, r, theme),
            Kind::Button { text, icon, .. } => {
                let Some(look) = self.button_look(i, theme) else {
                    return Ok(());
                };
                r.fill_rect(rect, look.radius, look.background)?;
                if let Some(border) = look.border {
                    r.stroke_rect(rect, look.radius, theme.border_width, border)?;
                }
                let m = Self::measure_icon_text(r, theme, icon, text.as_str());
                let x = centred(rect, m.size()).x;
                Self::paint_icon_text(r, theme, rect, x, m, text.as_str(), look.text)?;
                self.ring(i, r, theme, look.radius)
            }
        }
    }

    /// Runs `draw` limited to `rect`, and always puts the clip back.
    pub(crate) fn in_clip<R: Renderer>(
        r: &mut R,
        rect: Rect,
        draw: impl FnOnce(&mut R) -> Result<(), R::Error>,
    ) -> Result<(), R::Error> {
        r.save()?;
        let result = r.clip(rect).and_then(|()| draw(r));
        r.restore()?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, Round, Surface, TextRole};
    use crate::tree::{CrossAlign, Length, NodeId, Style};
    use fusuma_render::{Probe, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 160;
    const H: u32 = 120;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    fn app() -> (Tree<8>, NodeId, NodeId, NodeId) {
        let mut t = Tree::<8>::new();
        let root = t
            .column(
                None,
                Style::new()
                    .padding(10)
                    .gap(8)
                    .align(CrossAlign::Start)
                    .background(Surface::Surface)
                    .round(Round::Medium),
            )
            .unwrap();
        let title = t.label(Some(root), "Hi", TextRole::Title).unwrap();
        let primary = t.button(Some(root), "OK", ButtonVariant::Primary).unwrap();
        t.set_style(primary, Style::new().width(Length::Fixed(100)));
        let secondary = t
            .button(Some(root), "No", ButtonVariant::Secondary)
            .unwrap();
        t.set_style(secondary, Style::new().width(Length::Fixed(100)));
        (t, title, primary, secondary)
    }

    fn frame(t: &mut Tree<8>, s: &mut VelloCpu) {
        t.update(s, &THEME, Rect::new(0, 0, W, H));
        s.clear(THEME.colors.background).unwrap();
        t.paint(s, &THEME).unwrap();
    }

    /// A point inside a button, left of its centred label, away from the rounded corners.
    fn edge_of(t: &Tree<8>, id: NodeId) -> (i32, i32) {
        let r = t.rect(id).unwrap();
        (r.x + 8, r.y + (r.h / 2) as i32)
    }

    #[test]
    fn paints_containers_and_buttons_in_the_theme_colours() {
        let (mut t, _, primary, secondary) = app();
        let mut s = screen();
        frame(&mut t, &mut s);
        let at = |s: &VelloCpu, (x, y)| s.pixel(x, y).unwrap();
        assert_eq!(at(&s, edge_of(&t, primary)), THEME.colors.primary);
        assert_eq!(at(&s, edge_of(&t, secondary)), THEME.colors.surface_raised);
        // Inside the container, between the widgets, and outside of it.
        let panel = t.rect(t.root().unwrap()).unwrap();
        assert_eq!(
            s.pixel(panel.x + 5, panel.y + 50).unwrap(),
            THEME.colors.surface
        );
        assert_eq!(
            s.pixel(0, 0).unwrap(),
            THEME.colors.background,
            "the panel's rounded corner shows the page behind it"
        );
    }

    #[test]
    fn a_secondary_button_has_an_outline_and_a_primary_one_has_none() {
        let (mut t, _, primary, secondary) = app();
        let mut s = screen();
        frame(&mut t, &mut s);
        let sr = t.rect(secondary).unwrap();
        assert_eq!(
            s.pixel(sr.x + (sr.w / 2) as i32, sr.y).unwrap(),
            THEME.colors.border
        );
        let pr = t.rect(primary).unwrap();
        assert_eq!(
            s.pixel(pr.x + (pr.w / 2) as i32 + 30, pr.y).unwrap(),
            THEME.colors.primary
        );
    }

    #[test]
    fn the_label_text_is_drawn_inside_its_box() {
        let (mut t, title, _, _) = app();
        let mut s = screen();
        frame(&mut t, &mut s);
        let r = t.rect(title).unwrap();
        let lit = |c: Color| c == THEME.colors.text;
        let inside =
            (r.x..r.right()).any(|x| (r.y..r.bottom()).any(|y| lit(s.pixel(x, y).unwrap())));
        assert!(inside, "text pixels inside {r:?}");
        let below = (r.x..r.right())
            .any(|x| (r.bottom()..r.bottom() + 4).any(|y| lit(s.pixel(x, y).unwrap())));
        assert!(!below);
    }

    #[test]
    fn a_press_shows_at_once_and_a_hover_fades_in() {
        let (mut t, _, primary, _) = app();
        let mut s = screen();
        frame(&mut t, &mut s);
        let spot = edge_of(&t, primary);
        let pr = t.rect(primary).unwrap();
        let centre = Point::new(pr.x + (pr.w / 2) as i32, pr.y + (pr.h / 2) as i32);
        t.pointer(Pointer::Move(centre));
        t.advance(&THEME, THEME.motion.fast_ms / 3);
        frame(&mut t, &mut s);
        let mid = s.pixel(spot.0, spot.1).unwrap();
        assert_ne!(mid, THEME.colors.primary);
        assert_ne!(mid, THEME.colors.primary_hover, "part-way: neither end");
        t.advance(&THEME, 1000);
        frame(&mut t, &mut s);
        assert_eq!(s.pixel(spot.0, spot.1).unwrap(), THEME.colors.primary_hover);
        t.pointer(Pointer::Down(centre));
        frame(&mut t, &mut s);
        assert_eq!(
            s.pixel(spot.0, spot.1).unwrap(),
            THEME.colors.primary_active,
            "no waiting for the fade"
        );
    }

    #[test]
    fn a_disabled_button_looks_disabled() {
        let (mut t, _, primary, _) = app();
        t.set_enabled(primary, false);
        let mut s = screen();
        frame(&mut t, &mut s);
        assert_eq!(
            s.pixel(edge_of(&t, primary).0, edge_of(&t, primary).1)
                .unwrap(),
            THEME.colors.surface
        );
    }

    #[test]
    fn painting_clears_the_redraw_request_and_changes_ask_for_a_new_one() {
        let (mut t, title, _, _) = app();
        let mut s = screen();
        assert!(t.needs_frame());
        frame(&mut t, &mut s);
        assert!(!t.needs_frame(), "everything is drawn");
        t.set_text(title, "Hello");
        assert!(t.needs_frame());
        frame(&mut t, &mut s);
        assert!(!t.needs_frame());
    }

    #[test]
    fn a_light_theme_paints_light_colours() {
        let (mut t, _, primary, _) = app();
        let mut s = screen();
        t.update(&s, &Theme::LIGHT, Rect::new(0, 0, W, H));
        s.clear(Theme::LIGHT.colors.background).unwrap();
        t.paint(&mut s, &Theme::LIGHT).unwrap();
        let (x, y) = edge_of(&t, primary);
        assert_eq!(s.pixel(x, y).unwrap(), Theme::LIGHT.colors.primary);
    }

    #[test]
    fn mixing_colours_and_outlines() {
        let (a, b) = (Color::rgb(0, 100, 255), Color::rgb(200, 100, 55));
        assert_eq!(mix(a, b, Fx::ZERO), a);
        assert_eq!(mix(a, b, Fx::ONE), b);
        assert_eq!(mix(a, b, Fx::HALF), Color::rgb(100, 100, 155));
        assert_eq!(
            mix(a, b, Fx::from_int(9)),
            Color::rgb(255, 100, 0),
            "clamped to a valid colour"
        );
        assert_eq!(mix_border(None, None, Fx::HALF), None);
        assert_eq!(mix_border(Some(a), None, Fx::from_ratio(1, 4)), Some(a));
        assert_eq!(mix_border(Some(a), None, Fx::from_ratio(3, 4)), None);
        assert_eq!(
            mix_border(Some(a), Some(b), Fx::HALF),
            Some(Color::rgb(100, 100, 155))
        );
    }

    #[test]
    fn empty_and_off_screen_trees_paint_without_error() {
        let mut empty = Tree::<2>::new();
        empty.paint(&mut screen(), &THEME).unwrap();
        let (mut t, _, _, _) = app();
        let mut s = screen();
        t.update(&s, &THEME, Rect::new(i32::MAX - 5, i32::MIN, 40, 40));
        t.paint(&mut s, &THEME).unwrap();
        t.update(&s, &THEME, Rect::new(0, 0, 0, 0));
        t.paint(&mut s, &THEME).unwrap();
    }

    #[test]
    fn the_integer_and_the_anti_aliased_backends_agree_on_layout_and_flat_colour() {
        let (mut a, _, primary, secondary) = app();
        let (mut b, ..) = app();
        let mut small = screen();
        frame(&mut a, &mut small);
        let mut vello = fusuma_render_vello_cpu::new(W, H).unwrap();
        b.update(&vello, &THEME, Rect::new(0, 0, W, H));
        vello.clear(THEME.colors.background).unwrap();
        b.paint(&mut vello, &THEME).unwrap();
        // Text is measured by the same fonts, so the layouts are identical.
        for id in [primary, secondary] {
            assert_eq!(a.rect(id), b.rect(id));
        }
        let near = |p: Color, q: Color| {
            p.r.abs_diff(q.r) <= 10 && p.g.abs_diff(q.g) <= 10 && p.b.abs_diff(q.b) <= 10
        };
        for id in [primary, secondary] {
            let (x, y) = edge_of(&a, id);
            assert!(
                near(small.pixel(x, y).unwrap(), vello.pixel(x, y).unwrap()),
                "{x},{y}"
            );
        }
    }

    #[test]
    fn invalidating_repaints_everything_and_an_empty_tree_ignores_it() {
        let (mut t, ..) = app();
        let mut s = screen();
        frame(&mut t, &mut s);
        assert!(!t.needs_frame() && t.take_damage().is_none());
        t.invalidate();
        assert!(t.needs_frame());
        assert_eq!(t.take_damage(), t.rect(t.root().unwrap()));
        Tree::<2>::new().invalidate();
    }
}
