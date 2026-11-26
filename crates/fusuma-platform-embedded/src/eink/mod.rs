#![doc = include_str!("README.md")]

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::renderers::embedded::Embedded;
pub use fusuma::renderers::embedded::{Dither, MonoFrame};
use fusuma::{App, DisplayKind, Input, Screen, Shell};

use crate::{DriverError, Pixels, Surface};

pub mod bus;
pub mod epd13e6;
pub mod epd4g;
pub mod ssd1677;
pub mod ssd1680;
pub mod ssd2677;
pub mod sticky;
pub mod uc8179;

/// The brightness of an RGB565 pixel, 0 to 255 (Rec. 601 weights).
#[must_use]
pub const fn luma(p: u16) -> u8 {
    fusuma::renderers::embedded::luma565(p)
}

/// The four grays a panel with a gray mode shows, darkest first. A level is two bits: 0 black, 1 dark gray, 2 light gray, 3 white.
pub const GRAY_LEVELS: u8 = 4;

/// The level (0 to 3) of a brightness 0 to 255 at pixel `(x, y)`: the nearest of the four, or with an ordered dither the one a
/// 4x4 pattern of the neighbours' rounding errors picks, so a smooth ramp is dots of two neighbouring levels instead of bands.
#[must_use]
pub const fn gray_level(dither: Dither, x: u32, y: u32, luma: u8) -> u8 {
    // The four levels are at 0, 85, 170 and 255: a step of 85.
    match dither {
        Dither::Threshold(_) => ((luma as u32 * 3 + 127) / 255) as u8,
        Dither::Ordered => {
            let t = Dither::bayer(x, y);
            // Which step the brightness is in, and how far into it (0 to 84): above the pattern's threshold it rounds up.
            let step = luma as u32 / 85;
            let into = luma as u32 % 85;
            let up = into * 16 / 85 > t;
            let level = step + up as u32;
            if level > 3 { 3 } else { level as u8 }
        }
    }
}

/// Packs `region` (the whole width, from [`byte_region`]) of the picture in `src` as a four-gray panel takes it: two bits a pixel,
/// four pixels to a byte, the first pixel in the top bits, a level as in [`gray_level`]. Rows follow one another. `out` is cleared first
/// and ends up `region.w / 4 * region.h` long; pixels past the picture's edge are white.
pub fn pack_gray4<P: Pixels + ?Sized>(
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
    let per_row = (region.w as usize).div_ceil(4);
    out.reserve(per_row * region.h as usize);
    let on = region.w.min(screen.0.saturating_sub(x0)) as usize;
    let mut line = vec![0u16; on];
    for row in 0..region.h {
        let y = y0 + row;
        if y >= screen.1 {
            out.extend(core::iter::repeat_n(0xFF, per_row));
            continue;
        }
        src.row(y, x0, &mut line);
        for byte in 0..per_row {
            let mut b = 0u8;
            for k in 0..4 {
                let i = byte * 4 + k;
                let level = line
                    .get(i)
                    .map_or(3, |&p| gray_level(dither, x0 + i as u32, y, luma(p)));
                b = (b << 2) | level;
            }
            out.push(b);
        }
    }
}

/// The inks of a colour e-paper panel and how the panel codes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// What each ink looks like to the quantiser, RGB565 (not what the glass is exactly).
    pub colors: &'static [u16],
    /// The panel's code for each ink, parallel to `colors`.
    pub codes: &'static [u8],
    /// Bits a pixel takes in the panel's memory: 2 (four to a byte) or 4 (two to a byte), the first pixel in the top bits.
    pub bits: u8,
}

