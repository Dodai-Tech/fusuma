# Fusuma design spec

Named for the sliding paper door: sumi ink (dark) and washi paper (light), one vermilion accent, small corners, no shadow, thin outlines; a second thin outline inside cards and dialogs like the lattice of a shoji screen, a bar on the chosen list row like a seal, an underline under the chosen tab.

Generated from `Theme` and `Palette` by `crates/fusuma-ui/tests/design.rs`; do not edit. The tokens are in `design/tokens/fusuma/`, the design file is built with `nu scripts/design.nu build`.

## Colour roles

| role | dark | light | contrast dark | contrast light |
| --- | --- | --- | --- | --- |
| `background` | `#14110f` | `#f4eee2` | 1.0 | 1.0 |
| `surface` | `#1c1815` | `#fbf7ee` | 1.1 | 1.1 |
| `surface-raised` | `#28221e` | `#eee6d7` | 1.2 | 1.1 |
| `border` | `#423a33` | `#d8cdb8` | 1.7 | 1.4 |
| `border-strong` | `#82766a` | `#887a64` | 4.3 | 3.6 |
| `text` | `#efe8dd` | `#2a2420` | 14.5 | 14.3 |
| `text-muted` | `#a89c8f` | `#685e54` | 6.6 | 5.9 |
| `text-disabled` | `#70675e` | `#a09688` | 3.2 | 2.7 |
| `primary` | `#ec5444` | `#c8372d` | 5.0 | 4.9 |
| `primary-hover` | `#f47060` | `#b02c24` | 6.6 | 5.6 |
| `primary-active` | `#e04c3c` | `#94221c` | 4.7 | 7.3 |
| `on-primary` | `#100c0a` | `#ffffff` | 5.5 | 5.2 |
| `danger` | `#ff6888` | `#a81440` | 6.4 | 6.9 |
| `success` | `#8abe68` | `#3a6c2c` | 8.1 | 5.8 |
| `warning` | `#e2b248` | `#845c08` | 9.0 | 5.6 |
| `info` | `#8296da` | `#344696` | 6.2 | 8.0 |

Contrast is the WCAG ratio of the colour against the surface it is read on (`on-primary` on `primary`, outlines and fills against the page).

## Typography

| role | font | size |
| --- | --- | --- |
| title | Inter | 21 |
| body | Inter | 15 |
| caption | Inter | 11 |
| code | JetBrains Mono | 15 |
| icon | Lucide | 20 |

## Spacing, shape and size

| token | px |
| --- | --- |
| `decor/bar` | 4 |
| `decor/dots` | 0 |
| `decor/hatch` | 0 |
| `decor/lattice` | 4 |
| `decor/ticks` | 0 |
| `radius/lg` | 4 |
| `radius/md` | 3 |
| `radius/sm` | 2 |
| `size/border` | 1 |
| `size/control-height` | 44 |
| `size/elevation` | 0 |
| `size/indicator` | 20 |
| `size/track-height` | 6 |
| `space/lg` | 20 |
| `space/md` | 12 |
| `space/sm` | 6 |
| `space/xs` | 2 |

A control is at least 44 px tall (the smallest touch target). Shadows: none.

## Motion

fast 100 ms, normal 240 ms, slow 420 ms.

## Decoration

| piece | rule | on a glass without greys |
| --- | --- | --- |
| lattice | a 1 px outline `4` px inside the edge of a card and a dialog, in `border` mixed 80 % over the fill | kept (it is a line) |
| seal bar | a `4` px bar in `primary` on the left edge of the chosen list row, inset a sixth of the height top and bottom | kept |

## Widgets

| widget | look |
| --- | --- |
| primary button, idle | `primary` fill, no outline, `on-primary` text, radius 2 |
| primary button, hover | `primary-hover` fill, no outline, `on-primary` text, radius 2 |
| primary button, pressed | `primary-active` fill, no outline, `on-primary` text, radius 2 |
| primary button, disabled | `surface` fill, `border` outline, `text-disabled` text, radius 2 |
| secondary button, idle | `surface` fill, `border-strong` outline, `text` text, radius 2 |
| secondary button, hover | `surface-raised` fill, `border-strong` outline, `text` text, radius 2 |
| secondary button, pressed | `border` fill, `border-strong` outline, `text` text, radius 2 |
| secondary button, disabled | `surface` fill, `border` outline, `text-disabled` text, radius 2 |
| ghost button, idle | #28221e fill, no outline, `text` text, radius 2 |
| ghost button, hover | `surface-raised` fill, no outline, `text` text, radius 2 |
| ghost button, pressed | `border` fill, no outline, `text` text, radius 2 |
| ghost button, disabled | `surface` fill, `border` outline, `text-disabled` text, radius 2 |
| list item, chosen | `surface-raised` fill, `text` text |
| list item, idle | no fill, `text` text |
| list item, hover | `surface-raised` fill, `text` text |
| list item, pressed | `border` fill, `text` text |
| tab, chosen | no fill, `text` text, `primary` bar under it |
| tab, idle | no fill, `text-muted` text |
| tab, hover | `surface-raised` fill, `text` text |
| dialog | `surface` fill, `border-strong` outline, radius 3, no shadow |
| menu | `surface` fill, `border-strong` outline, radius 3, no shadow |
| toast | `surface-raised` fill, `primary` outline, radius 3, no shadow |

Source of the look: `crates/fusuma-ui/src/fusuma.rs`.
