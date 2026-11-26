#![allow(clippy::unwrap_used)]
//! The showcase on a black and white panel: drawn by the integer renderer and by Vello, quantised, batched and sent to
//! an SSD1680 whose bus is a recorder.

use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::{Input, Key};
use fusuma_platform_embedded::eink::ssd1680::{Bus, Call, Config, Recorder, Ssd1680};
use fusuma_platform_embedded::eink::{EinkDriver, Panel, Refresh, RefreshPolicy};
use fusuma_showcase::Showcase;

fn policy() -> RefreshPolicy {
    RefreshPolicy {
        min_interval_ms: 300,
        settle_ms: 100,
        full_every: 30,
        dither: fusuma_platform_embedded::eink::Dither::default(),
    }
}

fn data_of(log: &[Call], after: u8) -> Vec<Vec<u8>> {
    // The data sent right after command `after`, each time.
    let mut out = Vec::new();
    for w in log.windows(2) {
        if let [Call::Command(c), Call::Data(d)] = w
            && *c == after
        {
            out.push(d.clone());
        }
    }
    out
}

fn commands(log: &[Call]) -> Vec<u8> {
    log.iter()
        .filter_map(|c| {
            if let Call::Command(c) = c {
                Some(*c)
            } else {
                None
            }
        })
        .collect()
}

fn ink(frame: &[u8]) -> usize {
    frame.iter().map(|b| b.count_zeros() as usize).sum()
}

fn run(backend: Backend) {
    let (w, h) = (128, 296);
    let panel = Ssd1680::new(Recorder::default(), Config::EPD_2IN9);
    assert_eq!(panel.size(), (w, h));
    let mut d = EinkDriver::new(Showcase::new(w, h), panel, backend, policy()).unwrap();

    // The first refresh is a full one of everything, after the first picture has settled.
    assert_eq!(d.step(0).unwrap().refreshed, None);
    let s = d.step(10).unwrap();
    assert_eq!(s.refreshed, None);
    assert!(matches!(s.wake, fusuma::Wake::At(90)), "{s:?}");
    let s = d.step(100).unwrap();
    assert_eq!(s.refreshed, Some(Refresh::Full));
    let log = d.panel_mut().bus_mut().log.clone();
    let frame = &data_of(&log, 0x24)[0];
    assert_eq!(frame.len(), 16 * 296, "the whole picture, a bit a pixel");
    assert_eq!(
        &data_of(&log, 0x26)[0],
        frame,
        "the previous memory gets it too"
    );
    let ink_px = ink(frame);
    assert!(
        ink_px > 300,
        "there is text and edges: {ink_px} black pixels"
    );
    assert!(
        ink_px < frame.len() * 8 / 2,
        "most of a page is paper: {ink_px}"
    );
    // The panel was set up once, in the order the chip needs.
    let cmds = commands(&log);
    assert_eq!(&cmds[..3], [0x12, 0x01, 0x11]);
    assert_eq!(*cmds.last().unwrap(), 0x20, "the refresh is the last thing");
    // The app was told what the glass is: no animation, so the first frame is also the last.
    assert!(d.shell().app.device().is_reflective());

    // Nothing changed: nothing is sent, and the driver may sleep.
    let n = d.panel_mut().bus_mut().log.len();
    let s = d.step(1000).unwrap();
    assert_eq!(s.refreshed, None);
    assert_eq!(d.panel_mut().bus_mut().log.len(), n);

    // Vello on the CPU keeps asking for frames after a focus change on this page and draws none (it does so on any
    // screen, e-paper or not; see the roadmap), so the key press is checked on the integer renderer only.
    if backend != Backend::Integer {
        return;
    }
    // A key press moves the focus: that is one small partial refresh, after the screen has settled.
    d.input(Input::Key(Key::Next));
    // A few frames for the app to settle, 10 ms apart, then it waits for the screen to be quiet.
    let (mut now, mut s) = (2000u64, d.step(2000).unwrap());
    while s.wake == fusuma::Wake::Now && now < 2090 {
        now += 10;
        s = d.step(now).unwrap();
        assert_eq!(s.refreshed, None, "not before the screen was quiet");
    }
    assert_eq!(s.refreshed, None, "waits for the settle time: {s:?}");
    assert!(matches!(s.wake, fusuma::Wake::At(ms) if ms <= 100), "{s:?}");
    let s = d.step(now + 100).unwrap();
    let Some(Refresh::Partial(r)) = s.refreshed else {
        panic!("a partial refresh: {s:?}");
    };
    assert_eq!(r.x % 8, 0, "whole bytes");
    assert!(r.w < 128 || r.h < 296, "not the whole screen: {r:?}");
    let log = d.panel_mut().bus_mut().log.clone();
    let part = data_of(&log, 0x24).pop().unwrap();
    assert_eq!(part.len(), (r.w as usize / 8) * r.h as usize);
    // The partial refresh used the partial sequence, then wrote the previous memory.
    assert_eq!(data_of(&log, 0x22).pop().unwrap(), [0x0F]);
    assert!(log.ends_with(&[Call::Command(0x26), Call::Data(part)]));
}

