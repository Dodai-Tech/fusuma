# fusuma-ui

[![Crates.io](https://img.shields.io/crates/v/fusuma.svg)](https://crates.io/crates/fusuma)
[![docs.rs](https://img.shields.io/docsrs/fusuma)](https://docs.rs/fusuma)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The core of the [Fusuma](https://github.com/Dodai-Tech/fusuma) toolkit, designed for the smallest
target first: `no_std`, no allocator, no floating point. If it runs on an ESP32 it runs everywhere.

**A retained tree.** `Tree<N>` is a fixed-size arena of at most `N` nodes (rows, columns, labels,
buttons, checkboxes, radio buttons, list items, progress bars, spacers). Nodes keep their state between frames: text (inline, up to 32 bytes), hover,
pressed, an animation. The application changes them through methods (`set_text`, `set_enabled`,
`remove`, ...), each marking what needs redoing, with the request travelling up to the root so a pass
can tell from the top whether anything below needs it.

**Passes** run on that tree, in this order each frame:

| Pass | Call | What it does |
|---|---|---|
| events | `pointer(event)`, `key(key)` | the press target keeps the release; clicks are queued as actions; an unhandled event goes up to the parent |
| update | `update(renderer, theme, bounds)` | disabled state flows down, layout is redone if asked for, hover is refreshed against the new layout |
| animation | `advance(theme, dt_ms)` | steps only the widgets that asked for frames; says whether to keep drawing |
| paint | `paint(renderer, theme)` or `take_damage_in(screen)` then `paint_region(renderer, theme, area)` | draws through `fusuma_render::Renderer` on any backend; the second redraws only what changed |

**Keyboard and focus.** Buttons, checkboxes, radio buttons and list items can take the focus. `key(Key::Next)`
and `Prev` (Tab) walk them in drawing order, `Down` and `Up` do too and also choose in a group of radio
buttons, `Activate` presses the focused one. A touch or a click focuses without drawing the ring; a key
draws it. Radio buttons (and list items) that share a parent are one group.

**Layout** is flex-like, in whole pixels: sizes are `Content`, `Fixed` or `Fill(weight)`, containers
have padding, a gap and a cross-axis alignment. Text is measured through the renderer, so the same
tree lays out the same on every backend.

**Theme** is plain tokens: colours as semantic roles, spacing, radii, text sizes and motion times,
with a neutral default dark and light theme (blue-grey, one accent colour, no brand). Widgets read only from the theme, so a design system can
replace it.

```rust
use fusuma_ui::{ButtonVariant, CrossAlign, Pointer, Style, TextRole, Theme, Tree};
use fusuma_render::{Point, Rect, Renderer};

let theme = Theme::DARK;
let mut tree = Tree::<8>::new();
let root = tree
    .column(None, Style::new().padding(theme.spacing.md).gap(theme.spacing.sm).align(CrossAlign::Center))
    .unwrap();
tree.label(Some(root), "Hello", TextRole::Title).unwrap();
let ok = tree.button(Some(root), "OK", ButtonVariant::Primary).unwrap();

let mut screen = fusuma_render_vello_cpu::new(160, 120).unwrap();
let bounds = Rect::new(0, 0, 160, 120);

// One frame: update, then draw what changed.
tree.update(&screen, &theme, bounds);
if tree.needs_frame() {
    screen.clear(theme.colors.background).unwrap();
    tree.paint(&mut screen, &theme).unwrap();
}

// A tap: down, up, then leave (a lifted finger hovers over nothing).
let r = tree.rect(ok).unwrap();
let tap = Point::new(r.x + 5, r.y + 5);
for e in [Pointer::Down(tap), Pointer::Up(tap), Pointer::Leave] {
    tree.pointer(e);
}
let action = tree.pop_action().unwrap();
assert_eq!(action.source, ok);
```
