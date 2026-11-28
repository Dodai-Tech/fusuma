# fusuma-icons

Every [Lucide](https://lucide.dev/icons/) icon for Fusuma: the icon font (lucide-static 1.80.0, 2431 names), each icon
by the name on lucide.dev and as a constant. `no_std`, data only, drawn by every renderer and platform the same way as
any other icon font.

```rust
use fusuma_fonts::FontSet;

// Draw the icons with Lucide, found by Lucide's names.
let set = FontSet { icons: Some(&fusuma_icons::FAMILY), ..FontSet::default() };
let back = fusuma_icons::icon("arrow-left");          // Some('\u{e048}')
let house = fusuma_icons::HOUSE;                      // the same, checked at compile time
```

`fusuma-fonts` has a small `lucide` feature too: a cut of 90 icons under the Material Symbols names (`home`,
`settings`), so a design can switch the drawing style and keep its names. This crate is the whole set under Lucide's own
names.

**Size.** The font is about 1 MB, which is nothing on a desktop, a phone or the web and too much for a small chip: a
firmware that wants a few Lucide icons should link the cut in `fusuma-fonts` (or cut the font to its icons with
`pyftsubset`) instead of this crate's `FONT`.

**Updating.** `nu scripts/icons.nu fetch [--version <v>]` downloads the font and its name table and writes
`src/names.rs`, which is committed, so a build needs no network.

## Licence

The code is MIT. The Lucide font and names are ISC, with parts MIT from Feather: the text is `LICENSE-lucide.txt`,
and `LICENCE` is the same text in the code. An application that ships the font should show it.
