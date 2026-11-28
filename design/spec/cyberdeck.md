# Cyberdeck design spec

Near-black, white mono text, one vermilion accent, 1 px outlines, square corners, no shadow; corner ticks, a hatch on the chosen row, a dotted track and a progress bar round the frame.

Generated from `Theme` and `Palette` by `crates/fusuma-ui/tests/design.rs`; do not edit. The tokens are in `design/tokens/cyberdeck/`, the design file is built with `nu scripts/design.nu build`.

## Colour roles

| role | dark | light | contrast dark | contrast light |
| --- | --- | --- | --- | --- |
| `background` | `#0a0a0b` | `#f4f3ef` | 1.0 | 1.0 |
| `surface` | `#111113` | `#ffffff` | 1.0 | 1.1 |
| `surface-raised` | `#1e1e21` | `#eae8e3` | 1.2 | 1.1 |
| `border` | `#3c3c42` | `#ceccc6` | 1.8 | 1.4 |
| `border-strong` | `#96969e` | `#64625e` | 6.7 | 5.5 |
| `text` | `#f6f6f6` | `#101012` | 17.5 | 19.0 |
| `text-muted` | `#a6a6ac` | `#545250` | 7.8 | 7.8 |
| `text-disabled` | `#68686e` | `#969490` | 3.4 | 3.0 |
| `primary` | `#f04e3a` | `#c8372d` | 5.3 | 5.2 |
| `primary-hover` | `#f86e5c` | `#b02c24` | 7.0 | 5.9 |
| `primary-active` | `#e24632` | `#94221c` | 4.9 | 7.6 |
| `on-primary` | `#0a0a0b` | `#ffffff` | 5.5 | 5.2 |
| `danger` | `#ff5470` | `#ba163a` | 6.1 | 6.4 |
| `success` | `#4cd67a` | `#007636` | 10.1 | 5.8 |
| `warning` | `#ffc400` | `#865c00` | 11.8 | 5.9 |
| `info` | `#468cff` | `#005cc4` | 5.8 | 6.3 |

Contrast is the WCAG ratio of the colour against the surface it is read on (`on-primary` on `primary`, outlines and fills against the page).

## Typography

| role | font | size |
| --- | --- | --- |
| title | JetBrains Mono | 18 |
| body | JetBrains Mono | 14 |
| caption | JetBrains Mono | 10 |
| code | JetBrains Mono | 14 |
| icon | Lucide | 18 |

## Spacing, shape and size

| token | px |
| --- | --- |
| `decor/bar` | 0 |
| `decor/dots` | 3 |
| `decor/hatch` | 5 |
| `decor/lattice` | 0 |
| `decor/ticks` | 8 |
| `radius/lg` | 3 |
| `radius/md` | 2 |
| `radius/sm` | 0 |
| `size/border` | 1 |
| `size/control-height` | 44 |
| `size/elevation` | 0 |
| `size/indicator` | 18 |
| `size/track-height` | 4 |
| `space/lg` | 14 |
| `space/md` | 8 |
| `space/sm` | 4 |
| `space/xs` | 2 |

A control is at least 44 px tall (the smallest touch target). Shadows: none.

## Motion

fast 80 ms, normal 160 ms, slow 300 ms.

## Decoration

| piece | rule | on a glass without greys |
| --- | --- | --- |
| corner ticks | `8` px arms, 1 px thick, in the four corners of a card and a dialog; the card has a quiet 1 px outline between them | kept (they are lines) |
| hatch | 45 degree lines `5` px apart in `primary` thinned to 62 % over `surface`, on the chosen list row and tab | a solid `primary` fill with `on-primary` text |
| dotted track | a solid bar to the value, then a 1 px dot every `3` px in `border-strong` | a solid track |
| frame progress | a progress with room on all four sides (taller than twice `track-height`) runs round its edge, clockwise from the top left, `track-height` thick, over a `border` track | kept (it is lines) |

## Widgets

| widget | look |
| --- | --- |
| primary button, idle | `primary` fill, no outline, `on-primary` text, radius 0 |
| primary button, hover | `primary-hover` fill, no outline, `on-primary` text, radius 0 |
| primary button, pressed | `primary-active` fill, no outline, `on-primary` text, radius 0 |
| primary button, disabled | `surface` fill, `border` outline, `text-disabled` text, radius 0 |
| secondary button, idle | `surface` fill, `border-strong` outline, `text` text, radius 0 |
| secondary button, hover | `surface-raised` fill, `border-strong` outline, `text` text, radius 0 |
| secondary button, pressed | `border` fill, `border-strong` outline, `text` text, radius 0 |
| secondary button, disabled | `surface` fill, `border` outline, `text-disabled` text, radius 0 |
| ghost button, idle | #1e1e21 fill, no outline, `text` text, radius 0 |
| ghost button, hover | `surface-raised` fill, no outline, `text` text, radius 0 |
| ghost button, pressed | `border` fill, no outline, `text` text, radius 0 |
| ghost button, disabled | `surface` fill, `border` outline, `text-disabled` text, radius 0 |
| list item, chosen | `surface` fill, hatched, `text` text |
| list item, idle | no fill, `text` text |
| list item, hover | `surface-raised` fill, `text` text |
| list item, pressed | `border` fill, `text` text |
| tab, chosen | `surface` fill, hatched, `text` text, `primary` bar under it |
| tab, idle | no fill, `text-muted` text |
| tab, hover | `surface-raised` fill, `text` text |
| dialog | `surface` fill, `border-strong` outline, radius 2, no shadow, corner ticks |
| menu | `surface` fill, `border-strong` outline, radius 2, no shadow, corner ticks |
| toast | `surface-raised` fill, `primary` outline, radius 2, no shadow |

Source of the look: `crates/fusuma-ui/src/cyberdeck.rs`.