impl Palette {
    /// The code of the ink nearest to the RGB565 pixel `p` at `(x, y)`. With an ordered dither the pixel is first moved by the 4x4
    /// pattern, up to about a quarter of the range either way, so a colour between two inks becomes dots of both.
    #[must_use]
    pub fn code(&self, dither: Dither, x: u32, y: u32, p: u16) -> u8 {
        let split = |p: u16| {
            (
                i32::from((p >> 11) & 0x1F) * 255 / 31,
                i32::from((p >> 5) & 0x3F) * 255 / 63,
                i32::from(p & 0x1F) * 255 / 31,
            )
        };
        let bias = match dither {
            Dither::Threshold(_) => 0,
            Dither::Ordered => (Dither::bayer(x, y) as i32 - 8) * 8,
        };
        let (r, g, b) = split(p);
        let (r, g, b) = (r + bias, g + bias, b + bias);
        let mut best = (0u8, i32::MAX);
        for (&c, &code) in self.colors.iter().zip(self.codes) {
            let (cr, cg, cb) = split(c);
            // Green counts most and blue least, as the eye does.
            let d = 3 * (r - cr).pow(2) + 4 * (g - cg).pow(2) + 2 * (b - cb).pow(2);
            if d < best.1 {
                best = (code, d);
            }
        }
        best.0
    }

    /// The code of white (the paper), for the pixels past a picture's edge.
    #[must_use]
    pub fn white(&self) -> u8 {
        self.code(Dither::Threshold(0), 0, 0, 0xFFFF)
    }

    /// The code of black.
    #[must_use]
    pub fn black(&self) -> u8 {
        self.code(Dither::Threshold(0), 0, 0, 0x0000)
    }

    /// `code` in every slot of a byte.
    const fn repeat(&self, code: u8) -> u8 {
        let mut b = 0u8;
        let mut i = 0;
        while i < 8 / self.bits {
            b = (b << self.bits) | code;
            i += 1;
        }
        b
    }
}

/// Packs `region` (the whole width, from [`byte_region`]) of the picture in `src` for a colour panel: `palette.bits` a pixel, the
/// first in the top bits, each the code of the nearest ink ([`Palette::code`]). Rows follow one another. `out` is cleared first;
/// pixels past the picture's edge are white.
pub fn pack_color<P: Pixels + ?Sized>(
    src: &mut P,
    screen: (u32, u32),
    region: Rect,
    dither: Dither,
    palette: &Palette,
    out: &mut Vec<u8>,
) {
    out.clear();
    if region.w == 0
        || region.h == 0
        || region.x < 0
        || region.y < 0
        || !matches!(palette.bits, 2 | 4)
    {
        return;
    }
    let white = palette.white();
    let (x0, y0) = (region.x as u32, region.y as u32);
    let per_byte = usize::from(8 / palette.bits);
    let per_row = (region.w as usize).div_ceil(per_byte);
    out.reserve(per_row * region.h as usize);
    let on = region.w.min(screen.0.saturating_sub(x0)) as usize;
    let mut line = vec![0u16; on];
    for row in 0..region.h {
        let y = y0 + row;
        if y >= screen.1 {
            out.extend(core::iter::repeat_n(palette.repeat(white), per_row));
            continue;
        }
        src.row(y, x0, &mut line);
        for byte in 0..per_row {
            let mut b = 0u8;
            for k in 0..per_byte {
                let i = byte * per_byte + k;
                let code = line
                    .get(i)
                    .map_or(white, |&p| palette.code(dither, x0 + i as u32, y, p));
                b = (b << palette.bits) | code;
            }
            out.push(b);
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
    /// What most black and white panels want: 300 ms apart, a 100 ms settle, a clean-up every 30 refreshes, and a threshold
    /// at 170 (smooth text is mixed with the paper before it is turned black or white: a lower level loses the thin strokes
    /// of a small glyph, a higher one makes it bold).
    pub const DEFAULT: Self = Self {
        min_interval_ms: 300,
        settle_ms: 100,
        full_every: 30,
        dither: Dither::Threshold(170),
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

    /// Whether this panel can show four grays (and has been set up to): see [`full_gray4`](Self::full_gray4). Nothing does by
    /// default.
    fn gray4(&self) -> bool {
        false
    }

    /// The inks of a colour panel ([`Palette`]); `None`, the default, is a panel with no colour. A driver made for it
    /// ([`EinkDriver::new_color`]) draws in colour, packs the nearest ink of each pixel and shows it with
    /// [`full_color`](Self::full_color).
    fn palette(&self) -> Option<Palette> {
        None
    }

    /// The whole picture as [`pack_color`] makes it for [`palette`](Self::palette), shown with the panel's colour refresh. A
    /// panel with no colours never has this called.
    ///
    /// # Errors
    /// The bus error.
    fn full_color(&mut self, packed: &[u8]) -> Result<(), Self::Error> {
        let _ = packed;
        Ok(())
    }

    /// The whole picture in four grays, as [`pack_gray4`] makes it (two bits a pixel, four to a byte), shown with the panel's gray
    /// refresh. A panel with no gray mode never has this called.
    ///
    /// # Errors
    /// The bus error.
    fn full_gray4(&mut self, levels: &[u8]) -> Result<(), Self::Error> {
        let _ = levels;
        Ok(())
    }

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
    canvas: Canvas,
    planner: Planner,
    packed: Vec<u8>,
    /// The panel shows four grays and this driver was made for them ([`EinkDriver::new_gray4`]): every refresh is a whole gray one.
    gray: bool,
    /// The panel has inks and this driver was made for them ([`EinkDriver::new_color`]): the codes of the whole picture are the nearest
    /// of these colours and go out as a whole refresh.
    palette: Option<Palette>,
}

/// What the app draws on: an RGB565 surface (either renderer) that is quantised for the panel when it is shown, or a one-bit
/// frame the integer renderer draws on directly (what the panel takes, a sixteenth of the memory, nothing to pack).
enum Canvas {
    Rgb(Surface),
    Mono(Box<Embedded<MonoFrame<Vec<u8>>>>),
}

impl core::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Rgb(_) => "Canvas::Rgb",
            Self::Mono(_) => "Canvas::Mono",
        })
    }
}

