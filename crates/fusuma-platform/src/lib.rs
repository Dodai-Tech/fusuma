#![cfg_attr(not(test), no_std)]

//! Which renderer a machine draws with: the backends, the hardware facts that decide it and the layers
//! that choose it. Pure logic with no UI and no dependencies, so a platform crate (a board, a desktop, a
//! phone), a build script and a flashing wizard can all ask the same question and show the same answer.
//!
//! Layers, strongest first: the **build** (a cargo feature or a command-line flag), the **settings**
//! (a board file's `[render] backend`, or a stored setting), then an **automatic** rule over the hardware
//! facts. Whatever wins, the [`Choice`] says why, so an override is visible.

/// What the choice is about: the screen, what the chip has, and what the settings say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    /// The screen size in pixels; `None` for a device without one (nothing to draw on).
    pub screen: Option<(u32, u32)>,
    /// What the machine has, if known.
    pub hardware: Option<Hardware>,
    /// What the settings say about rendering.
    pub render: Render,
}

/// A renderer backend. Each is a crate of Fusuma and passes the same conformance suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    /// `fusuma-render-embedded`: integer drawing straight into RGB565; the lightest and fastest, and the one for
    /// chips where Vello has no SIMD (every microcontroller).
    Integer,
    /// `fusuma-render-vello-cpu`: the richest CPU backend, for machines where Vello has SIMD.
    VelloCpu,
    /// `fusuma-render-vello`: Vello on a GPU through `wgpu`.
    VelloGpu,
}

impl Backend {
    /// Every backend, lightest first.
    pub const ALL: [Self; 3] = [Self::Integer, Self::VelloCpu, Self::VelloGpu];

    /// The name used in device files and on command lines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::VelloCpu => "vello-cpu",
            Self::VelloGpu => "vello-gpu",
        }
    }

    /// The backend called `name`, if there is one.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == name)
    }

    /// The Rust path of the variant, for generated code.
    #[must_use]
    pub const fn variant(self) -> &'static str {
        match self {
            Self::Integer => "Integer",
            Self::VelloCpu => "VelloCpu",
            Self::VelloGpu => "VelloGpu",
        }
    }

    /// The Fusuma crate that implements it.
    #[must_use]
    pub const fn crate_name(self) -> &'static str {
        match self {
            Self::Integer => "fusuma-render-embedded",
            Self::VelloCpu => "fusuma-render-vello-cpu",
            Self::VelloGpu => "fusuma-render-vello",
        }
    }

    /// The next lighter backend, for a fallback; `None` for the lightest.
    #[must_use]
    pub const fn lighter(self) -> Option<Self> {
        match self {
            Self::Integer => None,
            Self::VelloCpu => Some(Self::Integer),
            Self::VelloGpu => Some(Self::VelloCpu),
        }
    }

    /// RAM the backend needs for a surface of `width` x `height`, in bytes. Measured on an ESP32-C6
    /// and an ESP32-S3 (`tools/bench-c6/RESULT.md`, `tools/bench-s3/RESULT.md` in Hibari): the integer
    /// renderer's frame is 2 bytes per pixel, `vello_cpu` about 4.8 with its working buffers. The GPU backend keeps its pixels in video memory, so it needs none here.
    #[must_use]
    pub const fn memory(self, width: u32, height: u32) -> u64 {
        // u32 x u32 fits in a u64, but the bytes per pixel may not: saturate.
        let pixels = width as u64 * height as u64;
        match self {
            Self::Integer => pixels.saturating_mul(2),
            Self::VelloCpu => pixels.saturating_mul(48) / 10,
            Self::VelloGpu => 0,
        }
    }
}

