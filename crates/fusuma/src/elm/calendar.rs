//! A month calendar, a date picker made of it, and a time picker: plain arithmetic on dates (no clock, no allocation, no float)
//! and views built from the grid and the buttons that exist.

use core::fmt::Write;

use fusuma_ui::{
    BuildError, ButtonVariant, CrossAlign, Length, NodeId, SmallText, Style, TextRole,
};

use super::{Cx, Handler, View, ViewMarker};

/// The names of the months, in English.
pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Two-letter names of the days, from Monday.
pub const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// Whether `year` has a 29th of February.
#[must_use]
pub const fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// How many days `month` (1 to 12; others are held to the ends) of `year` has.
#[must_use]
pub const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        0 => 31,
        1..=12 => 31,
        _ => 31,
    }
}

/// The day of the week of a date, 0 for Monday to 6 for Sunday (Sakamoto's method, for the Gregorian calendar).
#[must_use]
pub const fn weekday(year: i32, month: u8, day: u8) -> u8 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let m = if month < 1 {
        1
    } else if month > 12 {
        12
    } else {
        month
    };
    let y = if m < 3 { year - 1 } else { year };
    // 0 is Sunday here.
    let sunday_first = (y + y.div_euclid(4) - y.div_euclid(100)
        + y.div_euclid(400)
        + T[(m - 1) as usize]
        + day as i32)
        .rem_euclid(7);
    ((sunday_first + 6) % 7) as u8
}

/// The month `delta` months from `month` of `year`.
#[must_use]
pub const fn shift_month(year: i32, month: u8, delta: i32) -> (i32, u8) {
    let index = year * 12 + (month as i32 - 1) + delta;
    (index.div_euclid(12), (index.rem_euclid(12) + 1) as u8)
}

/// How many cells a month grid has: six weeks of seven days, enough for any month.
pub const CALENDAR_CELLS: usize = 42;

/// Which cell of a grid that starts its weeks on Monday (`monday_first`) or Sunday holds the first of the month.
#[must_use]
pub const fn first_cell(year: i32, month: u8, monday_first: bool) -> u8 {
    let w = weekday(year, month, 1);
    if monday_first { w } else { (w + 1) % 7 }
}

/// A month: arrows and the month's name above a row of weekday names and the days.
#[derive(Clone, Copy, Debug)]
pub struct Calendar<M> {
    year: i32,
    month: u8,
    selected: Option<u8>,
    monday_first: bool,
    on_pick: fn(u8) -> M,
    on_month: fn(i32) -> M,
}

/// A calendar of `month` (1 to 12) of `year` with `selected` (a day of the month) marked. `on_pick` makes a message of the day
/// that was pressed, `on_month` of -1 or +1 for the arrows (the app moves the month with [`shift_month`]).
#[must_use]
pub const fn calendar<M>(
    year: i32,
    month: u8,
    selected: Option<u8>,
    on_pick: fn(u8) -> M,
    on_month: fn(i32) -> M,
) -> Calendar<M> {
    Calendar {
        year,
        month,
        selected,
        monday_first: true,
        on_pick,
        on_month,
    }
}

impl<M> Calendar<M> {
    /// Weeks start on Sunday, as in the United States (Monday by default).
    #[must_use]
    pub const fn sunday_first(mut self) -> Self {
        self.monday_first = false;
        self
    }
}

impl<M: Clone> Calendar<M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, root: NodeId) {
        let month = self.month.clamp(1, 12);
        // The header: previous, the title, next.
        let Some(head) = cx.tree.first_child(root) else {
            return;
        };
        let prev = cx.tree.first_child(head);
        let title = prev.and_then(|p| cx.tree.next_sibling(p));
        let next = title.and_then(|t| cx.tree.next_sibling(t));
        if let (Some(p), Some(n)) = (prev, next) {
            cx.msgs.on(p, Handler::Press((self.on_month)(-1)));
            cx.msgs.on(n, Handler::Press((self.on_month)(1)));
        }
        if let Some(t) = title {
            let mut text = SmallText::EMPTY;
            let _ = write!(text, "{} {}", MONTHS[usize::from(month - 1)], self.year);
            cx.tree.set_text(t, text.as_str());
        }
        // The weekday names.
        let names = cx.tree.next_sibling(head);
        if let Some(names) = names {
            let mut at = cx.tree.first_child(names);
            for k in 0..7usize {
                let Some(l) = at else { break };
                let day = if self.monday_first { k } else { (k + 6) % 7 };
                cx.tree.set_text(l, WEEKDAYS[day]);
                at = cx.tree.next_sibling(l);
            }
        }
        // The days: 42 cells, the ones outside the month empty.
        let Some(days) = names.and_then(|n| cx.tree.next_sibling(n)) else {
            return;
        };
        let first = usize::from(first_cell(self.year, month, self.monday_first));
        let count = usize::from(days_in_month(self.year, month));
        let mut at = cx.tree.first_child(days);
        for cell in 0..CALENDAR_CELLS {
            let Some(slot) = at else { break };
            let day = cell
                .checked_sub(first)
                .filter(|d| *d < count)
                .map(|d| d as u8 + 1);
            if let Some(button) = cx.tree.first_child(slot) {
                match day {
                    Some(d) => {
                        let mut text = SmallText::EMPTY;
                        let _ = write!(text, "{d}");
                        cx.tree.set_text(button, text.as_str());
                        cx.tree.set_hidden(button, false);
                        cx.tree.set_variant(
                            button,
                            if self.selected == Some(d) {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                        );
                        cx.msgs.on(button, Handler::Press((self.on_pick)(d)));
                    }
                    None => cx.tree.set_hidden(button, true),
                }
            }
            at = cx.tree.next_sibling(slot);
        }
    }
}