impl Canvas {
    fn frame<A: App>(
        &mut self,
        shell: &mut Shell<A>,
        now_ms: u64,
    ) -> Result<Option<Rect>, crate::DrawError> {
        match self {
            Self::Rgb(s) => s.frame(shell, now_ms),
            Self::Mono(r) => shell
                .frame(now_ms, &mut **r)
                .map_err(|e| crate::DrawError(alloc::format!("{e:?}"))),
        }
    }

    fn rasterise(&mut self) {
        if let Self::Rgb(s) = self {
            s.rasterise();
        }
    }

    /// The whole picture in four grays (see [`pack_gray4`]). Only the colour surface has grays to give: the one-bit frame has none, so
    /// it gives its black and white as levels 0 and 3.
    fn pack_gray(&mut self, screen: (u32, u32), region: Rect, dither: Dither, out: &mut Vec<u8>) {
        match self {
            Self::Rgb(s) => pack_gray4(s, screen, region, dither, out),
            Self::Mono(r) => {
                out.clear();
                let f = r.target();
                let per_row = (region.w as usize).div_ceil(4);
                for y in 0..region.h as i32 {
                    for byte in 0..per_row {
                        let mut b = 0u8;
                        for k in 0..4 {
                            let x = (byte * 4 + k) as i32;
                            let white = f.white_at(region.x + x, region.y + y).unwrap_or(true);
                            b = (b << 2) | if white { 3 } else { 0 };
                        }
                        out.push(b);
                    }
                }
            }
        }
    }