/// What a chip has that decides what it can draw with. A board file lists them; none is detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hardware {
    /// Internal RAM in kilobytes (about 512 on an ESP32-S3 or ESP32-C6).
    pub ram_kb: u32,
    /// External PSRAM in kilobytes, 0 for none.
    pub psram_kb: u32,
    /// Whether the chip has a floating-point unit.
    pub fpu: bool,
    /// Whether Vello can use SIMD here: x86 and x86_64 (SSE, AVX), aarch64 (NEON) and WebAssembly (`simd128`),
    /// the levels `fearless_simd` has. Every microcontroller runs Vello's scalar fallback, several times slower
    /// (an ESP32-S3 has SIMD of its own, which Vello does not use).
    pub simd: bool,
    /// Whether there is a GPU that `wgpu` can use.
    pub gpu: bool,
}

/// Internal RAM, in kilobytes, that must stay free after a frame is placed in it, unless a setting says
/// otherwise: room for the stack, the network stack and the app. **A conservative guess, not a
/// measurement.** The real figure is what WiFi, Bluetooth and the app need at their peak; read it from the
/// heap's minimum-free watermark while the firmware runs, then set `min_free_kb`.
pub const DEFAULT_MIN_FREE_KB: u32 = 100;

/// What the settings say about rendering, resolved: every field has a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Render {
    /// A backend to use whatever the rule would pick; `None` is automatic.
    pub backend: Option<Backend>,
    /// Whether to step down to a lighter backend if the chosen one does not fit in memory.
    pub fallback: bool,
    /// Internal RAM that must stay free after the frame, in kilobytes (see [`DEFAULT_MIN_FREE_KB`]).
    pub min_free_kb: u32,
}

impl Default for Render {
    /// Automatic, with a fallback, and [`DEFAULT_MIN_FREE_KB`].
    fn default() -> Self {
        Self {
            backend: None,
            fallback: true,
            min_free_kb: DEFAULT_MIN_FREE_KB,
        }
    }
}

/// The `backend` setting of one layer: leave it to the rule, or name one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendSetting {
    /// `"auto"`: the automatic rule. Written explicitly, it lets a stronger layer undo a weaker one's pick.
    Auto,
    /// A named backend.
    Fixed(Backend),
}

/// One layer of settings, as a file or the build says it: a field left out says nothing, so the layer
/// below it decides. Layers are combined with [`Render::layered`], weakest first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderLayer {
    /// `backend`, if the layer says.
    pub backend: Option<BackendSetting>,
    /// `fallback`, if the layer says.
    pub fallback: Option<bool>,
    /// `min_free_kb`, if the layer says.
    pub min_free_kb: Option<u32>,
}

impl RenderLayer {
    /// A layer that says nothing.
    pub const EMPTY: Self = Self {
        backend: None,
        fallback: None,
        min_free_kb: None,
    };

    /// This layer over a weaker one: what this one says wins, the rest comes from `weaker`.
    #[must_use]
    pub const fn over(self, weaker: Self) -> Self {
        Self {
            backend: if self.backend.is_some() {
                self.backend
            } else {
                weaker.backend
            },
            fallback: if self.fallback.is_some() {
                self.fallback
            } else {
                weaker.fallback
            },
            min_free_kb: if self.min_free_kb.is_some() {
                self.min_free_kb
            } else {
                weaker.min_free_kb
            },
        }
    }

    /// The settings with the defaults filled in for what no layer said.
    #[must_use]
    pub const fn resolve(self) -> Render {
        Render {
            backend: match self.backend {
                Some(BackendSetting::Fixed(b)) => Some(b),
                _ => None,
            },
            fallback: match self.fallback {
                Some(f) => f,
                None => true,
            },
            min_free_kb: match self.min_free_kb {
                Some(k) => k,
                None => DEFAULT_MIN_FREE_KB,
            },
        }
    }
}

impl Render {
    /// The settings from `layers`, weakest first (for example the project file, then the user's, then the
    /// build's): the last layer that says something wins, and what none says is the default.
    #[must_use]
    pub fn layered(layers: &[RenderLayer]) -> Self {
        layers
            .iter()
            .fold(RenderLayer::EMPTY, |weaker, &l| l.over(weaker))
            .resolve()
    }
}

