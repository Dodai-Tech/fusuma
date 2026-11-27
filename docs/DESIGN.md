# Design files (OpenPencil and Figma)

The design is data, not a hand-drawn file: the tokens and the written spec are generated from the same `Theme` and `Palette`
constants the toolkit paints with, so the design file cannot drift from the code. Two systems are described: **cyberdeck**
(`Theme::CYBERDECK`) and **fusuma** (`Theme::FUSUMA`, sumi and washi with a vermilion accent).

| path | what | tracked |
| --- | --- | --- |
| `design/tokens/<system>/<system>.tokens.json` | tokens of the dark mode (the default mode), W3C design-token format | yes |
| `design/tokens/<system>/<system>-light.tokens.json` | the same tokens with the light colours | yes |
| `design/palettes/<palette>/<palette>[-light].tokens.json` | the 16 colour roles of each of the 27 palettes, dark and light; one collection `palette-<name>` each, drawn on the Palettes page | yes |
| `design/spec/<system>.md` | written spec: colour roles with contrast, typography, spacing and shape, motion, decoration and its fallbacks, widgets | yes |
| `design/build.js` | the drawing of the `.fig` (Figma Plugin API, run by `openpencil eval`) | yes |
| `dist/design/fusuma.fig` | design file for [OpenPencil](https://github.com/open-pencil/open-pencil), opens in Figma-compatible tools | no, build it |
| `crates/fusuma-ui/tests/design.rs` | the generator, and the test that fails when the tracked files are stale | yes |
| `scripts/design.nu` | `tokens`, `build`, `verify`, `check` | yes |

## The design file

One variable collection per system (a folder), one mode per theme (a file): colours (`color/*`), spacing, radii, text sizes,
sizes (control height, border, indicator, track height, elevation), the decoration rules (`decor/ticks`, `decor/hatch`,
`decor/dots`), motion milliseconds and the font families. Switching a mode recolours everything bound to variables. Pages per
system:

| page | what it draws |
| --- | --- |
| Palettes | one page: a dark and a light row of 16 swatches for each of the 27 palettes, bound to that palette's variables |
| Foundations | the 16 colour roles as swatches, the type scale, spacing bars, radii |
| Controls | buttons (primary, hover, pressed, secondary, disabled), the chosen and the idle list row (hatched in cyberdeck, a seal bar in fusuma), the dotted progress bar at three values, the progress round the frame, a card and a dialog with corner ticks (cyberdeck) or a lattice line (fusuma) |

Fills, strokes, radii and sizes are bound to variables. The decoration is drawn from the `decor/*` numbers, so a system with
a design without it (none today) shows solid fills and plain bars.

## Regenerate

```sh
just design                  # tokens and spec, the .fig, verify
nu scripts/design.nu tokens  # design/tokens and design/spec, from Theme and Palette
nu scripts/design.nu build   # dist/design/fusuma.fig (needs bun: runs @open-pencil/cli)
nu scripts/design.nu verify  # the .fig carries exactly the token variables
nu scripts/design.nu check   # tracked tokens and spec are up to date (no bun)
```

Run `nu scripts/design.nu tokens` and commit the result after changing a colour, a size, a hook or the decoration.

## What CI checks

| check | where | catches |
| --- | --- | --- |
| tokens and spec equal what `Theme` and `Palette` generate | `cargo test --workspace` (so `just check-all`), `design check` | a changed constant with stale files |
| the widget table is made by calling the theme's own hooks | same test | a hook change the spec does not say |
| the pixels: tick arm, hatch step and dot step on the real renderer | `the_pixels_are_what_the_spec_says` | a painter that stops following `decor` |
| the `.fig` carries every token by name and default-mode value, and a Foundations and a Controls frame per system, every palette's colours as variables, a row per palette and mode on the Palettes page, and the drawn tick arm, hatch step and dot step equal `decor/*` (`design/check.js` reads the built file) | the `design files` job (GitHub and Gitea): `design build` then `design verify` | a stale or broken `build.js`, a token the file lacks |

The job uploads the `.fig` as an artifact on GitHub. The drawing is checked by measurement, not pixel by pixel: the sheet is laid out for reading, not as a screenshot of the toolkit,
so a pixel diff would not be meaningful. The pixel check is the Rust test above, on the toolkit's own render.

## Keeping it aligned

- **Tokens** are the contract: `verify` fails when the `.fig` has a different number of variables than the token files.
- **New system**: add it to `systems()` in `crates/fusuma-ui/tests/design.rs`.
- **New decoration or token**: add it to `numbers()` there, then draw it in `design/build.js` from the variable.
- The cyberdeck definition itself is in `crates/fusuma-ui/src/cyberdeck.rs` (palette, fonts, tokens, decoration, hooks), apart from
  the widgets, so it can move to dodai-core later and a generator write the constants.

## Limits

Corner ticks, the hatch and the frame progress are drawn from rectangles and one vector, as the toolkit draws them; the `.fig`
does not animate them. The font is JetBrains Mono when the editor has it, Inter otherwise.