    /// The whole picture in a colour panel's inks (see [`pack_color`]). Only the colour surface has colours to give: the one-bit frame
    /// gives the nearest inks to black and white.
    fn pack_ink(
        &mut self,
        screen: (u32, u32),
        region: Rect,
        dither: Dither,
        palette: &Palette,
        out: &mut Vec<u8>,
    ) {
        match self {
            Self::Rgb(s) => pack_color(s, screen, region, dither, palette, out),
            Self::Mono(r) => {
                out.clear();
                let f = r.target();
                let per_byte = usize::from(8 / palette.bits.max(1));
                let per_row = (region.w as usize).div_ceil(per_byte);
                for y in 0..region.h as i32 {
                    for byte in 0..per_row {
                        let mut b = 0u8;
                        for k in 0..per_byte {
                            let x = (byte * per_byte + k) as i32;
                            let white = f.white_at(region.x + x, region.y + y).unwrap_or(true);
                            b = (b << palette.bits)
                                | if white {
                                    palette.white()
                                } else {
                                    palette.black()
                                };
                        }
                        out.push(b);
                    }
                }
            }
        }
    }

    /// `region` (byte-aligned, from [`byte_region`]) of the picture, packed for the panel into `out`.
    fn pack(&mut self, screen: (u32, u32), region: Rect, dither: Dither, out: &mut Vec<u8>) {
        match self {
            Self::Rgb(s) => pack_mono(s, screen, region, dither, out),
            Self::Mono(r) => {
                let f = r.target();
                out.clear();
                let (stride, rows) = (f.stride(), usize::from(f.height()));
                let (x0, wb) = (
                    region.x.max(0) as usize / 8,
                    (region.w as usize).div_ceil(8),
                );
                out.reserve(wb * region.h as usize);
                for row in 0..region.h as usize {
                    let y = region.y.max(0) as usize + row;
                    if y >= rows {
                        out.extend(core::iter::repeat_n(0xFF, wb));
                        continue;
                    }
                    let line = &f.bytes()[y * stride..(y + 1) * stride];
                    for b in x0..x0 + wb {
                        out.push(line.get(b).copied().unwrap_or(0xFF));
                    }
                }
            }
        }
    }
}

impl<A: App, P: Panel> EinkDriver<A, P> {
    /// Runs `app` on `panel` with `backend` (the integer renderer or Vello on the CPU). `None` when the surface cannot
    /// be made. The app is told what the panel is before its first frame.
    pub fn new(app: A, panel: P, backend: Backend, policy: RefreshPolicy) -> Option<Self> {
        let (w, h) = panel.size();
        let surface = Surface::new(backend, w, h)?;
        Some(Self::with_canvas(app, panel, Canvas::Rgb(surface), policy))
    }

    /// Like [`new`](Self::new), drawing straight into a one-bit frame with the integer renderer: a sixteenth of the memory
    /// (48 KB for an 800x480 panel, where RGB565 is 768 KB, which only a chip with PSRAM has) and nothing to quantise when it
    /// is shown. `policy.dither` is the dither the renderer turns colours with as it draws (a threshold keeps text crisp);
    /// anti-aliasing becomes hard edges or dots. `None` when the panel is wider than 65 535 pixels or has no room.
    pub fn new_mono(app: A, panel: P, policy: RefreshPolicy) -> Option<Self> {
        let (w, h) = panel.size();
        let (w16, h16) = (u16::try_from(w).ok()?, u16::try_from(h).ok()?);
        let bytes = MonoFrame::<Vec<u8>>::stride_of(w16).checked_mul(usize::from(h16))?;
        // Paper: every bit set, and the bits past the right edge stay so.
        let frame = MonoFrame::new(w16, h16, vec![0xFF; bytes], policy.dither)?;
        let canvas = Canvas::Mono(Box::new(Embedded::new(frame)));
        Some(Self::with_canvas(app, panel, canvas, policy))
    }

