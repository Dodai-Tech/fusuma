#![allow(clippy::unwrap_used, clippy::too_many_lines)]
//! A model of the SSD168x memory (the SSD1680 and SSD1677 families), as a `Bus`: it takes the command bytes the driver sends,
//! keeps the two RAMs the way the chip's data sheet says (a window, an address counter that wraps inside it, the data entry
//! mode), and "shows" the new RAM when the update is activated. The drivers are then checked by what ends up on the glass, not
//! by the list of bytes: a picture that comes out mirrored, shifted a byte, or with a partial update that would flash fails here.
//!
//! The model is built from the data sheet and Waveshare's drivers, the same sources the drivers follow; it proves the driver
//! does what it means to, not what a panel does.

use fusuma::render::Rect;
use fusuma_platform_embedded::eink::bus::Bus;
use fusuma_platform_embedded::eink::ssd1677::{Config as Cfg1677, Ssd1677};
use fusuma_platform_embedded::eink::ssd1680::{Config as Cfg1680, Ssd1680};
use fusuma_platform_embedded::eink::{Panel, byte_region};

/// How a family counts the X address.
#[derive(Clone, Copy)]
enum XUnit {
    /// One byte (eight pixels) a step; one byte in the commands (SSD1680).
    Bytes,
    /// One pixel, two bytes in the commands, still eight pixels to a stored byte (SSD1677).
    Pixels,
}

struct Sim {
    unit: XUnit,
    /// Columns the RAM holds (a multiple of 8) and rows.
    cols: usize,
    rows: usize,
    /// Wired mirrored in x: RAM column c shows at c' = cols - 1 - c.
    wired_mirror: bool,
    new: Vec<u8>,
    old: Vec<u8>,
    /// What the glass shows: one bool per pixel, true is white.
    shown: Vec<bool>,
    x_range: (usize, usize),
    y_range: (usize, usize),
    x: usize,
    y: usize,
    entry: u8,
    cur: Option<u8>,
    buf: Vec<u8>,
    shown_ram: Option<Vec<u8>>,
    update: u8,
    /// A problem the model saw, with what caused it.
    problems: Vec<String>,
    awake: bool,
}

impl Sim {
    fn new(unit: XUnit, cols: usize, rows: usize, wired_mirror: bool) -> Self {
        let n = cols / 8 * rows;
        Self {
            unit,
            cols,
            rows,
            wired_mirror,
            new: vec![0xFF; n],
            old: vec![0xFF; n],
            shown: vec![true; cols * rows],
            x_range: (0, cols / 8 - 1),
            y_range: (0, rows - 1),
            x: 0,
            y: 0,
            entry: 0x03,
            cur: None,
            buf: vec![],
            shown_ram: None,
            update: 0,
            problems: vec![],
            awake: false,
        }
    }

    fn xb(&self, lo: u8, hi: u8) -> usize {
        match self.unit {
            XUnit::Bytes => usize::from(lo),
            XUnit::Pixels => (usize::from(lo) | usize::from(hi) << 8) / 8,
        }
    }

    fn run(&mut self, command: u8, d: &[u8]) {
        let at = |i: usize| d.get(i).copied().unwrap_or(0);
        match command {
            0x12 => self.awake = true,
            0x11 => self.entry = at(0),
            0x44 => {
                self.x_range = match self.unit {
                    XUnit::Bytes => (self.xb(at(0), 0), self.xb(at(1), 0)),
                    XUnit::Pixels => (self.xb(at(0), at(1)), self.xb(at(2), at(3))),
                };
            }
            0x45 => {
                self.y_range = (
                    usize::from(at(0)) | usize::from(at(1)) << 8,
                    usize::from(at(2)) | usize::from(at(3)) << 8,
                );
            }
            0x4E => self.x = self.xb(at(0), at(1)),
            0x4F => self.y = usize::from(at(0)) | usize::from(at(1)) << 8,
            0x24 | 0x26 => {
                if !self.awake {
                    self.problems
                        .push("RAM written before the soft reset".into());
                }
                for &b in d {
                    self.store(command, b);
                }
            }
            0x22 => self.update = at(0),
            0x20 => self.activate(),
            _ => {}
        }
    }

