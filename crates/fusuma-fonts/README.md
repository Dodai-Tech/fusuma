# fusuma-fonts

The fonts Fusuma can bundle, as `&'static [u8]`, `no_std`, one cargo feature each so nobody pays flash for a font
they do not use:

| Feature | Font | Size |
|---|---|---|
| `inter` (default) | Inter Regular, the UI font | 63 KB |
| `inter-bold` | Inter Bold, titles and emphasis | 64 KB |
| `jetbrains-mono` | JetBrains Mono Regular, logs and fixed-width text | 86 KB |

These are **Latin subsets** (see `COVERAGE`: ASCII, Latin-1, Latin Extended-A, Romanian comma-below ș ț, common
punctuation, the euro sign): the full files are 270 to 420 KB each. Greek, Cyrillic, CJK and emoji are not included;
a desktop or a phone can use system fonts for those. The subsets were made with `pyftsubset` from the upstream
files (Inter 4.1, JetBrains Mono 2.304), keeping kerning and ligatures; neither licence reserves the font name.

## Licence

The code is MIT. The fonts are under the **SIL Open Font License 1.1**, which allows bundling and redistribution
with software but requires the licence text to travel with the fonts: `INTER_LICENSE` and `JETBRAINS_MONO_LICENSE`
are the texts, and the files are in `fonts/`. An application that ships these fonts should show them (an About screen
or a notices file).
