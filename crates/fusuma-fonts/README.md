# fusuma-fonts

The font families Fusuma can bundle, for every renderer and every platform: the Vello renderers (desktop, web,
phone) draw them from their outlines through Parley, the integer renderer (microcontrollers) from glyph atlases this
crate makes at build time. `no_std`, data only.

**A family is one cargo feature, opt-in**, so nobody pays flash for a font they do not use. A sans brings Regular
and Bold, a mono one face. Sizes are of the Latin subsets:

| Feature | Family | Role | Regular + Bold |
|---|---|---|---|
| `inter` (default) | Inter | sans | 61 + 62 KB |
| `roboto` | Roboto | sans | 62 + 62 KB |
| `atkinson-hyperlegible` | Atkinson Hyperlegible Next | sans | 35 + 35 KB |
| `ibm-plex-sans` | IBM Plex Sans | sans | 74 + 75 KB |
| `noto-sans` | Noto Sans | sans | 48 + 48 KB |
| `jetbrains-mono` (default) | JetBrains Mono | mono | 83 KB |
| `roboto-mono` | Roboto Mono | mono | 33 KB |
| `atkinson-hyperlegible-mono` | Atkinson Hyperlegible Mono | mono | 23 KB |
| `ibm-plex-mono` | IBM Plex Mono | mono | 52 KB |
| `noto-sans-mono` | Noto Sans Mono | mono | 36 KB |

**Presets** pair a sans with a mono; `preset-<id>` turns both on:

| Feature | Sans and mono |
|---|---|
| `preset-fusuma` (default) | Inter, JetBrains Mono |
| `preset-material` | Roboto, Roboto Mono: the Android and Material look |
| `preset-legible` | Atkinson Hyperlegible Next and Mono: for small screens and low vision |
| `preset-plex` | IBM Plex Sans and Mono |
| `preset-noto` | Noto Sans and Noto Sans Mono |
| `all-families` | every family |

**Which linked families draw is data**, the `[fonts]` table of the app's `fusuma.toml`:

```toml
[fonts]
preset = "legible"      # default: the first preset the build linked
sans = "roboto"         # or a family per role, over the preset's
mono = "roboto-mono"
sizes = [10, 15, 20]    # line heights of the glyph atlases, read at build time
```

`FontSet::choose` turns it into the set renderers draw with (`set_fonts` on each), and says what is wrong: a family
that is not linked names the feature that links it. With the `fusuma` facade: `fonts-fusuma` (default),
`fonts-material`, `fonts-legible`, `fonts-plex`, `fonts-noto`, `fonts-all`.

**Atlases** (feature `atlas`): every glyph of every linked face rasterised to 4-bit coverage at the line heights in
`[fonts] sizes` of the file `FUSUMA_SETTINGS` names (else 10, 15 and 20), with `fusuma-text`'s em and baseline, about
40 KB a face. The integer renderer blends them; held to Vello drawing the same fonts.

## The catalog

`fonts/catalog.toml` lists every family (role, where it comes from, its axes) and every preset; the cargo features
follow it (a test keeps the two in step). Each family's files are in `fonts/<id>/`: `regular.ttf`, `bold.ttf` (sans)
and its licence `OFL.txt`. `nu scripts/fonts.nu fetch` downloads a family, cuts static instances from a variable
font (fonttools), and subsets it to `COVERAGE`: ASCII, Latin-1, Latin Extended-A, the Romanian comma-below ș ț,
common punctuation and the euro sign. Not every family has the arrows; every one has the Latin and Romanian letters
(B612 was tried and left out: no ș, ț or ă). Greek, Cyrillic, CJK and emoji are not included. A family the catalog
names is checked at build time: its file must call itself by the catalog's name. Adding a family is a catalog entry,
a cargo feature and `fonts.nu fetch`.

## Licence

The code is MIT. Every family is under the **SIL Open Font License 1.1**, which allows bundling and redistribution
with software but requires the licence text to travel with the font: `Family::licence` is the text, and the files
are in `fonts/<id>/`. An application that ships a family should show it (an About screen or a notices file).
