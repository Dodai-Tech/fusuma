//! E-paper and other reflective panels: a picture of black and white (or a few grays) that is slow to change.
//!
//! The app draws as it always does, on the integer renderer or on Vello. What differs is what happens after:
//!
//! * the RGB565 picture is **quantised** to one bit a pixel ([`pack_mono`]), by a threshold (crisp text) or an ordered
//!   dither (photos, grays), and packed the way every e-paper controller takes it: eight pixels a byte, most
//!   significant bit first, a set bit is white;
//! * what changed is **batched**: a [`Planner`] collects the damage of many frames into one rectangle and shows it
//!   when the panel may refresh (a minimum interval, a short settle time so a burst of changes is one refresh), and a
//!   **full refresh** now and then clears the ghosts that partial refreshes leave;
//! * a [`Panel`] sends the bytes to the controller ([`ssd1680`] is one, behind a bus the chip's HAL implements).
//!
//! [`EinkDriver`] runs all of it for an app: it tells the app what the glass is ([`fusuma::Input::Screen`]), so the
//! theme turns to black and white and the motion stops without the app asking.

use alloc::vec;
use alloc::vec::Vec;

use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::{App, DisplayKind, Input, Screen, Shell};

use crate::board::Pixels;
use crate::driver::DriverError;
use crate::surface::Surface;

pub mod bus;
pub mod ssd1677;
pub mod ssd1680;
pub mod ssd2677;
pub mod sticky;

/// How a gray becomes black or white.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dither {
    /// Black below the level (0 to 255), white from it: text and lines stay crisp. The default, at 128.
    Threshold(u8),
    /// A 4x4 ordered dither (Bayer): a gray is a pattern of dots, the same pattern wherever the pixel is, so a partial
    /// refresh matches the picture around it.
    Ordered,
}

impl Default for Dither {
    fn default() -> Self {
        Self::Threshold(128)
    }
}

/// The brightness of an RGB565 pixel, 0 to 255 (Rec. 601 weights).
#[must_use]
pub const fn luma(p: u16) -> u8 {
    let r = ((p >> 11) & 0x1F) as u32;
    let g = ((p >> 5) & 0x3F) as u32;
    let b = (p & 0x1F) as u32;
    // Each channel spread to 8 bits, weights 77, 150, 29 of 256.
    let (r, g, b) = (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    );
    ((77 * r + 150 * g + 29 * b) >> 8) as u8
}

const BAYER4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

impl Dither {
    /// Whether the pixel at `(x, y)` with brightness `luma` is white.
    #[must_use]
    pub const fn white(self, x: u32, y: u32, luma: u8) -> bool {
        match self {
            Self::Threshold(level) => luma >= level,
            Self::Ordered => {
                let t = BAYER4[(y & 3) as usize][(x & 3) as usize] as u32 * 16 + 8;
                luma as u32 >= t
            }
        }
    }
}

/// `region` cut to a `width` x `height` screen and grown to whole bytes in x (columns in multiples of 8), the way
/// e-paper controllers address their memory. `None` when nothing of it is on the screen. The right edge may go past a
/// screen whose width is not a multiple of 8 (122 pixels: 16 bytes); [`pack_mono`] fills the extra pixels white.
#[must_use]
pub fn byte_region(region: Rect, width: u32, height: u32) -> Option<Rect> {
    let r = crate::aligned(region, width, height, 1)?;
    let x0 = r.x & !7;
    let x1 = (i64::from(r.x) + i64::from(r.w) + 7) & !7;
    Some(Rect::new(
        x0,
        r.y,
        u32::try_from(x1 - i64::from(x0)).ok()?,
        r.h,
    ))
}

/// The bytes of one row of `region` in a packed picture (`region.w / 8`, `region` being [`byte_region`] output).
#[must_use]
pub const fn row_bytes(region: Rect) -> usize {
    (region.w as usize).div_ceil(8)
}