    /// Like [`new`](Self::new) for a panel that shows four grays ([`Panel::gray4`]): the picture is drawn in colour and quantised to
    /// four grays, so smooth text and edges are smooth, and every refresh is a whole gray one (the chips' gray mode has no partial
    /// refresh). `None` when the panel has no gray mode, or the surface cannot be made. It needs the colour surface (768 KB for
    /// 800x480), so a chip with PSRAM.
    pub fn new_gray4(app: A, panel: P, backend: Backend, policy: RefreshPolicy) -> Option<Self> {
        if !panel.gray4() {
            return None;
        }
        let mut d = Self::new(app, panel, backend, policy)?;
        d.gray = true;
        // The app is told the glass has four levels: smooth enough for the paper palette's anti-aliasing, not for colour.
        let s = d.panel.screen();
        d.shell.input(Input::Screen(s.with_glass(
            DisplayKind::Reflective,
            2,
            s.refresh_ms,
        )));
        Some(d)
    }

    /// Like [`new`](Self::new) for a colour panel ([`Panel::palette`]): the picture is drawn in colour and each pixel becomes the nearest
    /// ink, and every refresh is a whole one (these panels have no partial refresh, and take seconds). `None` when the panel has no
    /// colours or the surface cannot be made. It needs the colour surface, so a chip with PSRAM for anything large.
    pub fn new_color(app: A, panel: P, backend: Backend, policy: RefreshPolicy) -> Option<Self> {
        let palette = panel.palette()?;
        let mut d = Self::new(app, panel, backend, policy)?;
        d.gray = true;
        d.palette = Some(palette);
        // The app is told the glass has few levels: paper-like drawing with a splash of ink colours.
        let s = d.panel.screen();
        d.shell.input(Input::Screen(s.with_glass(
            DisplayKind::Reflective,
            palette.bits.min(2),
            s.refresh_ms,
        )));
        Some(d)
    }

    fn with_canvas(app: A, panel: P, canvas: Canvas, policy: RefreshPolicy) -> Self {
        let (w, h) = panel.size();
        let mut shell = Shell::new(app);
        shell.input(Input::Screen(panel.screen()));
        Self {
            shell,
            panel,
            canvas,
            planner: Planner::new((w, h), policy),
            packed: Vec::new(),
            gray: false,
            palette: None,
        }
    }

    /// The app and its frame statistics.
    pub const fn shell(&self) -> &Shell<A> {
        &self.shell
    }

    /// Draws the font roles with `set`'s families (`[fonts]` in the settings). Call it before the first
    /// [`step`](Self::step).
    #[cfg(feature = "fonts")]
    pub fn set_fonts(&mut self, set: fusuma::fonts::FontSet) {
        match &mut self.canvas {
            Canvas::Rgb(s) => s.set_fonts(set),
            Canvas::Mono(r) => r.set_fonts(set),
        }
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
            .canvas
            .frame(&mut self.shell, now_ms)
            .map_err(DriverError::Draw)?;
        if let Some(region) = drawn {
            // Vello forgets a frame's commands at the next one, so the pixels are made now and not when the panel is
            // ready; it only redraws the damaged part, which is small.
            self.canvas.rasterise();
            self.planner.damage(region, now_ms);
        }
        let refreshed = self.planner.take(now_ms);
        if let Some(what) = refreshed {
            let (w, h) = self.panel.size();
            let dither = self.planner.policy().dither;
            if self.gray {
                // Whatever was asked, the gray or colour mode shows the whole picture: pack it in levels or inks and send that.
                let whole = Rect::new(0, 0, w.next_multiple_of(8), h);
                if let Some(palette) = self.palette {
                    self.canvas
                        .pack_ink((w, h), whole, dither, &palette, &mut self.packed);
                    self.panel
                        .full_color(&self.packed)
                        .map_err(DriverError::Board)?;
                } else {
                    self.canvas
                        .pack_gray((w, h), whole, dither, &mut self.packed);
                    self.panel
                        .full_gray4(&self.packed)
                        .map_err(DriverError::Board)?;
                }
                return Ok(EinkStep {
                    refreshed: Some(Refresh::Full),
                    wake: self.wake_after(now_ms),
                });
            }
            match what {
                Refresh::Full => {
                    self.canvas.pack(
                        (w, h),
                        Rect::new(0, 0, w.next_multiple_of(8), h),
                        dither,
                        &mut self.packed,
                    );
                    self.panel.full(&self.packed).map_err(DriverError::Board)?;
                }
                Refresh::Partial(r) => {
                    self.canvas.pack((w, h), r, dither, &mut self.packed);
                    self.panel
                        .partial(r, &self.packed)
                        .map_err(DriverError::Board)?;
                }
            }
        }
        Ok(EinkStep {
            refreshed,
            wake: self.wake_after(now_ms),
        })
    }