#[cfg(feature = "toml")]
extern crate std;
#[cfg(feature = "toml")]
mod settings;
#[cfg(feature = "toml")]
pub use settings::{RawRender, SETTINGS_SCHEMA, SettingsError};
#[cfg(feature = "toml")]
pub mod board;

/// Why a backend was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The build asked for it (a cargo feature or a flag).
    Build,
    /// The board file asked for it.
    Board,
    /// The automatic rule picked it; the text says which part of the rule.
    Automatic(&'static str),
}

impl core::fmt::Display for Reason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Build => f.write_str("asked for by the build"),
            Self::Board => f.write_str("set in the settings"),
            Self::Automatic(rule) => write!(f, "automatic: {rule}"),
        }
    }
}

/// A backend and why it was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    /// The backend.
    pub backend: Backend,
    /// Why.
    pub reason: Reason,
}

/// The memory one frame may use, in bytes: the larger of the PSRAM (a frame placed there costs internal RAM
/// nothing) and the internal RAM left after keeping `min_free_kb` free.
#[must_use]
pub fn budget(hw: &Hardware, min_free_kb: u32) -> u64 {
    let psram = u64::from(hw.psram_kb) * 1024;
    let internal = u64::from(hw.ram_kb.saturating_sub(min_free_kb)) * 1024;
    psram.max(internal)
}

/// Whether `backend` fits a surface of `width` x `height` in `budget_bytes`.
#[must_use]
pub const fn fits(backend: Backend, width: u32, height: u32, budget_bytes: u64) -> bool {
    backend.memory(width, height) <= budget_bytes
}

/// The automatic rule, with a reason: Vello on the GPU when there is one; Vello on the CPU where it has SIMD and
/// its frame fits; else the integer renderer, which is several times faster than Vello's scalar fallback
/// (measured on the S3 watch: a tap 7 ms against 47, a first frame 97 ms against 558). `None` for a device without
/// a screen.
#[must_use]
pub fn automatic(machine: &Machine) -> Option<Choice> {
    let (w, h) = machine.screen?;
    let auto = |backend, why| {
        Some(Choice {
            backend,
            reason: Reason::Automatic(why),
        })
    };
    let Some(hw) = machine.hardware else {
        return auto(
            Backend::Integer,
            "no hardware facts in the board file, so the lightest renderer",
        );
    };
    if hw.gpu {
        return auto(Backend::VelloGpu, "the device has a GPU");
    }
    if !hw.simd {
        return auto(
            Backend::Integer,
            "Vello has no SIMD on this chip, so the integer renderer is several times faster",
        );
    }
    if fits(
        Backend::VelloCpu,
        w,
        h,
        budget(&hw, machine.render.min_free_kb),
    ) {
        return auto(Backend::VelloCpu, "Vello has SIMD here and its frame fits");
    }
    auto(
        Backend::Integer,
        "Vello's frame does not fit the memory left after the free minimum",
    )
}

/// The layers: the build's choice beats the board file's, which beats the automatic rule. `None` for a
/// device without a screen.
#[must_use]
pub fn choose(machine: &Machine, build: Option<Backend>) -> Option<Choice> {
    machine.screen?;
    if let Some(backend) = build {
        return Some(Choice {
            backend,
            reason: Reason::Build,
        });
    }
    if let Some(backend) = machine.render.backend {
        return Some(Choice {
            backend,
            reason: Reason::Board,
        });
    }
    automatic(machine)
}

/// What the memory check says about a choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// The frame fits the memory budget (or there is nothing to check: a GPU, or no hardware facts).
    Fits,
    /// It does not fit and the board file did not allow a fallback: use it and warn.
    TooBig {
        /// Bytes the frame needs.
        need: u64,
        /// Bytes the device offers.
        budget: u64,
    },
    /// It did not fit and a lighter backend does: use that one instead.
    FellBack {
        /// The backend that was chosen first.
        from: Backend,
        /// The lighter one that fits.
        to: Backend,
    },
}

