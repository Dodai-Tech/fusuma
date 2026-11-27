# Fusuma design spec

The default: a neutral blue-grey with a blue main action, rounded corners, soft shadow under floating panels.

Generated from `Theme` and `Palette` by `crates/fusuma-ui/tests/design.rs`; do not edit. The tokens are in `design/tokens/fusuma/`, the design file is built with `nu scripts/design.nu build`.

## Colour roles

| role | dark | light | contrast dark | contrast light |
| --- | --- | --- | --- | --- |
| `background` | `#101218` | `#f5f6fa` | 1.0 | 1.0 |
| `surface` | `#1a1d24` | `#ffffff` | 1.1 | 1.1 |
| `surface-raised` | `#262a34` | `#eceef4` | 1.3 | 1.1 |
| `border` | `#3c4250` | `#c8cdd8` | 1.9 | 1.5 |
| `border-strong` | `#6e788c` | `#788091` | 4.2 | 3.7 |
| `text` | `#ebeef5` | `#141822` | 14.5 | 17.7 |
| `text-muted` | `#a0a8b9` | `#586070` | 7.1 | 6.3 |
| `text-disabled` | `#646b7a` | `#969caa` | 3.2 | 2.8 |
| `primary` | `#60a5fa` | `#1e64dc` | 6.6 | 5.4 |
| `primary-hover` | `#7db9fc` | `#266ee1` | 9.1 | 4.4 |
| `primary-active` | `#468ceb` | `#1650be` | 5.5 | 6.6 |
| `on-primary` | `#080c18` | `#ffffff` | 7.7 | 5.4 |
| `danger` | `#f05a5a` | `#be2828` | 5.1 | 6.0 |
| `success` | `#5ac576` | `#007d37` | 7.8 | 5.3 |
| `warning` | `#f0b135` | `#8e6402` | 8.9 | 5.3 |
| `info` | `#61b0fe` | `#006ebe` | 7.3 | 5.3 |

Contrast is the WCAG ratio of the colour against the surface it is read on (`on-primary` on `primary`, outlines and fills against the page).

## Typography

| role | font | size |
| --- | --- | --- |
| title | Inter | 20 |
| body | Inter | 15 |
| caption | Inter | 10 |
| code | JetBrains Mono | 15 |
| icon | Material Symbols | 20 |

## Spacing, shape and size

| token | px |
| --- | --- |
| `decor/dots` | 0 |
| `decor/hatch` | 0 |
| `decor/ticks` | 0 |
| `radius/lg` | 16 |
| `radius/md` | 10 |
| `radius/sm` | 4 |
| `size/border` | 1 |
| `size/control-height` | 44 |
| `size/elevation` | 8 |
| `size/indicator` | 20 |
| `size/track-height` | 8 |
| `space/lg` | 20 |
| `space/md` | 12 |
| `space/sm` | 6 |
| `space/xs` | 2 |

A control is at least 44 px tall (the smallest touch target). Shadows: 8 px blur under floating panels.

## Motion

fast 100 ms, normal 220 ms, slow 400 ms.

## Decoration

None: every widget is as the tokens say.

## Widgets

| widget | look |
| --- | --- |
| every widget | the tokens above, no hooks |

Source of the look: `crates/fusuma-ui/src/theme.rs`.