/// Packs `region` of the picture in `src` into `out`, one bit a pixel, eight to a byte, most significant bit first, a set
/// bit is white, rows one after the other. `region` is [`byte_region`] output (`x` and `w` multiples of 8): pixels past
/// the picture's edge are white. `out` is cleared first and ends up `row_bytes(region) * region.h` long.
pub fn pack_mono<P: Pixels + ?Sized>(
    src: &mut P,
    screen: (u32, u32),
    region: Rect,
    dither: Dither,
    out: &mut Vec<u8>,
) {
    out.clear();
    if region.w == 0 || region.h == 0 || region.x < 0 || region.y < 0 {
        return;
    }
    let (x0, y0) = (region.x as u32, region.y as u32);
    let bytes = row_bytes(region);
    out.reserve(bytes * region.h as usize);
    // The part of a row that is on the screen; the rest of the last byte is paper.
    let on = region.w.min(screen.0.saturating_sub(x0)) as usize;
    let mut line = vec![0u16; on];
    for row in 0..region.h {
        let y = y0 + row;
        if y >= screen.1 {
            out.extend(core::iter::repeat_n(0xFF, bytes));
            continue;
        }
        src.row(y, x0, &mut line);
        for byte in 0..bytes {
            let mut b = 0u8;
            for bit in 0..8 {
                let i = byte * 8 + bit;
                let white = line
                    .get(i)
                    .is_none_or(|&p| dither.white(x0 + i as u32, y, luma(p)));
                b = (b << 1) | u8::from(white);
            }
            out.push(b);
        }
    }
}

/// When a reflective panel may refresh, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefreshPolicy {
    /// The least time between two refreshes, in milliseconds: the panel is still busy before (partial 150 to 500 for
    /// small panels, a full one 1 to 4 seconds).
    pub min_interval_ms: u32,
    /// Changes closer together than this are one refresh: the refresh waits until the screen has been quiet this long
    /// (a typed word is one update, not five). 0 shows at once.
    pub settle_ms: u32,
    /// A full refresh (the whole screen, with the flashing that clears ghosts) after this many partial ones; 0 never.
    pub full_every: u32,
    /// How to turn grays to black and white.
    pub dither: Dither,
}

impl RefreshPolicy {
    /// What most black and white panels want: 300 ms apart, a 100 ms settle, a clean-up every 30 refreshes.
    pub const DEFAULT: Self = Self {
        min_interval_ms: 300,
        settle_ms: 100,
        full_every: 30,
        dither: Dither::Threshold(128),
    };
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What to show now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// The whole screen, with the clean-up flashing.
    Full,
    /// This part, byte-aligned ([`byte_region`]), without the flashing.
    Partial(Rect),
}

/// Decides when and what a reflective panel shows: it is told what changed ([`damage`](Self::damage)), asked what to
/// do ([`take`](Self::take)), and says when to ask again ([`next_at`](Self::next_at)). It never touches the panel,
/// so it is the same on every chip and tested without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planner {
    policy: RefreshPolicy,
    screen: (u32, u32),
    pending: Option<Rect>,
    last_damage: u64,
    last_refresh: Option<u64>,
    partials: u32,
    dirty_full: bool,
}

impl Planner {
    /// A planner for a `width` x `height` panel. The first refresh is a full one: nobody knows what is on the glass.
    #[must_use]
    pub const fn new(screen: (u32, u32), policy: RefreshPolicy) -> Self {
        Self {
            policy,
            screen,
            pending: None,
            last_damage: 0,
            last_refresh: None,
            partials: 0,
            dirty_full: true,
        }
    }

    /// The policy.
    #[must_use]
    pub const fn policy(&self) -> RefreshPolicy {
        self.policy
    }

