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
    /// `fusuma-render-embedded`: integer drawing on `embedded-graphics`; the lightest and fastest, no
    /// anti-aliasing and no alpha.
    Integer,
    /// `fusuma-render-skia`: `tiny-skia`; anti-aliased, 4 bytes per pixel.
    Skia,
    /// `fusuma-render-vello-cpu`: the richest CPU backend, and the heaviest.
    VelloCpu,
    /// `fusuma-render-vello`: Vello on a GPU through `wgpu`.
    VelloGpu,
}

impl Backend {
    /// Every backend, lightest first.
    pub const ALL: [Self; 4] = [Self::Integer, Self::Skia, Self::VelloCpu, Self::VelloGpu];

    /// The name used in device files and on command lines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Skia => "skia",
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
            Self::Skia => "Skia",
            Self::VelloCpu => "VelloCpu",
            Self::VelloGpu => "VelloGpu",
        }
    }

    /// The Fusuma crate that implements it.
    #[must_use]
    pub const fn crate_name(self) -> &'static str {
        match self {
            Self::Integer => "fusuma-render-embedded",
            Self::Skia => "fusuma-render-skia",
            Self::VelloCpu => "fusuma-render-vello-cpu",
            Self::VelloGpu => "fusuma-render-vello",
        }
    }

    /// The next lighter backend, for a fallback; `None` for the lightest.
    #[must_use]
    pub const fn lighter(self) -> Option<Self> {
        match self {
            Self::Integer => None,
            Self::Skia => Some(Self::Integer),
            Self::VelloCpu => Some(Self::Skia),
            Self::VelloGpu => Some(Self::VelloCpu),
        }
    }

    /// RAM the backend needs for a surface of `width` x `height`, in bytes. Measured on an ESP32-C6
    /// and an ESP32-S3 (`tools/bench-c6/RESULT.md`, `tools/bench-s3/RESULT.md`): the integer
    /// renderer's frame is 2 bytes per pixel, `tiny-skia`'s pixmap 4, `vello_cpu` about 4.8 with its
    /// working buffers. The GPU backend keeps its pixels in video memory, so it needs none here.
    #[must_use]
    pub const fn memory(self, width: u32, height: u32) -> u64 {
        // u32 x u32 fits in a u64, but the bytes per pixel may not: saturate.
        let pixels = width as u64 * height as u64;
        match self {
            Self::Integer => pixels.saturating_mul(2),
            Self::Skia => pixels.saturating_mul(4),
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
    /// Whether there is a GPU that `wgpu` can use.
    pub gpu: bool,
}

/// What a board file may say about rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Render {
    /// A backend to use whatever the rule would pick.
    pub backend: Option<Backend>,
    /// Whether to step down to a lighter backend if the chosen one does not fit in memory.
    pub fallback: bool,
}

/// Share of the internal RAM that a frame may take, in percent. The rest is for the code's data, the
/// stack and the radio. This is an assumption, not a measurement: nothing here knows what the radio
/// needs. Raise it when that is measured.
pub const RAM_SHARE_PERCENT: u64 = 40;

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

/// The memory a frame may use on a device: its PSRAM and its share of internal RAM, in bytes.
#[must_use]
pub fn budget(hw: &Hardware) -> u64 {
    u64::from(hw.psram_kb) * 1024 + u64::from(hw.ram_kb) * 1024 * RAM_SHARE_PERCENT / 100
}

/// Whether `backend` fits a surface of `width` x `height` in `budget_bytes`.
#[must_use]
pub const fn fits(backend: Backend, width: u32, height: u32, budget_bytes: u64) -> bool {
    backend.memory(width, height) <= budget_bytes
}

