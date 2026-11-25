//! The Overview page, written as a view: cards, a stepper, badges. It is a function of the page's look and the number of
//! seats, and the one message it can send is a press on the stepper.

use fusuma::view::{
    Children, View, avatar, badge, card, column, divider, progress, row, section, stepper, text,
};
use fusuma::{CrossAlign, Length};

/// What the page sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverviewMsg {
    /// The minus of the stepper.
    Fewer,
    /// The plus of the stepper.
    More,
}

/// How the page is laid out for the screen it is on.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// A narrow screen stacks the cards.
    pub compact: bool,
    /// A watch leaves out the title and the extras.
    pub lite: bool,
    /// The medium and the small gap.
    pub md: u32,
    /// The small gap.
    pub sm: u32,
    /// The side of the avatar.
    pub avatar: u32,
}

/// The cards of the page.
fn cards(look: Look, seats: &str) -> impl View<OverviewMsg> {
    let who = card((
        row((
            avatar("AM", look.avatar),
            column((text("Ana Maria"), text("Designer").caption())),
        ))
        .gap(look.md)
        .align(CrossAlign::Center),
        divider(),
        row((badge("Pro", true), badge("Team", false))).gap(look.sm),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let seats_card = card((
        section("Seats"),
        stepper(seats)
            .on_dec(OverviewMsg::Fewer)
            .on_inc(OverviewMsg::More),
    ))
    .padding(look.md)
    .gap(look.sm)
    .width(Length::Fill(1));
    let usage = (!look.lite).then(|| {
        card((section("Storage"), progress(640), text("6.4 of 10 GB")))
            .padding(look.md)
            .gap(look.sm)
            .width(Length::Fill(1))
    });
    // A row of cards on a wide screen, a column of them on a narrow one.
    let body = (who, seats_card, usage);
    if look.compact {
        column(body).gap(look.md)
    } else {
        row(body).gap(look.md)
    }
}

/// The children of the page container: a title, the cards, and the status badges.
pub fn page(look: Look, seats: &str) -> impl Children<OverviewMsg> {
    (
        (!look.lite).then(|| text("Overview").title()),
        cards(look, seats),
        (!look.lite).then(|| section("Status")),
        (!look.lite).then(|| {
            row((
                badge("Online", true),
                badge("Synced", false),
                badge("3 new", false),
                badge("v0.2", false),
            ))
            .gap(look.sm)
        }),
    )
}