    fn store(&mut self, which: u8, b: u8) {
        let (x0, x1) = self.x_range;
        let (y0, y1) = self.y_range;
        if self.x < x0 || self.x > x1 || self.y < y0 || self.y > y1 || self.y >= self.rows {
            self.problems
                .push(format!("a byte past the window: x {} y {}", self.x, self.y));
            return;
        }
        // Entry mode 0x03: X and Y count up, X moves first.
        assert_eq!(self.entry & 7, 3, "only entry mode 3 is modelled");
        let i = self.y * (self.cols / 8) + self.x;
        if which == 0x24 {
            self.new[i] = b
        } else {
            self.old[i] = b
        }
        self.x += 1;
        if self.x > x1 {
            self.x = x0;
            self.y += 1;
        }
    }

    fn activate(&mut self) {
        let partial = self.update == 0xFF || self.update == 0x0F || self.update == 0xCC;
        if partial && self.old != self.glass_ram() {
            self.problems.push("a partial refresh while the comparison RAM is not the shown picture (it would flash)".into());
        }
        for y in 0..self.rows {
            for c in 0..self.cols {
                let white = self.new[y * (self.cols / 8) + c / 8] >> (7 - c % 8) & 1 == 1;
                let px = if self.wired_mirror {
                    self.cols - 1 - c
                } else {
                    c
                };
                self.shown[y * self.cols + px] = white;
            }
        }
        self.shown_ram = Some(self.new.clone());
    }

    fn glass_ram(&self) -> Vec<u8> {
        self.shown_ram
            .clone()
            .unwrap_or_else(|| vec![0xFF; self.new.len()])
    }
}

impl Sim {
    fn flush(&mut self) {
        if let Some(c) = self.cur.take() {
            let d = std::mem::take(&mut self.buf);
            self.run(c, &d);
        }
    }

    /// The picture on the glass, `width` x `height` of it, after whatever is still pending is run.
    fn picture(&mut self, width: usize) -> Vec<Vec<bool>> {
        self.flush();
        (0..self.rows)
            .map(|y| self.shown[y * self.cols..y * self.cols + width].to_vec())
            .collect()
    }
}

impl Bus for Sim {
    type Error = std::convert::Infallible;

    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.flush();
        self.cur = Some(command);
        Ok(())
    }

    fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        self.buf.extend_from_slice(data);
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        self.flush();
        Ok(())
    }

    fn wait_busy(&mut self) -> Result<(), Self::Error> {
        self.flush();
        Ok(())
    }
}

/// A picture of `w` x `h` that no mirror, shift or swap leaves unchanged: a pseudo-random pattern.
fn picture(w: usize, h: usize, seed: u32) -> Vec<Vec<bool>> {
    let mut s = seed;
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| {
                    s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    // Mostly structure (a diagonal and a frame) so a shift shows, plus noise.
                    x == y || x + 1 == w || y == 0 || (s >> 24) & 3 == 0
                })
                .collect()
        })
        .collect()
}

/// `region` of `pic` packed like `pack_mono`: rows of bytes, most significant bit first, set is white, past the edge white.
fn pack(pic: &[Vec<bool>], region: Rect) -> Vec<u8> {
    let (x0, y0) = (region.x as usize, region.y as usize);
    let mut out = vec![];
    for row in 0..region.h as usize {
        for byte in 0..(region.w as usize).div_ceil(8) {
            let mut b = 0u8;
            for bit in 0..8 {
                let (x, y) = (x0 + byte * 8 + bit, y0 + row);
                let white = pic.get(y).and_then(|r| r.get(x)).copied().unwrap_or(true);
                b |= u8::from(white) << (7 - bit);
            }
            out.push(b);
        }
    }
    out
}

fn show<P: Panel>(panel: &mut P, pic: &[Vec<bool>]) {
    let (w, h) = panel.size();
    let whole = Rect::new(0, 0, w.next_multiple_of(8), h);
    panel.full(&pack(pic, whole)).unwrap();
}

fn with_patch(pic: &[Vec<bool>], other: &[Vec<bool>], r: Rect) -> Vec<Vec<bool>> {
    let mut out = pic.to_vec();
    for y in r.y as usize..(r.y as usize + r.h as usize).min(out.len()) {
        for x in r.x as usize..(r.x as usize + r.w as usize).min(out[y].len()) {
            out[y][x] = other[y][x];
        }
    }
    out
}