impl<M: Clone> View<M> for Calendar<M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let root = cx
            .tree
            .column(parent, Style::new().gap(4).align(CrossAlign::Stretch))?;
        let head = cx
            .tree
            .row(Some(root), Style::new().gap(8).align(CrossAlign::Center))?;
        cx.tree.button(Some(head), "<", ButtonVariant::Secondary)?;
        let title = cx.tree.label(Some(head), "", TextRole::Body)?;
        cx.tree
            .set_style(title, Style::new().width(Length::Fill(1)));
        cx.tree.button(Some(head), ">", ButtonVariant::Secondary)?;
        let names = cx
            .tree
            .grid(Some(root), 7, Style::new().width(Length::Fill(1)).gap(2))?;
        for _ in 0..7 {
            cx.tree.label(Some(names), "", TextRole::Caption)?;
        }
        let days = cx.tree.grid(
            Some(root),
            7,
            Style::new()
                .width(Length::Fill(1))
                .gap(2)
                .align(CrossAlign::Stretch),
        )?;
        for _ in 0..CALENDAR_CELLS {
            // A cell that is always there (so a blank day keeps its column), holding a button that is hidden when blank.
            let slot = cx
                .tree
                .column(Some(days), Style::new().align(CrossAlign::Stretch))?;
            cx.tree.button(Some(slot), "", ButtonVariant::Secondary)?;
        }
        self.sync(cx, root);
        Ok(root)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Calendar<M> {}

/// Which part of a time a step is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimePart {
    /// The hour.
    Hour,
    /// The minute.
    Minute,
}

/// Minutes a step of the minute moves by.
pub const MINUTE_STEP: i32 = 5;

/// The time after a step: `part` moved by `delta` (hours by one, minutes by [`MINUTE_STEP`]), wrapping round (23 to 0, and
/// 55 to 0 without moving the hour).
#[must_use]
pub const fn step_time(hour: u8, minute: u8, part: TimePart, delta: i32) -> (u8, u8) {
    let h = hour as i32;
    let m = minute as i32;
    match part {
        TimePart::Hour => ((h + delta).rem_euclid(24) as u8, minute),
        TimePart::Minute => (hour, (m + delta * MINUTE_STEP).rem_euclid(60) as u8),
    }
}

/// An hour and a minute, each with a minus and a plus: `[-] 09 [+] : [-] 30 [+]`.
#[derive(Clone, Copy, Debug)]
pub struct TimePicker<M> {
    hour: u8,
    minute: u8,
    on_step: fn(TimePart, i32) -> M,
}

/// A time picker showing `hour` (0 to 23) and `minute` (0 to 59). `on_step` makes a message of the part and the direction
/// (-1 or +1); the app moves the time with [`step_time`].
#[must_use]
pub const fn time_picker<M>(
    hour: u8,
    minute: u8,
    on_step: fn(TimePart, i32) -> M,
) -> TimePicker<M> {
    TimePicker {
        hour,
        minute,
        on_step,
    }
}

impl<M: Clone> TimePicker<M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, root: NodeId) {
        // Children: minus, hour, plus, colon, minus, minute, plus.
        let mut at = cx.tree.first_child(root);
        let mut next = |cx: &Cx<'_, M, N, K>| {
            let n = at;
            at = n.and_then(|n| cx.tree.next_sibling(n));
            n
        };
        for (part, value) in [(TimePart::Hour, self.hour), (TimePart::Minute, self.minute)] {
            let (Some(minus), Some(text), Some(plus)) = (next(cx), next(cx), next(cx)) else {
                return;
            };
            let mut s = SmallText::EMPTY;
            let _ = write!(s, "{value:02}");
            cx.tree.set_text(text, s.as_str());
            cx.msgs.on(minus, Handler::Press((self.on_step)(part, -1)));
            cx.msgs.on(plus, Handler::Press((self.on_step)(part, 1)));
            if part == TimePart::Hour {
                let _ = next(cx);
            }
        }
    }
}

