//! A cell and how it looks: colours, attributes and the colour depth of an output.

/// A colour of a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    /// The output's own foreground or background.
    #[default]
    Default,
    /// One of the 256 palette colours: 0 to 15 are the terminal's own 16, 16 to 231 a 6x6x6 cube, 232 to 255 greys.
    Indexed(u8),
    /// A colour of 8 bits a channel.
    Rgb(u8, u8, u8),
}

/// How many colours an output can show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Depth {
    /// 24-bit colour.
    #[default]
    TrueColor,
    /// The 256-colour palette.
    Ansi256,
    /// The 16 colours of the terminal.
    Ansi16,
    /// No colour at all: only the attributes (bold, reverse, ...) show.
    Mono,
}

/// The standard values of the 16 colours (xterm's), used to find the nearest one.
const SIXTEEN: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 0, 0),
    (0, 205, 0),
    (205, 205, 0),
    (0, 0, 238),
    (205, 0, 205),
    (0, 205, 205),
    (229, 229, 229),
    (127, 127, 127),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (92, 92, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

/// The six levels of each channel in the 256-colour cube.
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

/// The index of the cube level nearest to `v`.
fn level(v: u8) -> usize {
    (0..CUBE.len())
        .min_by_key(|&i| CUBE[i].abs_diff(v))
        .unwrap_or(0)
}

impl Color {
    /// The red, green and blue of a palette entry.
    fn rgb_of_index(i: u8) -> (u8, u8, u8) {
        match i {
            0..=15 => SIXTEEN[usize::from(i)],
            16..=231 => {
                let i = usize::from(i - 16);
                (CUBE[i / 36], CUBE[(i / 6) % 6], CUBE[i % 6])
            }
            _ => {
                let g = 8 + 10 * (i - 232);
                (g, g, g)
            }
        }
    }

    /// This colour as an output of `depth` shows it: the nearest one it has, or [`Color::Default`] on a monochrome
    /// output. A colour the output already has is kept as it is.
    #[must_use]
    pub fn at(self, depth: Depth) -> Self {
        match (self, depth) {
            (Self::Default, _) | (_, Depth::TrueColor) => self,
            (_, Depth::Mono) => Self::Default,
            (Self::Indexed(_), Depth::Ansi256) => self,
            (Self::Indexed(i), Depth::Ansi16) if i < 16 => self,
            (Self::Indexed(i), Depth::Ansi16) => Self::nearest16(Self::rgb_of_index(i)),
            (Self::Rgb(r, g, b), Depth::Ansi256) => Self::nearest256((r, g, b)),
            (Self::Rgb(r, g, b), Depth::Ansi16) => Self::nearest16((r, g, b)),
        }
    }

    fn nearest16(c: (u8, u8, u8)) -> Self {
        let i = (0..16u8)
            .min_by_key(|&i| distance(SIXTEEN[usize::from(i)], c))
            .unwrap_or(0);
        Self::Indexed(i)
    }

    fn nearest256(c: (u8, u8, u8)) -> Self {
        let (r, g, b) = (level(c.0), level(c.1), level(c.2));
        let cube = (16 + 36 * r + 6 * g + b) as u8;
        // The grey ramp is finer than the cube's greys: take it when it is nearer.
        let avg = (u32::from(c.0) + u32::from(c.1) + u32::from(c.2)) / 3;
        let step = (avg.saturating_sub(3) / 10).min(23) as u8;
        let grey = 232 + step;
        let pick = if distance(Self::rgb_of_index(grey), c) < distance(Self::rgb_of_index(cube), c)
        {
            grey
        } else {
            cube
        };
        Self::Indexed(pick)
    }
}

/// Text attributes, as a small bit set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attrs(u8);

impl Attrs {
    /// No attribute.
    pub const NONE: Self = Self(0);
    /// Bold, or bright on some terminals.
    pub const BOLD: Self = Self(1);
    /// Dim.
    pub const DIM: Self = Self(2);
    /// Italic.
    pub const ITALIC: Self = Self(4);
    /// Underlined.
    pub const UNDERLINE: Self = Self(8);
    /// Foreground and background swapped: emphasis that works on a monochrome output.
    pub const REVERSE: Self = Self(16);

    /// These attributes and `other`.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether all of `other` is set.
    #[must_use]
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// How a cell looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    /// The character's colour.
    pub fg: Color,
    /// The cell's colour.
    pub bg: Color,
    /// Bold, underline and the rest.
    pub attrs: Attrs,
}

impl Style {
    /// The output's own colours, no attributes.
    pub const PLAIN: Self = Self {
        fg: Color::Default,
        bg: Color::Default,
        attrs: Attrs::NONE,
    };

    /// The same style with another foreground.
    #[must_use]
    pub const fn fg(self, fg: Color) -> Self {
        Self { fg, ..self }
    }

    /// The same style with another background.
    #[must_use]
    pub const fn bg(self, bg: Color) -> Self {
        Self { bg, ..self }
    }

    /// The same style with `attrs` added.
    #[must_use]
    pub const fn attrs(self, attrs: Attrs) -> Self {
        Self {
            attrs: self.attrs.with(attrs),
            ..self
        }
    }
}

/// One character cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    /// The character. A control character is shown as a space.
    pub ch: char,
    /// How it looks.
    pub style: Style,
}