/// The automatic rule: the richest backend that fits, with a reason. `None` for a device without a
/// screen.
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
    // PSRAM holds a Vello frame with room to spare: the richest CPU backend.
    if u64::from(hw.psram_kb) * 1024 >= 2 * Backend::VelloCpu.memory(w, h) {
        return auto(Backend::VelloCpu, "PSRAM holds a Vello frame twice over");
    }
    if fits(Backend::Skia, w, h, budget(&hw)) {
        return auto(
            Backend::Skia,
            "an anti-aliased frame fits in the memory budget",
        );
    }
    auto(
        Backend::Integer,
        "only the 2 bytes per pixel of the integer renderer fit in the memory budget",
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
    let budget = budget(&hw);
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
        gpu: false,
    };
    const C6: Hardware = Hardware {
        ram_kb: 512,
        psram_kb: 0,
        fpu: false,
        gpu: false,
    };
    const PC: Hardware = Hardware {
        ram_kb: 16_000_000,
        psram_kb: 0,
        fpu: true,
        gpu: true,
    };

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
        assert_eq!(Backend::from_name("Skia"), None);
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
        assert_eq!(
            Backend::Skia.memory(240, 320),
            307_200,
            "measured: 307200 bytes used"
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
            Backend::VelloCpu,
            "the S3 watch: PSRAM holds a Vello frame"
        );
        assert_eq!(
            pick(160, 160, C6),
            Backend::Skia,
            "a small C6 display: 102 KB fits in 40% of 512 KB"
        );
        assert_eq!(
            pick(172, 320, C6),
            Backend::Integer,
            "215 KB does not fit in 204 KB, the integer 108 KB does"
        );
        assert_eq!(pick(240, 320, C6), Backend::Integer);
        assert_eq!(pick(1920, 1080, PC), Backend::VelloGpu);
        // A PSRAM too small for a Vello frame falls to the next backend.
        // 410x502: integer 412 KB, skia 823 KB, vello 988 KB. Budget = PSRAM + 40% of 512 KB (205 KB).
        let one_mb = Hardware {
            psram_kb: 1024,
            ..S3
        };
        assert_eq!(
            pick(410, 502, one_mb),
            Backend::Skia,
            "1 MB holds skia's 823 KB, but not twice Vello's 988 KB"
        );
        let half_mb = Hardware {
            psram_kb: 512,
            ..S3
        };
        assert_eq!(
            pick(410, 502, half_mb),
            Backend::Integer,
            "717 KB holds the integer frame, not skia's 823 KB"
        );
        let none = Hardware { psram_kb: 0, ..S3 };
        assert_eq!(
            pick(410, 502, none),
            Backend::Integer,
            "205 KB holds neither: the integer renderer is the floor"
        );
    }

    #[test]
    fn no_screen_means_no_renderer_and_no_facts_means_the_lightest() {
        let mut p = device(10, 10, Some(S3), Render::default());
        p.screen = None;
        assert_eq!(choose(&p, Some(Backend::Skia)), None);
        assert_eq!(automatic(&p), None);
        let blind = device(410, 502, None, Render::default());
        let c = choose(&blind, None).unwrap();
        assert_eq!(c.backend, Backend::Integer);
        assert!(matches!(c.reason, Reason::Automatic(_)));
    }

    #[test]
    fn the_layers_override_in_order_and_say_why() {
        let board_says = Render {
            backend: Some(Backend::Skia),
            fallback: false,
        };
        let p = device(410, 502, Some(S3), board_says);
        assert_eq!(
            choose(&p, None),
            Some(Choice {
                backend: Backend::Skia,
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
            },
        );
        let (c, fit) = check(&relaxed, choose(&relaxed, None).unwrap());
        assert_eq!(
            fit,
            Fit::FellBack {
                from: Backend::VelloCpu,
                to: Backend::Integer
            },
            "300 KB skia does not fit 204 KB either"
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
                backend: Some(Backend::Skia),
                fallback: true,
            },
        );
        assert_eq!(check(&s3, choose(&s3, None).unwrap()).1, Fit::Fits);
        // The floor: nothing lighter than the integer renderer, so it stays and warns.
        let tiny = device(
            4000,
            4000,
            Some(C6),
            Render {
                backend: Some(Backend::Integer),
                fallback: true,
            },
        );
        assert!(matches!(
            check(&tiny, choose(&tiny, None).unwrap()).1,
            Fit::TooBig { .. }
        ));
    }

    #[test]
    fn the_budget_adds_psram_and_a_share_of_internal_ram() {
        assert_eq!(budget(&C6), 512 * 1024 * 40 / 100);
        assert_eq!(budget(&S3), 8192 * 1024 + 512 * 1024 * 40 / 100);
        assert_eq!(
            budget(&Hardware {
                ram_kb: u32::MAX,
                psram_kb: u32::MAX,
                ..C6
            }),
            u64::from(u32::MAX) * 1024 + u64::from(u32::MAX) * 1024 * 40 / 100
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
        use Backend::{Integer, Skia, VelloCpu, VelloGpu};
        assert_eq!(
            nearest(Skia, &[Integer, VelloCpu]),
            Some(Integer),
            "down first, it is the safe direction"
        );
        assert_eq!(nearest(VelloGpu, &[Integer, VelloCpu]), Some(VelloCpu));
        assert_eq!(
            nearest(VelloCpu, &[Integer, Skia, VelloCpu]),
            Some(VelloCpu),
            "built in: as asked"
        );
        assert_eq!(
            nearest(Integer, &[Skia]),
            Some(Skia),
            "nothing lighter: the lightest heavier one"
        );
        assert_eq!(nearest(Integer, &[VelloCpu, Skia]), Some(Skia));
        assert_eq!(nearest(Skia, &[]), None);
    }

    #[test]
    fn a_plan_combines_the_layers_the_memory_check_and_what_is_built_in() {
        let s3 = device(410, 502, Some(S3), Render::default());
        let all = Backend::ALL;
        let p = plan(&s3, None, &all).unwrap();
        assert_eq!(
            (p.choice.backend, p.uses, p.fit),
            (Backend::VelloCpu, Backend::VelloCpu, Fit::Fits)
        );
        assert_eq!(
            std::string::ToString::to_string(&p),
            "vello-cpu (automatic: PSRAM holds a Vello frame twice over)"
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
            },
        );
        let line = std::string::ToString::to_string(&plan(&tight, None, &Backend::ALL).unwrap());
        assert!(
            line.contains("needs 360 KB")
                && line.contains("offers 204 KB")
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
            },
        );
        let line = std::string::ToString::to_string(&plan(&relaxed, None, &Backend::ALL).unwrap());
        assert!(
            line.contains("vello-cpu did not fit, fell back to integer"),
            "{line}"
        );
    }
}