    /// Something in `region` changed at `now_ms`. Changes pile up into one rectangle.
    pub fn damage(&mut self, region: Rect, now_ms: u64) {
        if let Some(r) = byte_region(region, self.screen.0, self.screen.1) {
            self.pending = Some(self.pending.map_or(r, |p| p.union(&r)));
            self.last_damage = now_ms;
        }
    }

    /// Whether anything waits to be shown.
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending.is_some() || self.dirty_full
    }

    /// Asks for a full refresh at the next opportunity (after a sleep, or when the picture is known to be wrong).
    pub fn want_full(&mut self) {
        self.dirty_full = true;
    }

    /// The time (ms) at which [`take`](Self::take) would give something, or `None` when nothing waits.
    #[must_use]
    pub fn next_at(&self) -> Option<u64> {
        if !self.pending() {
            return None;
        }
        let settled = if self.pending.is_some() {
            self.last_damage + u64::from(self.policy.settle_ms)
        } else {
            0
        };
        let free = self
            .last_refresh
            .map_or(0, |t| t + u64::from(self.policy.min_interval_ms));
        Some(settled.max(free))
    }

    /// What to show at `now_ms`, if anything: the pending damage as a partial refresh, or the whole screen when one is
    /// wanted (the first time, after `full_every` partial ones, or [`want_full`](Self::want_full)). Taking it makes it
    /// shown: the next change starts a new batch.
    pub fn take(&mut self, now_ms: u64) -> Option<Refresh> {
        if self.next_at()? > now_ms {
            return None;
        }
        self.last_refresh = Some(now_ms);
        let full_due = self.policy.full_every > 0 && self.partials >= self.policy.full_every;
        if self.dirty_full || full_due {
            self.dirty_full = false;
            self.pending = None;
            self.partials = 0;
            return Some(Refresh::Full);
        }
        self.partials += 1;
        self.pending.take().map(Refresh::Partial)
    }
}

/// A reflective panel: it takes packed black and white rows and shows them. A chip's runtime implements it over its
/// SPI bus (see [`ssd1680`]).
pub trait Panel {
    /// What goes wrong talking to the panel.
    type Error: core::fmt::Debug;

    /// The size in pixels, the way the app is laid out.
    fn size(&self) -> (u32, u32);

    /// The whole picture (`row_bytes * height` bytes, a set bit white), shown with the full, flashing refresh.
    ///
    /// # Errors
    /// The bus error.
    fn full(&mut self, frame: &[u8]) -> Result<(), Self::Error>;

    /// `region` (from [`byte_region`]) of the picture, `data` packed as [`pack_mono`] does, shown without flashing.
    ///
    /// # Errors
    /// The bus error.
    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), Self::Error>;

    /// Puts the panel to deep sleep: it keeps the picture with no power. The next refresh wakes it.
    ///
    /// # Errors
    /// The bus error.
    fn sleep(&mut self) -> Result<(), Self::Error>;

    /// What this panel is, for the app: reflective, one bit, the refresh it takes. The default is a plain 1-bit
    /// e-paper of its size; a panel with more levels or a different refresh says so.
    fn screen(&self) -> Screen {
        let (w, h) = self.size();
        Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Micro)
            .with_glass(DisplayKind::Reflective, 1, 300)
    }
}

/// What one [`EinkDriver::step`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EinkStep {
    /// What was sent to the panel, if anything.
    pub refreshed: Option<Refresh>,
    /// When to step again.
    pub wake: fusuma::Wake,
}

/// An app, a reflective panel and the surface the app draws on: the e-paper twin of [`Driver`](crate::Driver).
#[derive(Debug)]
pub struct EinkDriver<A, P> {
    shell: Shell<A>,
    panel: P,
    surface: Surface,
    planner: Planner,
    packed: Vec<u8>,
}