#[test]
fn the_showcase_on_an_e_paper_panel_with_the_integer_renderer() {
    run(Backend::Integer);
}

#[cfg(feature = "vello-cpu")]
#[test]
fn the_showcase_on_an_e_paper_panel_with_vello_on_the_cpu() {
    run(Backend::VelloCpu);
}

#[test]
fn an_idle_page_costs_the_panel_nothing_and_a_long_run_cleans_up_now_and_then() {
    let panel = Ssd1680::new(Recorder::default(), Config::EPD_2IN9);
    let mut p = policy();
    p.full_every = 3;
    let mut d = EinkDriver::new(Showcase::new(128, 296), panel, Backend::Integer, p).unwrap();
    d.step(0).unwrap();
    d.step(10).unwrap();
    assert_eq!(d.step(100).unwrap().refreshed, Some(Refresh::Full));
    let mut fulls = 1;
    let mut partials = 0;
    let mut now = 100u64;
    for i in 0..12 {
        now += 1000;
        d.input(Input::Key(if i % 2 == 0 { Key::Next } else { Key::Prev }));
        d.step(now).unwrap();
        now += 500;
        match d.step(now).unwrap().refreshed {
            Some(Refresh::Full) => fulls += 1,
            Some(Refresh::Partial(_)) => partials += 1,
            None => {}
        }
    }
    assert!(partials >= 6, "{partials} partial");
    assert!(
        fulls >= 3,
        "{fulls} full: one at the start and one after every three"
    );
    // Sleeping keeps the picture and the next refresh is a full one.
    d.sleep().unwrap();
    assert_eq!(
        *commands(&d.panel_mut().bus_mut().log).last().unwrap(),
        0x10
    );
    d.input(Input::Key(Key::Next));
    d.step(now + 10_000).unwrap();
    let s = d.step(now + 10_500).unwrap();
    assert_eq!(s.refreshed, Some(Refresh::Full));
}

#[test]
fn the_sequences_name_the_window_the_data_goes_to() {
    let mut panel = Ssd1680::new(Recorder::default(), Config::EPD_2IN9);
    panel.partial(Rect::new(8, 10, 16, 2), &[0xAA; 4]).unwrap();
    let log = &panel.bus_mut().log;
    // x range in bytes (1 to 2), y range 10 to 11, the counters at the corner.
    assert_eq!(data_of(log, 0x44).pop().unwrap(), [1, 2]);
    assert_eq!(data_of(log, 0x45).pop().unwrap(), [10, 0, 11, 0]);
    assert_eq!(data_of(log, 0x4E).pop().unwrap(), [1]);
    // Data of the wrong length is refused rather than sent crooked.
    let before = panel.bus_mut().log.len();
    panel.partial(Rect::new(8, 10, 16, 2), &[0xAA; 3]).unwrap();
    assert_eq!(panel.bus_mut().log.len(), before);
    // A region off the panel sends nothing.
    panel
        .partial(Rect::new(900, 10, 16, 2), &[0xAA; 4])
        .unwrap();
    assert_eq!(panel.bus_mut().log.len(), before);
    // Sleep twice is one deep-sleep command.
    panel.sleep().unwrap();
    panel.sleep().unwrap();
    let sleeps = commands(&panel.bus_mut().log)
        .iter()
        .filter(|&&c| c == 0x10)
        .count();
    assert_eq!(sleeps, 1);
    let _ = Bus::wait_busy(panel.bus_mut());
}
