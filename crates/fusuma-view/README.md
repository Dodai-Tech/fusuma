# fusuma-view

The Elm architecture for Fusuma: you write a `State`, a `Msg` enum, an `update` that is the only place state changes,
and a `view` that describes the screen from the state. The view layer makes the nodes of a `fusuma-ui` tree match it and
touches only what differs, so layout, damage and scrolling stay incremental. `no_std`, **no allocator**.

```rust,ignore
fn view(s: &State) -> impl View<Msg> {
    card((
        text(s.title()),
        row((button("-").on_press(Msg::Dec), text_fmt(format_args!("{}", s.n)), button("+").on_press(Msg::Inc))),
        show(s.help, text("Tap + to add")),
    ))
}
```

Views are cheap values built on the stack each time and thrown away: **the tree is the previous state**, so nothing is
kept between passes. A message is a value stored next to the node that sends it, in a fixed table.
