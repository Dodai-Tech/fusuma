#![no_main]
//! The date and time arithmetic of the calendar and time picker never panics and keeps its promises, for any year, month, day and
//! step, and the page window of a pagination is right for any page of any size.

use fusuma::{days_in_month, first_cell, is_leap, page_window, shift_month, step_time, weekday, PageSlot, TimePart, CALENDAR_CELLS};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for c in data.chunks_exact(12) {
        let year = i32::from_le_bytes([c[0], c[1], c[2], c[3]]) / 4096;
        let (month, day, delta) = (c[4], c[5], i32::from(i16::from_le_bytes([c[6], c[7]])));
        let (hour, minute) = (c[8], c[9]);
        let (current, total) = (u32::from(c[10]) * 7, u32::from(c[11]) * 3);
        let _ = is_leap(year);
        let d = days_in_month(year, month);
        assert!((28..=31).contains(&d));
        let w = weekday(year, month, day);
        assert!(w < 7, "{year}-{month}-{day}");
        // Each day moves the weekday on by one.
        if (1..12).contains(&month) && day >= 1 && day < days_in_month(year, month) {
            assert_eq!((weekday(year, month, day) + 1) % 7, weekday(year, month, day + 1));
        }
        let (y2, m2) = shift_month(year, month.clamp(1, 12), delta);
        assert!((1..=12).contains(&m2), "{m2}");
        // Shifting there and back is where it started.
        assert_eq!(shift_month(y2, m2, -delta), (year, month.clamp(1, 12)));
        for monday in [true, false] {
            let m = month.clamp(1, 12);
            assert!(usize::from(first_cell(year, m, monday)) + usize::from(days_in_month(year, m)) <= CALENDAR_CELLS);
        }
        for part in [TimePart::Hour, TimePart::Minute] {
            let (h, m) = step_time(hour % 24, minute % 60, part, delta % 100);
            assert!(h < 24 && m < 60);
        }
        let w = page_window(current, total);
        let pages: Vec<u32> = w.iter().filter_map(|s| if let PageSlot::Page(p) = s { Some(*p) } else { None }).collect();
        let t = total.max(1);
        assert!(pages.contains(&current.clamp(1, t)));
        assert!(pages.windows(2).all(|p| p[0] < p[1]));
        assert_eq!((pages[0], *pages.last().unwrap()), (1, t));
    }
});