/// A full refresh then partial ones: the glass must show exactly what was drawn, with no problem noted.
fn check<P: Panel>(mut panel: P, sim: impl Fn(&mut P) -> &mut Sim, width: usize, label: &str) {
    let (w, h) = panel.size();
    assert_eq!(w as usize, width);
    let a = picture(w as usize, h as usize, 1);
    show(&mut panel, &a);
    assert_eq!(
        sim(&mut panel).picture(width),
        a,
        "{label}: a full refresh shows the picture"
    );

    let b = picture(w as usize, h as usize, 2);
    let mut shown = a;
    for ask in [
        Rect::new(8, 3, 40, 10),
        Rect::new(w as i32 - 9, h as i32 - 5, 9, 5),
        Rect::new(0, 0, 8, 1),
        Rect::new(13, h as i32 / 2, w - 20, 7),
    ] {
        let r = byte_region(ask, w, h).unwrap();
        panel.partial(r, &pack(&b, r)).unwrap();
        shown = with_patch(&shown, &b, r);
        assert_eq!(
            sim(&mut panel).picture(width),
            shown,
            "{label}: partial {ask:?} (as {r:?})"
        );
    }
    assert_eq!(sim(&mut panel).problems, Vec::<String>::new(), "{label}");
}

#[test]
fn the_ssd1680_shows_what_it_is_given() {
    for (cfg, name) in [
        (Cfg1680::EPD_1IN54, "1.54"),
        (Cfg1680::EPD_2IN13, "2.13 (122 wide: 16 bytes)"),
        (Cfg1680::EPD_2IN9, "2.9"),
    ] {
        let cols = (cfg.width as usize).next_multiple_of(8);
        let p = Ssd1680::new(
            Sim::new(XUnit::Bytes, cols, cfg.height as usize, false),
            cfg,
        );
        check(p, |p| p.bus_mut(), cfg.width as usize, name);
    }
}

#[test]
fn the_ssd1677_shows_what_it_is_given_on_a_straight_and_a_mirrored_panel() {
    for (cfg, mirrored, name) in [
        (Cfg1677::new(800, 480), false, "800x480 straight"),
        (Cfg1677::STICKY, true, "800x480 wired mirrored"),
    ] {
        let p = Ssd1677::new(Sim::new(XUnit::Pixels, 800, 480, mirrored), cfg);
        check(p, |p| p.bus_mut(), 800, name);
    }
}

// The model must be able to fail, or a pass means nothing: a Sticky driver on a panel that is not mirrored shows a mirrored
// picture, and the 1.54" driver on a panel counted in pixels writes to the wrong columns.
#[test]
#[should_panic(expected = "a full refresh shows the picture")]
fn the_model_sees_a_mirrored_picture() {
    let p = Ssd1677::new(Sim::new(XUnit::Pixels, 800, 480, false), Cfg1677::STICKY);
    check(p, |p| p.bus_mut(), 800, "sticky driver, straight panel");
}

#[test]
#[should_panic(expected = "a full refresh shows the picture")]
fn the_model_sees_the_wrong_x_unit() {
    let cfg = Cfg1680::EPD_1IN54;
    let p = Ssd1680::new(Sim::new(XUnit::Pixels, 200, 200, false), cfg);
    check(
        p,
        |p| p.bus_mut(),
        200,
        "bytes sent to a pixel-counting chip",
    );
}

// ── Waveshare's own firmware, command by command ────────────────────────────────────────────────────────────────────────
// What the vendor's init sends (written down from the source named at each table) against what our driver sends on its first
// full refresh. A command the vendor sends that the driver does not send with the same bytes is a *difference*; the lists below
// are every difference, so a new one (or one that goes away) fails the test and has to be read. Differences are not all bugs:
// they are what to look at first when a panel misbehaves.
use fusuma_platform_embedded::eink::bus::{Call, Recorder};

