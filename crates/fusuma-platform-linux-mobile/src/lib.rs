#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

use fusuma_platform_mobile::{HostInfo, Status};

pub use fusuma_platform_mobile as mobile;

/// What this host is, and how far it has got.
pub const HOST: HostInfo = HostInfo {
    name: "linux-mobile",
    targets: &[
        "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-gnu",
    ],
    surface: "a Wayland window through winit (the desktop host); Vello on Vulkan or GL, the CPU as the fallback",
    text_input: "zwp_text_input_v3 for the on-screen keyboard, through winit's IME events",
    status: Status::Scaffold,
};

/// The phone shell a session runs under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    /// Phosh (GNOME for phones): draws the status bar and the on-screen keyboard (Squeekboard).
    Phosh,
    /// Plasma Mobile.
    PlasmaMobile,
    /// Sxmo: a tiling window manager for phones, with a keyboard of its own.
    Sxmo,
    /// Anything else: assume a desktop with a touch screen.
    Other,
}

/// The shell named by `XDG_CURRENT_DESKTOP` (a colon-separated list, compared without case).
#[must_use]
pub fn shell(xdg_current_desktop: &str) -> Shell {
    let has = |name: &str| {
        xdg_current_desktop
            .split(':')
            .any(|part| part.trim().eq_ignore_ascii_case(name))
    };
    if has("Phosh") {
        Shell::Phosh
    } else if has("Plasma-Mobile") || has("PlasmaMobile") {
        Shell::PlasmaMobile
    } else if has("sxmo") {
        Shell::Sxmo
    } else {
        Shell::Other
    }
}

impl Shell {
    /// Whether the shell provides its own on-screen keyboard (so the app only asks for text input and does not draw one).
    #[must_use]
    pub const fn has_virtual_keyboard(self) -> bool {
        !matches!(self, Self::Other)
    }

    /// Whether the shell reserves the top for its status bar, so the app's content starts below it.
    #[must_use]
    pub const fn draws_status_bar(self) -> bool {
        matches!(self, Self::Phosh | Self::PlasmaMobile)
    }
}

/// A `wl_output` transform: how the output is turned from its natural orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform {
    /// 0, 90, 180 or 270 degrees, anticlockwise.
    pub degrees: u16,
    /// Whether it is also mirrored.
    pub flipped: bool,
}

/// The transform for a `wl_output.transform` value (0 normal, 1 to 3 rotated, 4 flipped, 5 to 7 flipped and rotated).
#[must_use]
pub const fn transform(value: u32) -> Option<Transform> {
    if value > 7 {
        return None;
    }
    Some(Transform {
        degrees: ((value % 4) * 90) as u16,
        flipped: value >= 4,
    })
}

impl Transform {
    /// The size the app sees for an output of `width` x `height` pixels: width and height swap at 90 and 270 degrees.
    #[must_use]
    pub const fn apparent(self, width: u32, height: u32) -> (u32, u32) {
        if self.degrees == 90 || self.degrees == 270 {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// Whether the screen is wider than it is tall once turned (a landscape phone).
    #[must_use]
    pub const fn is_landscape(self, width: u32, height: u32) -> bool {
        let (w, h) = self.apparent(width, height);
        w > h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_is_found_in_the_desktop_list_whatever_the_case_or_company() {
        assert_eq!(shell("Phosh:GNOME"), Shell::Phosh);
        assert_eq!(shell("GNOME:phosh"), Shell::Phosh);
        assert_eq!(shell("Plasma-Mobile"), Shell::PlasmaMobile);
        assert_eq!(
            shell("KDE"),
            Shell::Other,
            "desktop Plasma is not Plasma Mobile"
        );
        assert_eq!(shell("sxmo"), Shell::Sxmo);
        assert_eq!(shell(""), Shell::Other);
        assert_eq!(shell("Phoshy"), Shell::Other, "a whole word, not a prefix");
    }

    #[test]
    fn a_phone_shell_has_its_own_keyboard_and_some_draw_a_status_bar() {
        assert!(Shell::Phosh.has_virtual_keyboard() && Shell::Phosh.draws_status_bar());
        assert!(Shell::Sxmo.has_virtual_keyboard() && !Shell::Sxmo.draws_status_bar());
        assert!(!Shell::Other.has_virtual_keyboard() && !Shell::Other.draws_status_bar());
    }

    #[test]
    fn output_transforms_turn_and_flip_and_swap_the_size() {
        assert_eq!(
            transform(0),
            Some(Transform {
                degrees: 0,
                flipped: false
            })
        );
        assert_eq!(
            transform(1),
            Some(Transform {
                degrees: 90,
                flipped: false
            })
        );
        assert_eq!(
            transform(3),
            Some(Transform {
                degrees: 270,
                flipped: false
            })
        );
        assert_eq!(
            transform(4),
            Some(Transform {
                degrees: 0,
                flipped: true
            })
        );
        assert_eq!(
            transform(7),
            Some(Transform {
                degrees: 270,
                flipped: true
            })
        );
        assert_eq!(transform(8), None);
        let upright = transform(0).unwrap();
        let sideways = transform(1).unwrap();
        assert_eq!(upright.apparent(720, 1440), (720, 1440));
        assert_eq!(sideways.apparent(720, 1440), (1440, 720));
        assert!(sideways.is_landscape(720, 1440) && !upright.is_landscape(720, 1440));
        assert_eq!(
            transform(2).unwrap().apparent(720, 1440),
            (720, 1440),
            "180 degrees keeps the shape"
        );
    }

    #[test]
    fn the_host_says_what_it_is() {
        assert_eq!(HOST.name, "linux-mobile");
        assert_eq!(
            HOST.status,
            Status::Scaffold,
            "the window still comes from the desktop host"
        );
    }
}