impl<A: App, P: Panel> EinkDriver<A, P> {
    /// Runs `app` on `panel` with `backend` (the integer renderer or Vello on the CPU). `None` when the surface cannot
    /// be made. The app is told what the panel is before its first frame.
    pub fn new(app: A, panel: P, backend: Backend, policy: RefreshPolicy) -> Option<Self> {
        let (w, h) = panel.size();
        let surface = Surface::new(backend, w, h)?;
        let mut shell = Shell::new(app);
        shell.input(Input::Screen(panel.screen()));
        Some(Self {
            shell,
            panel,
            surface,
            planner: Planner::new((w, h), policy),
            packed: Vec::new(),
        })
    }

    /// The app and its frame statistics.
    pub const fn shell(&self) -> &Shell<A> {
        &self.shell
    }

    /// Draws the font roles with `set`'s families (`[fonts]` in the settings). Call it before the first
    /// [`step`](Self::step).
    #[cfg(feature = "fonts")]
    pub fn set_fonts(&mut self, set: fusuma::fonts::FontSet) {
        self.surface.set_fonts(set);
    }

    /// The panel.
    pub const fn panel(&self) -> &P {
        &self.panel
    }

    /// The panel, to read or change it from outside.
    pub fn panel_mut(&mut self) -> &mut P {
        &mut self.panel
    }

    /// The planner, to ask for a full refresh or read what waits.
    pub fn planner_mut(&mut self) -> &mut Planner {
        &mut self.planner
    }

    /// Hands an input (a button, an encoder turn) to the app.
    pub fn input(&mut self, input: Input) {
        self.shell.input(input);
    }

    /// One step at the monotonic time `now_ms`: a frame runs, what it changed is noted, and the panel is refreshed if
    /// the policy allows it now. The wake is the soonest of the app's and the planner's.
    ///
    /// # Errors
    /// [`DriverError`] when drawing failed or the panel did.
    pub fn step(&mut self, now_ms: u64) -> Result<EinkStep, DriverError<P::Error>> {
        let drawn = self
            .surface
            .frame(&mut self.shell, now_ms)
            .map_err(DriverError::Draw)?;
        if let Some(region) = drawn {
            // Vello forgets a frame's commands at the next one, so the pixels are made now and not when the panel is
            // ready; it only redraws the damaged part, which is small.
            self.surface.rasterise();
            self.planner.damage(region, now_ms);
        }
        let refreshed = self.planner.take(now_ms);
        if let Some(what) = refreshed {
            let (w, h) = self.panel.size();
            let dither = self.planner.policy().dither;
            match what {
                Refresh::Full => {
                    pack_mono(
                        &mut self.surface,
                        (w, h),
                        Rect::new(0, 0, w.next_multiple_of(8), h),
                        dither,
                        &mut self.packed,
                    );
                    self.panel.full(&self.packed).map_err(DriverError::Board)?;
                }
                Refresh::Partial(r) => {
                    pack_mono(&mut self.surface, (w, h), r, dither, &mut self.packed);
                    self.panel
                        .partial(r, &self.packed)
                        .map_err(DriverError::Board)?;
                }
            }
        }
        let app = self.shell.wake(now_ms);
        let plan = self.planner.next_at().map(|t| {
            let ms = u32::try_from(t.saturating_sub(now_ms)).unwrap_or(u32::MAX);
            if ms == 0 {
                fusuma::Wake::Now
            } else {
                fusuma::Wake::At(ms)
            }
        });
        let wake = plan.map_or(app, |p| app.min(p));
        Ok(EinkStep { refreshed, wake })
    }