    /// When to step again: the soonest of the app's wake and the planner's.
    fn wake_after(&self, now_ms: u64) -> fusuma::Wake {
        let app = self.shell.wake(now_ms);
        let plan = self.planner.next_at().map(|t| {
            let ms = u32::try_from(t.saturating_sub(now_ms)).unwrap_or(u32::MAX);
            if ms == 0 {
                fusuma::Wake::Now
            } else {
                fusuma::Wake::At(ms)
            }
        });
        plan.map_or(app, |p| app.min(p))
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
    fn four_levels_are_the_nearest_of_four_or_a_pattern_of_two_neighbours_and_packing_is_two_bits_a_pixel()
     {
        // The nearest: 0, 85, 170, 255 are the levels, with the halfway points between.
        for (luma, level) in [
            (0u8, 0u8),
            (42, 0),
            (43, 1),
            (85, 1),
            (127, 1),
            (128, 2),
            (170, 2),
            (212, 2),
            (213, 3),
            (255, 3),
        ] {
            assert_eq!(
                gray_level(Dither::Threshold(0), 0, 0, luma),
                level,
                "{luma}"
            );
        }
        // Ordered: the ends stay the ends, a level stays itself, and a ramp is made of two neighbouring levels in the right share.
        for x in 0..4 {
            for y in 0..4 {
                assert_eq!(gray_level(Dither::Ordered, x, y, 0), 0);
                assert_eq!(gray_level(Dither::Ordered, x, y, 255), 3);
            }
        }
        for luma in [20u8, 60, 100, 140, 200, 240] {
            let mut sum = 0u32;
            let mut seen = alloc::collections::BTreeSet::new();
            for i in 0..16 {
                let l = gray_level(Dither::Ordered, i % 4, i / 4, luma);
                sum += u32::from(l);
                seen.insert(l);
            }
            assert!(seen.len() <= 2, "{luma}: {seen:?}");
            // The average level times 85 is the brightness, to within half a step.
            let avg = sum as f64 * 85.0 / 16.0;
            assert!((avg - f64::from(luma)).abs() < 45.0, "{luma}: {avg}");
        }
        // Packing: 4 pixels to a byte, the first in the top bits; a pixel past the edge is white.
        let mut pic = Pic {
            w: 6,
            px: vec![0x0000, 0x528A, 0xAD75, 0xFFFF, 0x0000, 0xFFFF],
        };
        let mut out = Vec::new();
        pack_gray4(
            &mut pic,
            (6, 1),
            Rect::new(0, 0, 8, 1),
            Dither::Threshold(0),
            &mut out,
        );
        // Levels: 0, 1, 2, 3 | 0, 3, then two past the edge: white.
        assert_eq!(out, [0b00_01_10_11, 0b00_11_11_11]);
        let mut none = vec![9u8];
        pack_gray4(
            &mut pic,
            (6, 1),
            Rect::new(0, 0, 0, 1),
            Dither::default(),
            &mut none,
        );
        assert!(none.is_empty());
        pack_gray4(
            &mut pic,
            (6, 1),
            Rect::new(0, 0, 8, 3),
            Dither::default(),
            &mut out,
        );
        assert_eq!(out.len(), 2 * 3, "rows below the picture are white");
        assert_eq!(&out[2..], [0xFF; 4]);
    }

    /// Black, white, yellow, red: the codes 0 to 3 of Waveshare's four-colour panels.
    const INKS: Palette = Palette {
        colors: &[0x0000, 0xFFFF, 0xFFE0, 0xF800],
        codes: &[0, 1, 2, 3],
        bits: 2,
    };

    /// Black, white, yellow, red, blue, green: the codes of Waveshare's six-colour panels (code 4 is not used), four bits a pixel.
    const SIX: Palette = Palette {
        colors: &[0x0000, 0xFFFF, 0xFFE0, 0xF800, 0x001F, 0x07E0],
        codes: &[0, 1, 2, 3, 5, 6],
        bits: 4,
    };

    #[test]
    fn a_colour_is_the_code_of_the_nearest_ink() {
        let t = Dither::Threshold(0);
        for (pixel, code) in [
            (0x0000, 0),
            (0xFFFF, 1),
            (0xFFE0, 2),
            (0xF800, 3),
            // Off the inks: a dark gray is black, a light one white, orange is nearer yellow, a reddish one red.
            (0x2104, 0),
            (0xDEFB, 1),
            (0xFD20, 2),
            (0xFA60, 3),
            // Blue and green have no ink: black for the dark one, white (or yellow) for the light one.
            (0x001F, 0),
        ] {
            assert_eq!(INKS.code(t, 3, 4, pixel), code, "{pixel:#06x}");
        }
        // An ordered dither turns a colour between two inks into dots of both, and the dots average to the middle.
        let mut counts = [0u32; 4];
        for y in 0..4 {
            for x in 0..4 {
                counts[usize::from(INKS.code(Dither::Ordered, x, y, 0xFD20))] += 1;
            }
        }
        assert!(counts[2] > 0 && counts[3] > 0, "{counts:?}");
    }

    #[test]
    fn a_four_colour_picture_packs_like_the_grays_with_the_codes_of_its_inks() {
        let mut pic = Pic {
            w: 6,
            px: vec![0x0000, 0xFFFF, 0xFFE0, 0xF800, 0x0000, 0xFFFF],
        };
        let mut out = Vec::new();
        pack_color(
            &mut pic,
            (6, 1),
            Rect::new(0, 0, 8, 1),
            Dither::Threshold(0),
            &INKS,
            &mut out,
        );
        // Codes 0, 1, 2, 3 | 0, 1, then two past the edge: white (code 1).
        assert_eq!(out, [0b00_01_10_11, 0b00_01_01_01]);
        pack_color(
            &mut pic,
            (6, 1),
            Rect::new(0, 0, 8, 3),
            Dither::default(),
            &INKS,
            &mut out,
        );
        assert_eq!(out.len(), 2 * 3, "rows below the picture are white");
        assert_eq!(&out[2..], [0x55; 4]);
        let mut none = vec![9u8];
        pack_color(
            &mut pic,
            (6, 1),
            Rect::new(0, 0, 0, 1),
            Dither::default(),
            &INKS,
            &mut none,
        );
        assert!(none.is_empty());
    }

    #[test]
    fn six_inks_take_four_bits_two_pixels_to_a_byte_and_skip_code_four() {
        let mut pic = Pic {
            w: 4,
            px: vec![0x0000, 0xFFFF, 0x001F, 0x07E0],
        };
        let mut out = Vec::new();
        pack_color(
            &mut pic,
            (4, 1),
            Rect::new(0, 0, 6, 1),
            Dither::Threshold(0),
            &SIX,
            &mut out,
        );
        // Black 0, white 1 | blue 5, green 6 | two past the edge: white, white.
        assert_eq!(out, [0x01, 0x56, 0x11]);
        assert_eq!((SIX.black(), SIX.white()), (0, 1));
        for (pixel, code) in [(0xFFE0, 2), (0xF800, 3)] {
            assert_eq!(SIX.code(Dither::Threshold(0), 0, 0, pixel), code);
        }
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