impl<M: Clone> View<M> for TimePicker<M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let root = cx
            .tree
            .row(parent, Style::new().gap(6).align(CrossAlign::Center))?;
        for part in [TimePart::Hour, TimePart::Minute] {
            cx.tree.button(Some(root), "-", ButtonVariant::Secondary)?;
            cx.tree.label(Some(root), "00", TextRole::Title)?;
            cx.tree.button(Some(root), "+", ButtonVariant::Secondary)?;
            if part == TimePart::Hour {
                cx.tree.label(Some(root), ":", TextRole::Title)?;
            }
        }
        self.sync(cx, root);
        Ok(root)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for TimePicker<M> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_years_and_month_lengths_follow_the_gregorian_rules() {
        for (y, leap) in [
            (1900, false),
            (2000, true),
            (2023, false),
            (2024, true),
            (2100, false),
            (1600, true),
            (0, true),
            (-4, true),
        ] {
            assert_eq!(is_leap(y), leap, "{y}");
            assert_eq!(days_in_month(y, 2), if leap { 29 } else { 28 }, "{y}");
        }
        let long = [1, 3, 5, 7, 8, 10, 12];
        for m in 1..=12u8 {
            let d = days_in_month(2025, m);
            assert_eq!(
                d,
                if m == 2 {
                    28
                } else if long.contains(&m) {
                    31
                } else {
                    30
                },
                "month {m}"
            );
        }
        // Out of range months do not panic.
        assert_eq!(days_in_month(2025, 0), 31);
        assert_eq!(days_in_month(2025, 99), 31);
    }

    #[test]
    fn the_weekday_of_known_dates_is_right_with_monday_as_zero() {
        for (y, m, d, w) in [
            (2000, 1, 1, 5),  // Saturday
            (2024, 2, 29, 3), // Thursday
            (1970, 1, 1, 3),  // Thursday
            (2026, 11, 26, 3),
            (1999, 12, 31, 4), // Friday
            (2023, 3, 1, 2),   // Wednesday
            (2100, 3, 1, 0),   // Monday
            (1900, 1, 1, 0),   // Monday
        ] {
            assert_eq!(weekday(y, m, d), w, "{y}-{m}-{d}");
        }
        // Seven days later is the same weekday, for every day of a leap year.
        for m in 1..=12u8 {
            for d in 1..=days_in_month(2024, m).saturating_sub(7) {
                assert_eq!(weekday(2024, m, d), weekday(2024, m, d + 7));
            }
        }
        // Each day moves the weekday on by one.
        assert_eq!((weekday(2025, 6, 1) + 1) % 7, weekday(2025, 6, 2));
        assert_eq!((weekday(2025, 6, 30) + 1) % 7, weekday(2025, 7, 1));
    }

    #[test]
    fn months_shift_across_years_both_ways() {
        assert_eq!(shift_month(2025, 12, 1), (2026, 1));
        assert_eq!(shift_month(2025, 1, -1), (2024, 12));
        assert_eq!(shift_month(2025, 6, 0), (2025, 6));
        assert_eq!(shift_month(2025, 3, -15), (2023, 12));
        assert_eq!(shift_month(2025, 3, 25), (2027, 4));
        assert_eq!(shift_month(0, 1, -1), (-1, 12));
    }

    #[test]
    fn the_first_of_the_month_is_in_the_right_cell_for_either_start_of_the_week() {
        // 1 June 2025 is a Sunday.
        assert_eq!(first_cell(2025, 6, true), 6);
        assert_eq!(first_cell(2025, 6, false), 0);
        // 1 January 2024 is a Monday.
        assert_eq!(first_cell(2024, 1, true), 0);
        assert_eq!(first_cell(2024, 1, false), 1);
        // Every month fits in 42 cells.
        for y in [2023, 2024, 2100] {
            for m in 1..=12 {
                for monday in [true, false] {
                    let end =
                        usize::from(first_cell(y, m, monday)) + usize::from(days_in_month(y, m));
                    assert!(end <= CALENDAR_CELLS, "{y}-{m}");
                }
            }
        }
    }

    #[test]
    fn the_time_wraps_round_and_minutes_move_in_fives_without_touching_the_hour() {
        assert_eq!(step_time(23, 30, TimePart::Hour, 1), (0, 30));
        assert_eq!(step_time(0, 30, TimePart::Hour, -1), (23, 30));
        assert_eq!(step_time(9, 55, TimePart::Minute, 1), (9, 0));
        assert_eq!(step_time(9, 0, TimePart::Minute, -1), (9, 55));
        assert_eq!(step_time(9, 32, TimePart::Minute, 1), (9, 37));
        assert_eq!(step_time(9, 30, TimePart::Hour, 0), (9, 30));
        for h in 0..24u8 {
            for m in 0..60u8 {
                for part in [TimePart::Hour, TimePart::Minute] {
                    for d in [-1, 1] {
                        let (a, b) = step_time(h, m, part, d);
                        assert!(a < 24 && b < 60);
                    }
                }
            }
        }
    }
}
