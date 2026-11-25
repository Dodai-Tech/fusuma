# fusuma-palette

One seed colour in, a whole dark and light `Colors` set out (`fusuma_ui::Colors`), made in OKLCH: the neutrals take the
seed's hue at a whisper of chroma, the main action is the seed at the lightness that stands out from the page, and every
text colour is searched for the lightness at which it is **readable on the surfaces it sits on** (WCAG contrast: body text
7:1, muted text 4.5:1, text on the main action 4.5:1 in rest, hover and pressed, the main action 3:1 against the page).
Colours are pulled into the sRGB gamut by lowering chroma, never clipped.

`generate` makes the colours, `palette` a `fusuma_ui::Palette` for a host with `std`, `rust_source` the Rust of a `Palette`
constant for a table that is committed. Fusuma's own palettes are tables, so the chips pay for no float maths: this crate runs
where a palette is made (a build, a tool, a host).