/// Checks a choice against the device's memory. An explicit choice (build or board) is kept even when
/// it does not fit unless the board file sets `fallback = true`; the automatic rule already fits.
#[must_use]
pub fn check(machine: &Machine, choice: Choice) -> (Choice, Fit) {
    let (Some((w, h)), Some(hw)) = (machine.screen, machine.hardware) else {
        return (choice, Fit::Fits);
    };
    let budget = budget(&hw, machine.render.min_free_kb);
    if choice.backend == Backend::VelloGpu || fits(choice.backend, w, h, budget) {
        return (choice, Fit::Fits);
    }
    if machine.render.fallback {
        let mut b = choice.backend;
        while let Some(lighter) = b.lighter() {
            b = lighter;
            if fits(b, w, h, budget) {
                return (
                    Choice {
                        backend: b,
                        reason: choice.reason,
                    },
                    Fit::FellBack {
                        from: choice.backend,
                        to: b,
                    },
                );
            }
        }
    }
    (
        choice,
        Fit::TooBig {
            need: choice.backend.memory(w, h),
            budget,
        },
    )
}

/// How a device will really draw: what the layers chose, whether it fits and what is built in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The backend the layers chose, and why (after a memory fallback, if the board file allowed one).
    pub choice: Choice,
    /// What the memory check said about it.
    pub fit: Fit,
    /// The backend that draws: the choice if it is built in, else the nearest lighter one that is, else
    /// the nearest heavier one. Stepping down first is the memory-safe direction.
    pub uses: Backend,
}

/// The backend to use when `wanted` is not built in: the nearest lighter one that is, else the nearest
/// heavier one; `None` when nothing is built in.
#[must_use]
pub fn nearest(wanted: Backend, available: &[Backend]) -> Option<Backend> {
    if available.contains(&wanted) {
        return Some(wanted);
    }
    let mut lighter = wanted.lighter();
    while let Some(b) = lighter {
        if available.contains(&b) {
            return Some(b);
        }
        lighter = b.lighter();
    }
    // Nothing lighter is built in: the lightest heavier one.
    Backend::ALL.into_iter().find(|b| available.contains(b))
}

/// The whole decision: the layers (`build` is the build's choice, a cargo feature or a flag), the memory
/// check, and which backends are `available` in this program. `None` for a device without a screen or
/// when no backend is built in.
#[must_use]
pub fn plan(machine: &Machine, build: Option<Backend>, available: &[Backend]) -> Option<Plan> {
    let (choice, fit) = check(machine, choose(machine, build)?);
    let uses = nearest(choice.backend, available)?;
    Some(Plan { choice, fit, uses })
}

