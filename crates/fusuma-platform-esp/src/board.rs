//! The board this firmware was built for, from its board file (see `build.rs`).

/// The board file's numbers. Every board gets every table; the ones its parts do not use stay unread.
#[allow(dead_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/board.rs"));
}
pub(crate) use generated::*;

/// The first GPIO of pin `name` in `pins`, or -1 (not wired).
pub(crate) fn pin(pins: &[(&str, &[i32])], name: &str) -> i32 {
    pins.iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, g)| g.first().copied())
        .unwrap_or(-1)
}

/// The GPIOs of pin `name` in `pins` (empty when not wired).
#[cfg_attr(not(fusuma_display = "co5300"), allow(dead_code))]
pub(crate) fn pins<'a>(pins: &[(&str, &'a [i32])], name: &str) -> &'a [i32] {
    pins.iter()
        .find(|(n, _)| *n == name)
        .map_or(&[], |(_, g)| *g)
}

/// Option `name` in `options`; the build filled in every default, so a missing one is a bug in this crate.
pub(crate) fn option(options: &[(&str, i64)], name: &str) -> i64 {
    options
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
        .unwrap_or_else(|| panic!("option {name} missing from the board's generated table"))
}

/// Whether the CPU has a floating-point unit, from the chip.
pub(crate) fn fpu() -> bool {
    fusuma_platform_embedded::Chip::from_name(CHIP).is_some_and(fusuma_platform_embedded::Chip::fpu)
}