    /// Puts the panel to sleep (it keeps the picture) and asks for a full refresh when it wakes.
    ///
    /// # Errors
    /// The panel's error.
    pub fn sleep(&mut self) -> Result<(), P::Error> {
        self.planner.want_full();
        self.panel.sleep()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture held in RGB565, readable by rows.
    struct Pic {
        w: u32,
        px: Vec<u16>,
    }

    impl Pixels for Pic {
        fn row(&mut self, y: u32, x: u32, out: &mut [u16]) {
            let at = (y * self.w + x) as usize;
            out.copy_from_slice(&self.px[at..at + out.len()]);
        }
    }

    const BLACK: u16 = 0;
    const WHITE: u16 = 0xFFFF;

    #[test]
    fn brightness_runs_from_black_to_white_and_green_is_brighter_than_blue() {
        assert_eq!(luma(0), 0);
        assert_eq!(luma(0xFFFF), 255);
        assert!(luma(0x07E0) > luma(0x001F) * 3, "green over blue");
        assert!(luma(0xF800) < luma(0x07E0), "red under green");
    }

    #[test]
    fn a_threshold_is_crisp_and_an_ordered_dither_makes_a_gray_a_pattern_that_averages_right() {
        let t = Dither::Threshold(128);
        assert!(!t.white(0, 0, 127) && t.white(0, 0, 128));
        let o = Dither::Ordered;
        for (level, expect) in [(0u8, 0u32), (255, 16), (128, 8)] {
            let whites = (0..16).filter(|i| o.white(i % 4, i / 4, level)).count() as u32;
            assert!(
                whites.abs_diff(expect) <= 1,
                "{level}: {whites} of 16 white"
            );
        }
        // The same pixel is always the same: a partial refresh matches the whole picture.
        assert_eq!(o.white(5, 9, 100), o.white(5 + 4, 9 + 4, 100));
    }

    #[test]
    fn a_region_grows_to_whole_bytes_and_stays_on_the_screen() {
        let r = |x, y, w, h| Rect::new(x, y, w, h);
        assert_eq!(byte_region(r(3, 2, 10, 5), 128, 296), Some(r(0, 2, 16, 5)));
        assert_eq!(byte_region(r(8, 0, 8, 1), 128, 296), Some(r(8, 0, 8, 1)));
        assert_eq!(byte_region(r(9, 0, 8, 1), 128, 296), Some(r(8, 0, 16, 1)));
        // A 122 wide panel: the last byte column is past the edge, which is allowed.
        assert_eq!(
            byte_region(r(100, 0, 100, 3), 122, 250),
            Some(r(96, 0, 32, 3))
        );
        assert_eq!(byte_region(r(0, 0, 0, 5), 128, 296), None);
        assert_eq!(byte_region(r(500, 0, 5, 5), 128, 296), None);
        assert_eq!(row_bytes(r(0, 0, 32, 1)), 4);
    }

    #[test]
    fn packing_is_eight_pixels_a_byte_msb_first_a_set_bit_is_white_and_the_edge_is_paper() {
        // 10 wide, 2 rows: row 0 black, row 1 white except pixel 2 black.
        let mut px = vec![BLACK; 10];
        px.extend(vec![WHITE; 10]);
        px[10 + 2] = BLACK;
        let mut pic = Pic { w: 10, px };
        let region = byte_region(Rect::new(0, 0, 10, 2), 10, 2).unwrap();
        assert_eq!(region, Rect::new(0, 0, 16, 2));
        let mut out = Vec::new();
        pack_mono(&mut pic, (10, 2), region, Dither::default(), &mut out);
        // Row 0: ten black then six paper pixels past the edge: 0000_0000, 0011_1111.
        // Row 1: 1101_1111, 1111_1111.
        assert_eq!(out, [0b0000_0000, 0b0011_1111, 0b1101_1111, 0b1111_1111]);
        // A region below the screen is paper.
        let mut out = Vec::new();
        pack_mono(
            &mut pic,
            (10, 2),
            Rect::new(0, 1, 16, 3),
            Dither::default(),
            &mut out,
        );
        assert_eq!(out.len(), 6);
        assert_eq!(&out[2..], [0xFF; 4]);
        // Nothing asked, nothing sent; a region off the left is refused rather than wrapped.
        pack_mono(
            &mut pic,
            (10, 2),
            Rect::new(0, 0, 0, 2),
            Dither::default(),
            &mut out,
        );
        assert!(out.is_empty());
        pack_mono(
            &mut pic,
            (10, 2),
            Rect::new(-8, 0, 16, 1),
            Dither::default(),
            &mut out,
        );
        assert!(out.is_empty());
    }

    fn policy() -> RefreshPolicy {
        RefreshPolicy {
            min_interval_ms: 300,
            settle_ms: 100,
            full_every: 3,
            dither: Dither::default(),
        }
    }

    #[test]
    fn the_first_refresh_is_a_full_one_and_changes_that_come_together_are_one_refresh() {
        let mut p = Planner::new((128, 296), policy());
        assert!(p.pending(), "nobody knows what is on the glass");
        assert_eq!(p.take(0), Some(Refresh::Full));
        assert_eq!(p.take(10), None);
        assert!(!p.pending());
        // Two changes 50 ms apart: shown once, after the screen was quiet for the settle time, as their union.
        p.damage(Rect::new(10, 10, 5, 5), 1000);
        assert_eq!(p.next_at(), Some(1100));
        p.damage(Rect::new(60, 100, 5, 5), 1050);
        assert_eq!(
            p.next_at(),
            Some(1150),
            "a new change restarts the quiet time"
        );
        assert_eq!(p.take(1100), None);
        assert_eq!(
            p.take(1150),
            Some(Refresh::Partial(Rect::new(8, 10, 64, 95)))
        );
        assert!(!p.pending());
    }

    #[test]
    fn the_panel_is_not_asked_again_before_the_minimum_interval() {
        let mut p = Planner::new((128, 296), policy());
        p.take(0);
        p.damage(Rect::new(0, 0, 8, 8), 1000);
        assert!(p.take(1100).is_some());
        p.damage(Rect::new(0, 0, 8, 8), 1101);
        // Quiet since 1201, but the last refresh was at 1100: free at 1400.
        assert_eq!(p.next_at(), Some(1400));
        assert_eq!(p.take(1300), None);
        assert!(p.take(1400).is_some());
        // With no settle and no interval a change shows at once.
        let mut p = Planner::new(
            (128, 296),
            RefreshPolicy {
                min_interval_ms: 0,
                settle_ms: 0,
                ..policy()
            },
        );
        p.take(0);
        p.damage(Rect::new(0, 0, 8, 8), 5);
        assert!(p.take(5).is_some());
    }

    #[test]
    fn a_full_refresh_comes_after_so_many_partial_ones_and_on_request() {
        let mut p = Planner::new((128, 296), policy());
        let mut now = 0;
        assert_eq!(p.take(now), Some(Refresh::Full));
        let mut kinds = Vec::new();
        for _ in 0..8 {
            now += 1000;
            p.damage(Rect::new(0, 0, 8, 8), now);
            now += 1000;
            kinds.push(matches!(p.take(now), Some(Refresh::Full)));
        }
        // Three partial, then the clean-up, and again.
        assert_eq!(
            kinds,
            [false, false, false, true, false, false, false, true]
        );
        p.want_full();
        assert_eq!(p.take(now + 5000), Some(Refresh::Full));
        // A full refresh shows everything, so the damage waiting is gone.
        p.damage(Rect::new(0, 0, 8, 8), now + 6000);
        p.want_full();
        assert_eq!(p.take(now + 9000), Some(Refresh::Full));
        assert_eq!(p.take(now + 20_000), None);
        // Off-screen damage is not damage; never cleaning up is allowed.
        let mut p = Planner::new(
            (128, 296),
            RefreshPolicy {
                full_every: 0,
                ..policy()
            },
        );
        p.take(0);
        p.damage(Rect::new(1000, 1000, 5, 5), 10);
        assert!(!p.pending());
        for i in 1..100u64 {
            p.damage(Rect::new(0, 0, 8, 8), i * 1000);
            assert!(matches!(p.take(i * 1000 + 500), Some(Refresh::Partial(_))));
        }
    }
}