impl core::fmt::Display for Plan {
    /// One line for a log or a terminal: what draws, why, and anything the user should know.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} ({})", self.uses.name(), self.choice.reason)?;
        if self.uses != self.choice.backend {
            write!(
                f,
                "; {} is not built in, using {}",
                self.choice.backend.name(),
                self.uses.name()
            )?;
        }
        match self.fit {
            Fit::Fits => Ok(()),
            Fit::TooBig { need, budget } => write!(
                f,
                "; a frame needs {} KB but the device offers {} KB (allow a fallback in the render settings to step down)",
                need / 1024,
                budget / 1024
            ),
            Fit::FellBack { from, to } => write!(
                f,
                "; {} did not fit, fell back to {}",
                from.name(),
                to.name()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(w: u32, h: u32, hw: Option<Hardware>, render: Render) -> Machine {
        Machine {
            screen: Some((w, h)),
            hardware: hw,
            render,
        }
    }

    const S3: Hardware = Hardware {
        ram_kb: 512,
        psram_kb: 8192,
        fpu: true,
        simd: false,
        gpu: false,
    };
    // The heap the C6 benchmark had: 320 KB of its 512 KB.
    const C6: Hardware = Hardware {
        ram_kb: 320,
        psram_kb: 0,
        fpu: false,
        simd: false,
        gpu: false,
    };
    const PC: Hardware = Hardware {
        ram_kb: 16_000_000,
        psram_kb: 0,
        fpu: true,
        simd: true,
        gpu: true,
    };
    /// A computer or a single-board one with SIMD and no GPU that `wgpu` can use.
    const NO_GPU: Hardware = Hardware { gpu: false, ..PC };

    fn pick(w: u32, h: u32, hw: Hardware) -> Backend {
        choose(&device(w, h, Some(hw), Render::default()), None)
            .unwrap()
            .backend
    }

    #[test]
    fn names_round_trip_and_are_unique() {
        for b in Backend::ALL {
            assert_eq!(Backend::from_name(b.name()), Some(b));
            assert!(b.crate_name().starts_with("fusuma-render"));
        }
        assert_eq!(Backend::from_name("vello"), None);
        assert_eq!(Backend::Integer.lighter(), None);
        assert_eq!(Backend::VelloGpu.lighter(), Some(Backend::VelloCpu));
    }

    #[test]
    fn memory_follows_the_measurements() {
        assert_eq!(
            Backend::Integer.memory(172, 320),
            110_080,
            "measured: 110080 bytes"
        );
        // Vello measured 264764 bytes at 172x320; the rule says 264192: within 0.3 percent.
        let v = Backend::VelloCpu.memory(172, 320);
        assert!(v.abs_diff(264_764) * 100 / 264_764 <= 1, "{v}");
        assert_eq!(Backend::VelloGpu.memory(1920, 1080), 0);
        assert_eq!(Backend::Integer.memory(0, 500), 0);
        assert!(
            Backend::VelloCpu.memory(u32::MAX, u32::MAX) > 0,
            "no overflow at the extremes"
        );
    }

    #[test]
    fn the_rule_matches_what_was_measured_on_real_boards() {
        assert_eq!(
            pick(410, 502, S3),
            Backend::Integer,
            "the S3 watch: Vello has no SIMD there; measured, a tap takes 7 ms against Vello's 47"
        );
        assert_eq!(
            pick(1024, 600, S3),
            Backend::Integer,
            "the P4 likewise (RISC-V)"
        );
        for (w, h) in [(160, 160), (172, 320), (240, 320)] {
            assert_eq!(pick(w, h, C6), Backend::Integer, "a C6 at {w}x{h}");
        }
        assert_eq!(pick(1920, 1080, PC), Backend::VelloGpu);
        assert_eq!(
            pick(1920, 1080, NO_GPU),
            Backend::VelloCpu,
            "SIMD and room: Vello"
        );
    }

    #[test]
    fn vello_needs_simd_and_room_and_says_why_it_was_not_picked() {
        let why = |hw: Hardware| match automatic(&device(800, 480, Some(hw), Render::default())) {
            Some(Choice {
                backend,
                reason: Reason::Automatic(r),
            }) => (backend, r),
            other => panic!("{other:?}"),
        };
        let (b, r) = why(Hardware {
            simd: false,
            ..NO_GPU
        });
        assert_eq!(b, Backend::Integer);
        assert!(r.contains("no SIMD"), "{r}");
        // SIMD, but a frame of 800x480 (about 1.8 MB for Vello) in 1 MB.
        let (b, r) = why(Hardware {
            ram_kb: 1024,
            psram_kb: 0,
            ..NO_GPU
        });
        assert_eq!(b, Backend::Integer);
        assert!(r.contains("does not fit"), "{r}");
        assert_eq!(why(NO_GPU).0, Backend::VelloCpu);
        assert_eq!(
            why(Hardware { simd: false, ..PC }).0,
            Backend::VelloGpu,
            "a GPU needs no SIMD"
        );
    }

    #[test]
    fn no_screen_means_no_renderer_and_no_facts_means_the_lightest() {
        let mut p = device(10, 10, Some(S3), Render::default());
        p.screen = None;
        assert_eq!(choose(&p, Some(Backend::VelloCpu)), None);
        assert_eq!(automatic(&p), None);
        let blind = device(410, 502, None, Render::default());
        let c = choose(&blind, None).unwrap();
        assert_eq!(c.backend, Backend::Integer);
        assert!(matches!(c.reason, Reason::Automatic(_)));
    }

    #[test]
    fn the_layers_override_in_order_and_say_why() {
        let board_says = Render {
            backend: Some(Backend::VelloCpu),
            fallback: false,
            ..Render::default()
        };
        let p = device(410, 502, Some(S3), board_says);
        assert_eq!(
            choose(&p, None),
            Some(Choice {
                backend: Backend::VelloCpu,
                reason: Reason::Board
            })
        );
        assert_eq!(
            choose(&p, Some(Backend::Integer)),
            Some(Choice {
                backend: Backend::Integer,
                reason: Reason::Build
            })
        );
        let plain = device(410, 502, Some(S3), Render::default());
        assert!(matches!(
            choose(&plain, None).unwrap().reason,
            Reason::Automatic(_)
        ));
        assert_eq!(
            choose(&plain, Some(Backend::VelloGpu)).unwrap().backend,
            Backend::VelloGpu,
            "an explicit choice is not second-guessed"
        );
        for r in [Reason::Build, Reason::Board, Reason::Automatic("x")] {
            assert!(!std::string::ToString::to_string(&r).is_empty());
        }
    }

    #[test]
    fn the_automatic_choice_always_fits_its_own_budget() {
        for hw in [
            S3,
            C6,
            NO_GPU,
            Hardware {
                ram_kb: 4096,
                psram_kb: 0,
                ..NO_GPU
            },
            Hardware {
                ram_kb: 256,
                psram_kb: 0,
                ..C6
            },
            Hardware {
                ram_kb: 520,
                psram_kb: 2048,
                ..C6
            },
        ] {
            for (w, h) in [
                (1, 1),
                (64, 64),
                (172, 320),
                (240, 240),
                (410, 502),
                (800, 480),
                (1024, 600),
                (4000, 4000),
            ] {
                let p = device(w, h, Some(hw), Render::default());
                let c = automatic(&p).unwrap();
                let (got, fit) = check(&p, c);
                if c.backend == Backend::Integer {
                    continue; // nothing lighter exists: it is the floor, fit or not
                }
                assert_eq!(fit, Fit::Fits, "{hw:?} {w}x{h}: {c:?}");
                assert_eq!(got, c);
            }
        }
    }

    #[test]
    fn an_explicit_choice_that_does_not_fit_warns_and_can_fall_back_when_allowed() {
        let strict = device(
            240,
            320,
            Some(C6),
            Render {
                backend: Some(Backend::VelloCpu),
                fallback: false,
                ..Render::default()
            },
        );
        let (c, fit) = check(&strict, choose(&strict, None).unwrap());
        assert_eq!(c.backend, Backend::VelloCpu, "kept");
        assert!(matches!(fit, Fit::TooBig { need, budget } if need > budget));
        let relaxed = device(
            240,
            320,
            Some(C6),
            Render {
                backend: Some(Backend::VelloCpu),
                fallback: true,
                ..Render::default()
            },
        );
        let (c, fit) = check(&relaxed, choose(&relaxed, None).unwrap());
        assert_eq!(
            fit,
            Fit::FellBack {
                from: Backend::VelloCpu,
                to: Backend::Integer
            },
            "Vello's 360 KB does not fit 220 KB"
        );
        assert_eq!(
            (c.backend, c.reason),
            (Backend::Integer, Reason::Board),
            "the reason stays the original one"
        );
        let s3 = device(
            410,
            502,
            Some(S3),
            Render {
                backend: Some(Backend::VelloCpu),
                fallback: true,
                ..Render::default()
            },
        );
        assert_eq!(
            check(&s3, choose(&s3, None).unwrap()).1,
            Fit::Fits,
            "a board may pin Vello where it fits, SIMD or not"
        );
        // The floor: nothing lighter than the integer renderer, so it stays and warns.
        let tiny = device(
            4000,
            4000,
            Some(C6),
            Render {
                backend: Some(Backend::Integer),
                fallback: true,
                ..Render::default()
            },
        );
        assert!(matches!(
            check(&tiny, choose(&tiny, None).unwrap()).1,
            Fit::TooBig { .. }
        ));
    }

    #[test]
    fn the_budget_is_the_psram_or_the_internal_ram_left_after_the_free_minimum() {
        assert_eq!(
            budget(&C6, 100),
            220 * 1024,
            "no PSRAM: internal RAM minus the minimum"
        );
        assert_eq!(budget(&C6, 0), 320 * 1024);
        assert_eq!(
            budget(&C6, 9999),
            0,
            "a minimum above the RAM leaves nothing, no underflow"
        );
        assert_eq!(
            budget(&S3, 100),
            8192 * 1024,
            "a frame goes to PSRAM, so internal RAM is not touched"
        );
        assert_eq!(
            budget(
                &Hardware {
                    ram_kb: 20_000,
                    psram_kb: 8,
                    ..C6
                },
                100
            ),
            19_900 * 1024,
            "internal RAM can be the larger one"
        );
        assert_eq!(
            budget(
                &Hardware {
                    ram_kb: u32::MAX,
                    psram_kb: u32::MAX,
                    ..C6
                },
                0
            ),
            u64::from(u32::MAX) * 1024
        );
        assert!(
            fits(Backend::Integer, 100, 100, 20_000) && !fits(Backend::Integer, 100, 100, 19_999)
        );
    }

    #[test]
    fn a_gpu_choice_needs_no_memory_check() {
        let p = device(
            1920,
            1080,
            Some(Hardware { gpu: true, ..C6 }),
            Render::default(),
        );
        let c = choose(&p, None).unwrap();
        assert_eq!(c.backend, Backend::VelloGpu);
        assert_eq!(check(&p, c).1, Fit::Fits);
    }

    #[test]
    fn a_backend_that_is_not_built_in_is_replaced_by_the_nearest_lighter_one() {
        use Backend::{Integer, VelloCpu, VelloGpu};
        assert_eq!(
            nearest(VelloCpu, &[Integer, VelloGpu]),
            Some(Integer),
            "down first, it is the safe direction"
        );
        assert_eq!(nearest(VelloGpu, &[Integer, VelloCpu]), Some(VelloCpu));
        assert_eq!(
            nearest(VelloCpu, &[Integer, VelloCpu]),
            Some(VelloCpu),
            "built in: as asked"
        );
        assert_eq!(
            nearest(Integer, &[VelloGpu, VelloCpu]),
            Some(VelloCpu),
            "nothing lighter: the lightest heavier one"
        );
        assert_eq!(nearest(VelloCpu, &[]), None);
    }

    #[test]
    fn a_plan_combines_the_layers_the_memory_check_and_what_is_built_in() {
        let s3 = device(410, 502, Some(NO_GPU), Render::default());
        let all = Backend::ALL;
        let p = plan(&s3, None, &all).unwrap();
        assert_eq!(
            (p.choice.backend, p.uses, p.fit),
            (Backend::VelloCpu, Backend::VelloCpu, Fit::Fits)
        );
        assert_eq!(
            std::string::ToString::to_string(&p),
            "vello-cpu (automatic: Vello has SIMD here and its frame fits)"
        );
        let integer_only = plan(&s3, None, &[Backend::Integer]).unwrap();
        assert_eq!(
            (integer_only.choice.backend, integer_only.uses),
            (Backend::VelloCpu, Backend::Integer)
        );
        let line = std::string::ToString::to_string(&integer_only);
        assert!(
            line.contains("vello-cpu is not built in, using integer"),
            "{line}"
        );
        assert!(
            plan(&s3, None, &[]).is_none(),
            "nothing built in, nothing to draw with"
        );
        let mut headless = s3;
        headless.screen = None;
        assert!(plan(&headless, None, &all).is_none());
    }

    #[test]
    fn the_plan_line_names_a_memory_problem() {
        let tight = device(
            240,
            320,
            Some(C6),
            Render {
                backend: Some(Backend::VelloCpu),
                fallback: false,
                ..Render::default()
            },
        );
        let line = std::string::ToString::to_string(&plan(&tight, None, &Backend::ALL).unwrap());
        assert!(
            line.contains("needs 360 KB")
                && line.contains("offers 220 KB")
                && line.contains("allow a fallback"),
            "{line}"
        );
        let relaxed = device(
            240,
            320,
            Some(C6),
            Render {
                backend: Some(Backend::VelloCpu),
                fallback: true,
                ..Render::default()
            },
        );
        let line = std::string::ToString::to_string(&plan(&relaxed, None, &Backend::ALL).unwrap());
        assert!(
            line.contains("vello-cpu did not fit, fell back to integer"),
            "{line}"
        );
    }

    #[test]
    fn a_larger_free_minimum_steps_the_choice_down() {
        // A single-board computer with SIMD and 2 MB to spare: Vello's 800x480 frame needs about 1.8 MB.
        let board = Hardware {
            ram_kb: 2048,
            psram_kb: 0,
            ..NO_GPU
        };
        let with = |min_free_kb| {
            let m = device(
                800,
                480,
                Some(board),
                Render {
                    min_free_kb,
                    ..Render::default()
                },
            );
            let (c, fit) = check(&m, choose(&m, None).unwrap());
            (c.backend, fit)
        };
        assert_eq!(with(0).0, Backend::VelloCpu, "everything fits");
        assert_eq!(with(100).0, Backend::VelloCpu, "1.8 MB of 1.9 MB");
        assert_eq!(
            with(300).0,
            Backend::Integer,
            "a bigger reserve leaves room for the lighter one only"
        );
        assert!(matches!(
            with(5000),
            (Backend::Integer, Fit::Fits | Fit::TooBig { .. })
        ));
    }

    #[test]
    fn the_defaults_are_automatic_with_a_fallback_and_the_documented_minimum() {
        let r = Render::default();
        assert_eq!(
            (r.backend, r.fallback, r.min_free_kb),
            (None, true, DEFAULT_MIN_FREE_KB)
        );
        assert_eq!(RenderLayer::EMPTY.resolve(), r);
        assert_eq!(Render::layered(&[]), r);
    }

    #[test]
    fn layers_combine_weakest_first_and_the_last_to_speak_wins() {
        let project = RenderLayer {
            backend: Some(BackendSetting::Fixed(Backend::Integer)),
            fallback: Some(false),
            min_free_kb: Some(150),
        };
        let user = RenderLayer {
            fallback: Some(true),
            ..RenderLayer::EMPTY
        };
        let build = RenderLayer {
            backend: Some(BackendSetting::Fixed(Backend::VelloCpu)),
            ..RenderLayer::EMPTY
        };
        let r = Render::layered(&[project, user, build]);
        assert_eq!(r.backend, Some(Backend::VelloCpu));
        assert!(r.fallback, "the user layer beat the project");
        assert_eq!(r.min_free_kb, 150, "nobody stronger said anything");
        assert_eq!(Render::layered(&[project]).backend, Some(Backend::Integer));
        // Order matters: the same layers the other way round.
        assert_eq!(
            Render::layered(&[build, project]).backend,
            Some(Backend::Integer)
        );
    }

    #[test]
    fn auto_in_a_stronger_layer_undoes_a_weaker_layers_pick() {
        let pinned = RenderLayer {
            backend: Some(BackendSetting::Fixed(Backend::Integer)),
            ..RenderLayer::EMPTY
        };
        let auto = RenderLayer {
            backend: Some(BackendSetting::Auto),
            ..RenderLayer::EMPTY
        };
        assert_eq!(Render::layered(&[pinned, auto]).backend, None);
        assert_eq!(
            Render::layered(&[auto, pinned]).backend,
            Some(Backend::Integer)
        );
    }
}
