# fusuma-icons

Every [Lucide](https://lucide.dev/icons/) icon (lucide-static 1.80.0, 2431 names) as one typed enum, `Icon`. A typo is
a compile error, an unused icon costs nothing, and `Icon::ALL` lists them. `no_std`, no dependencies.

```rust
use fusuma_icons::Icon;

let back = Icon::ArrowLeft;
assert_eq!(back.name(), "arrow-left");              // the name on lucide.dev
assert_eq!(Icon::from_name("arrow-left"), Some(back)); // for a name read from a file
let c: char = back.glyph();                         // its character in `FONT`
```

The variants are the lucide.dev names in PascalCase: `arrow-left` is `ArrowLeft`, `trash-2` is `Trash2`. (Lucide's
`arrow-down-0-1` and `arrow-down-01` are two icons; the first is `ArrowDown0_1`.)

In Fusuma, widgets take an `Icon` (`button("Save").icon(Icon::Save)`, `tab(..).icon(..)`, `fusuma::icon(Icon::Bell)`)
and the renderers draw it:

- **Vello** (CPU and GPU) draws it as text in `FONT`, the Lucide icon font, at any size. The font is about 1 MB, linked
  only where a renderer uses it.
- **The integer renderer** (microcontrollers) draws it from a glyph atlas made at build time, with the `atlas` feature
  (the `fusuma` crate's `icons` feature turns it on): one atlas per line height of **the icons the app uses**, not the
  whole set. Those are the toolkit's own (`fonts/toolkit.txt`) and the names in `[icons] used` of the `fusuma.toml` that
  `FUSUMA_SETTINGS` names, which stops the build on a name Lucide does not have. An icon outside the list draws
  nothing on that renderer.
- **A text screen** (terminal, serial console) has no icon font: every icon is one stand-in character.

**Updating.** `nu scripts/icons.nu fetch [--version <v>]` downloads the font and its name table and writes
`src/names.rs`, which is committed, so a build needs no network.

## Licence

The code is MIT. The Lucide font and names are ISC, with parts MIT from Feather: the text is `fonts/LICENSE-lucide.txt`,
and `LICENCE` is the same text in the code. An application that ships the font should show it.