type Step = (u8, &'static [u8]);

fn sent(mut run: impl FnMut(&mut Recorder)) -> Vec<(u8, Vec<u8>)> {
    let mut rec = Recorder::default();
    run(&mut rec);
    let mut out: Vec<(u8, Vec<u8>)> = vec![];
    for call in rec.log {
        match call {
            Call::Command(c) => out.push((c, vec![])),
            Call::Data(d) => out.last_mut().unwrap().1.extend(d),
            _ => {}
        }
    }
    out
}

fn differences(vendor: &[Step], ours: &[(u8, Vec<u8>)]) -> Vec<Step> {
    vendor
        .iter()
        .copied()
        .filter(|(c, d)| !ours.iter().any(|(oc, od)| oc == c && od == d))
        .collect()
}

#[test]
fn the_ssd1677_against_the_waveshare_3_97_firmware() {
    // waveshareteam/ESP32-S3-ePaper-3.97, ESP-IDF/08_ESP32-S3_e-Paper-3.97/components/epaper_port/epaper_port.c: EPD_Init, then
    // EPD_Display's refresh (EPD_TurnOnDisplay). 800x480.
    let vendor: &[Step] = &[
        (0x12, &[]),
        (0x18, &[0x80]),
        (0x0C, &[0xAE, 0xC7, 0xC3, 0xC0, 0x80]),
        (0x01, &[0xDF, 0x01, 0x02]),
        (0x3C, &[0x01]),
        (0x11, &[0x01]),
        (0x44, &[0x00, 0x00, 0x1F, 0x03]),
        (0x45, &[0xDF, 0x01, 0x00, 0x00]),
        (0x4E, &[0x00, 0x00]),
        (0x4F, &[0x00, 0x00]),
        (0x22, &[0xF7]),
        (0x20, &[]),
    ];
    let ours = sent(|bus| {
        let mut p = Ssd1677::new(std::mem::take(bus), Cfg1677::new(800, 480));
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        *bus = std::mem::take(p.bus_mut());
    });
    // Both set the same panel up; the vendor counts the Y address down (entry mode 1, window 479 -> 0), the driver up
    // (mode 3, window 0 -> 479). With the same gate scan one of them is upside down: on a Waveshare 3.97 look at the vertical
    // direction first.
    assert_eq!(
        differences(vendor, &ours),
        [(0x11, &[0x01][..]), (0x45, &[0xDF, 0x01, 0x00, 0x00][..])]
    );
}

#[test]
fn the_ssd1680_against_the_waveshare_1_54_firmware() {
    // waveshareteam/ESP32-S3-ePaper-1.54, ESP-IDF/V2/11_FactoryProgram/components/port_bsp/port_display.cpp: EPD_Init and
    // EPD_Display. 200x200.
    let vendor: &[Step] = &[
        (0x12, &[]),
        (0x01, &[0xC7, 0x00, 0x01]),
        (0x11, &[0x01]),
        (0x44, &[0x00, 0x18]),
        (0x45, &[0xC7, 0x00, 0x00, 0x00]),
        (0x3C, &[0x01]),
        (0x18, &[0x80]),
        (0x22, &[0xB1]),
        (0x4E, &[0x00]),
        (0x4F, &[0xC7, 0x00]),
        (0x22, &[0xC7]),
        (0x20, &[]),
    ];
    let ours = sent(|bus| {
        let mut p = Ssd1680::new(std::mem::take(bus), Cfg1680::EPD_1IN54);
        p.full(&vec![0xFF; 25 * 200]).unwrap();
        *bus = std::mem::take(p.bus_mut());
    });
    // The vendor's 1.54" V2 firmware loads its own waveform table (command 0x32, then 0xC7) and counts the Y address down
    // from 199; the driver follows Waveshare's V4 sequence (the chip's built-in waveform, 0xF7, Y counting up, border 0x05).
    // The built-in waveform is the part to check on a panel: if the picture is faint or ghosts, try the vendor's table.
    assert_eq!(
        differences(vendor, &ours),
        [
            (0x01, &[0xC7, 0x00, 0x01][..]),
            (0x11, &[0x01][..]),
            (0x45, &[0xC7, 0x00, 0x00, 0x00][..]),
            (0x3C, &[0x01][..]),
            (0x22, &[0xB1][..]),
            (0x4F, &[0xC7, 0x00][..]),
            (0x22, &[0xC7][..]),
        ]
    );
}