impl Cell {
    /// A space in the plain style.
    pub const BLANK: Self = Self {
        ch: ' ',
        style: Style::PLAIN,
    };

    /// The second column of a wide character: it shows nothing of its own, and an output writes nothing for it.
    pub const TAIL_CH: char = '\0';

    /// The second cell of a wide character written in `style`.
    #[must_use]
    pub const fn tail(style: Style) -> Self {
        Self {
            ch: Self::TAIL_CH,
            style,
        }
    }

    /// Whether this is the second column of a wide character.
    #[must_use]
    pub const fn is_tail(&self) -> bool {
        self.ch == Self::TAIL_CH
    }

    /// `ch` in `style`.
    #[must_use]
    pub const fn new(ch: char, style: Style) -> Self {
        Self { ch, style }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::BLANK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_the_output_has_is_kept() {
        let c = Color::Rgb(12, 34, 56);
        assert_eq!(c.at(Depth::TrueColor), c);
        assert_eq!(Color::Indexed(200).at(Depth::Ansi256), Color::Indexed(200));
        assert_eq!(Color::Indexed(9).at(Depth::Ansi16), Color::Indexed(9));
        assert_eq!(Color::Default.at(Depth::Ansi16), Color::Default);
    }

    #[test]
    fn a_monochrome_output_drops_every_colour() {
        for c in [Color::Rgb(1, 2, 3), Color::Indexed(4), Color::Default] {
            assert_eq!(c.at(Depth::Mono), Color::Default);
        }
    }

    #[test]
    fn rgb_lands_on_the_nearest_palette_colour() {
        assert_eq!(Color::Rgb(255, 0, 0).at(Depth::Ansi16), Color::Indexed(9));
        assert_eq!(Color::Rgb(0, 0, 0).at(Depth::Ansi16), Color::Indexed(0));
        assert_eq!(
            Color::Rgb(255, 255, 255).at(Depth::Ansi256),
            Color::Indexed(231)
        );
        assert_eq!(
            Color::Rgb(95, 135, 175).at(Depth::Ansi256),
            Color::Indexed(16 + 36 + 12 + 3)
        );
        // A mid grey is nearer a step of the grey ramp than any cube colour.
        assert_eq!(
            Color::Rgb(128, 128, 128).at(Depth::Ansi256),
            Color::Indexed(244)
        );
        // A cube colour taken down to 16 lands near it too.
        assert_eq!(Color::Indexed(196).at(Depth::Ansi16), Color::Indexed(9));
    }

    #[test]
    fn every_palette_index_maps_back_to_itself_at_256() {
        for i in 16..=255u8 {
            let (r, g, b) = Color::rgb_of_index(i);
            let back = Color::Rgb(r, g, b).at(Depth::Ansi256);
            let Color::Indexed(j) = back else {
                panic!("{back:?}")
            };
            assert_eq!(Color::rgb_of_index(j), (r, g, b), "{i} -> {j}");
        }
    }

    #[test]
    fn attributes_combine_and_styles_build() {
        let a = Attrs::BOLD.with(Attrs::REVERSE);
        assert!(a.has(Attrs::BOLD) && a.has(Attrs::REVERSE) && !a.has(Attrs::DIM));
        let s = Style::PLAIN
            .fg(Color::Indexed(2))
            .bg(Color::Rgb(1, 2, 3))
            .attrs(Attrs::UNDERLINE);
        assert_eq!(
            (s.fg, s.bg, s.attrs),
            (Color::Indexed(2), Color::Rgb(1, 2, 3), Attrs::UNDERLINE)
        );
        assert_eq!(Cell::default(), Cell::new(' ', Style::PLAIN));
    }
}
