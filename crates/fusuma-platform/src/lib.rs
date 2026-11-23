#![cfg_attr(not(test), no_std)]

//! Which Vello backend a machine draws with, so an application never has to name one: the GPU backend where
//! there is a GPU, the CPU backend everywhere else, and a check that the CPU backend's frame fits in the
//! machine's memory. Pure logic with no UI and no dependencies, so a platform crate (a board, a desktop, a
//! phone), a build script and a flashing wizard all ask the same question and show the same answer.
//!
//! Layers, strongest first: the **build** (a cargo feature or a command-line flag), the **settings** (a board
//! file's `[render] backend`, or a stored setting), then an **automatic** rule over the hardware facts.
//! Whatever wins, the [`Choice`] says why, so an override is visible.

/// A renderer backend. Both are Vello, so they draw the same picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Backend {
    /// `fusuma-render-vello-cpu`: Vello on the CPU. Runs everywhere that has the memory for a frame.
    VelloCpu,
    /// `fusuma-render-vello`: Vello on a GPU through `wgpu`.
    VelloGpu,
}

impl Backend {
    /// Every backend, lightest first.
    pub const ALL: [Self; 2] = [Self::VelloCpu, Self::VelloGpu];

    /// The name used in device files and on command lines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
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
            Self::VelloCpu => "VelloCpu",
            Self::VelloGpu => "VelloGpu",
        }
    }

    /// The Fusuma crate that implements it.
    #[must_use]
    pub const fn crate_name(self) -> &'static str {
        match self {
            Self::VelloCpu => "fusuma-render-vello-cpu",
            Self::VelloGpu => "fusuma-render-vello",
        }
    }

    /// The next lighter backend, for a fallback; `None` for the lightest.
    #[must_use]
    pub const fn lighter(self) -> Option<Self> {
        match self {
            Self::VelloCpu => None,
            Self::VelloGpu => Some(Self::VelloCpu),
        }
    }

    /// RAM the backend needs for a surface of `width` x `height`, in bytes. Measured on an ESP32-S3:
    /// about 4.8 bytes per pixel for `vello_cpu` with its working buffers. The GPU backend keeps its pixels
    /// in video memory, so it needs none here.
    #[must_use]
    pub const fn memory(self, width: u32, height: u32) -> u64 {
        // u32 x u32 fits in a u64, but the bytes per pixel may not: saturate.
        let pixels = width as u64 * height as u64;
        match self {
            Self::VelloCpu => pixels.saturating_mul(48) / 10,
            Self::VelloGpu => 0,
        }
    }
}

/// What a machine has that decides what it can draw with. A board file lists them, or a platform reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hardware {
    /// Internal RAM in kilobytes.
    pub ram_kb: u32,
    /// External PSRAM in kilobytes, 0 for none.
    pub psram_kb: u32,
    /// Whether the chip has a floating-point unit.
    pub fpu: bool,
    /// Whether there is a GPU that `wgpu` can use.
    pub gpu: bool,
}

/// What the settings may say about rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Render {
    /// A backend to use whatever the rule would pick.
    pub backend: Option<Backend>,
}

/// What the choice is about: the screen, what the machine has, and what the settings say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    /// The screen size in pixels; `None` for a device without one (nothing to draw on).
    pub screen: Option<(u32, u32)>,
    /// What the machine has, if known.
    pub hardware: Option<Hardware>,
    /// What the settings say about rendering.
    pub render: Render,
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
    /// The settings asked for it.
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

/// The memory a frame may use on a machine: its PSRAM and its share of internal RAM, in bytes.
#[must_use]
pub fn budget(hw: &Hardware) -> u64 {
    u64::from(hw.psram_kb) * 1024 + u64::from(hw.ram_kb) * 1024 * RAM_SHARE_PERCENT / 100
}

/// Whether `backend` fits a surface of `width` x `height` in `budget_bytes`.
#[must_use]
pub const fn fits(backend: Backend, width: u32, height: u32, budget_bytes: u64) -> bool {
    backend.memory(width, height) <= budget_bytes
}

/// The automatic rule: the GPU backend where there is a GPU, the CPU backend otherwise. `None` for a
/// machine without a screen.
#[must_use]
pub fn automatic(machine: &Machine) -> Option<Choice> {
    machine.screen?;
    let (backend, why) = match machine.hardware {
        Some(hw) if hw.gpu => (Backend::VelloGpu, "the machine has a GPU"),
        Some(_) => (Backend::VelloCpu, "no GPU, so Vello on the CPU"),
        None => (Backend::VelloCpu, "no hardware facts, so Vello on the CPU"),
    };
    Some(Choice {
        backend,
        reason: Reason::Automatic(why),
    })
}

/// The layers: the build's choice beats the settings', which beats the automatic rule. `None` for a machine
/// without a screen.
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
    /// It does not fit. Drawing with it would run the machine out of memory, so a platform should refuse
    /// to start and say so.
    TooBig {
        /// Bytes the frame needs.
        need: u64,
        /// Bytes the machine offers.
        budget: u64,
    },
}

/// Checks a choice against the machine's memory.
#[must_use]
pub fn check(machine: &Machine, choice: Choice) -> Fit {
    let (Some((w, h)), Some(hw)) = (machine.screen, machine.hardware) else {
        return Fit::Fits;
    };
    let budget = budget(&hw);
    if choice.backend == Backend::VelloGpu || fits(choice.backend, w, h, budget) {
        Fit::Fits
    } else {
        Fit::TooBig {
            need: choice.backend.memory(w, h),
            budget,
        }
    }
}

/// How a machine will really draw: what the layers chose, whether it fits and what is built in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The backend the layers chose, and why.
    pub choice: Choice,
    /// What the memory check said about it.
    pub fit: Fit,
    /// The backend that draws: the choice if it is built in, else the lighter one if that is.
    pub uses: Backend,
}

/// The backend to use when `wanted` is not built in: the nearest lighter one that is, else the lightest
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
    Backend::ALL.into_iter().find(|b| available.contains(b))
}

/// The whole decision: the layers (`build` is the build's choice, a cargo feature or a flag), what is
/// `available` in this program, and the memory check of the backend that will draw. `None` for a machine
/// without a screen or when no backend is built in.
#[must_use]
pub fn plan(machine: &Machine, build: Option<Backend>, available: &[Backend]) -> Option<Plan> {
    let choice = choose(machine, build)?;
    let uses = nearest(choice.backend, available)?;
    let fit = check(
        machine,
        Choice {
            backend: uses,
            reason: choice.reason,
        },
    );
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
                "; a frame needs {} KB but the machine offers {} KB",
                need / 1024,
                budget / 1024
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
    const P4: Hardware = Hardware {
        ram_kb: 477,
        psram_kb: 31_293,
        fpu: true,
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
            assert!(b.crate_name().starts_with("fusuma-render-vello"));
        }
        assert_eq!(Backend::from_name("Skia"), None);
        assert_eq!(Backend::from_name("vello"), None);
        assert_eq!(Backend::from_name("skia"), None, "removed");
        assert_eq!(Backend::VelloCpu.lighter(), None);
        assert_eq!(Backend::VelloGpu.lighter(), Some(Backend::VelloCpu));
    }

    #[test]
    fn memory_follows_the_measurements() {
        assert_eq!(
            Backend::VelloCpu.memory(410, 502),
            987_936,
            "about 4.8 bytes per pixel"
        );
        assert_eq!(Backend::VelloGpu.memory(1920, 1080), 0);
        assert_eq!(Backend::VelloCpu.memory(0, 500), 0);
        assert!(
            Backend::VelloCpu.memory(u32::MAX, u32::MAX) > 0,
            "no overflow"
        );
    }

    #[test]
    fn the_rule_is_the_gpu_where_there_is_one_and_the_cpu_everywhere_else() {
        assert_eq!(pick(1920, 1080, PC), Backend::VelloGpu);
        assert_eq!(pick(1024, 600, P4), Backend::VelloCpu);
        assert_eq!(pick(410, 502, S3), Backend::VelloCpu);
        assert_eq!(
            pick(240, 320, C6),
            Backend::VelloCpu,
            "the rule does not look at memory: the check does"
        );
    }

    #[test]
    fn no_screen_means_no_renderer_and_no_facts_means_the_cpu() {
        let mut m = device(10, 10, Some(S3), Render::default());
        m.screen = None;
        assert_eq!(choose(&m, None), None);
        assert_eq!(automatic(&m), None);
        assert_eq!(plan(&m, None, &Backend::ALL), None);
        let c = choose(&device(10, 10, None, Render::default()), None).unwrap();
        assert_eq!(c.backend, Backend::VelloCpu);
        assert!(matches!(c.reason, Reason::Automatic(_)));
    }

    #[test]
    fn the_layers_build_then_settings_then_the_rule() {
        let set = Render {
            backend: Some(Backend::VelloCpu),
        };
        let m = device(1920, 1080, Some(PC), set);
        let c = choose(&m, None).unwrap();
        assert_eq!((c.backend, c.reason), (Backend::VelloCpu, Reason::Board));
        let c = choose(&m, Some(Backend::VelloGpu)).unwrap();
        assert_eq!((c.backend, c.reason), (Backend::VelloGpu, Reason::Build));
        let c = choose(&device(1920, 1080, Some(PC), Render::default()), None).unwrap();
        assert!(matches!(c.reason, Reason::Automatic(_)));
    }

    #[test]
    fn the_memory_check_accepts_what_fits_and_refuses_what_does_not() {
        let fits = |w, h, hw| {
            let m = device(w, h, Some(hw), Render::default());
            check(&m, choose(&m, None).unwrap())
        };
        assert_eq!(fits(1024, 600, P4), Fit::Fits);
        assert_eq!(fits(410, 502, S3), Fit::Fits);
        assert_eq!(fits(1920, 1080, PC), Fit::Fits, "a GPU frame is not in RAM");
        assert!(
            matches!(fits(240, 320, C6), Fit::TooBig { .. }),
            "measured: the C6 ran out of memory"
        );
        let m = device(240, 320, None, Render::default());
        assert_eq!(
            check(&m, choose(&m, None).unwrap()),
            Fit::Fits,
            "nothing known, nothing refused"
        );
    }

    #[test]
    fn a_backend_that_is_not_built_in_is_replaced_by_the_lighter_one() {
        use Backend::{VelloCpu, VelloGpu};
        assert_eq!(nearest(VelloGpu, &[VelloCpu]), Some(VelloCpu));
        assert_eq!(nearest(VelloGpu, &[VelloGpu, VelloCpu]), Some(VelloGpu));
        assert_eq!(
            nearest(VelloCpu, &[VelloGpu]),
            Some(VelloGpu),
            "nothing lighter: the heavier one"
        );
        assert_eq!(nearest(VelloCpu, &[]), None);
    }

    #[test]
    fn a_plan_combines_the_layers_what_is_built_in_and_the_memory_check() {
        let p = plan(
            &device(1024, 600, Some(P4), Render::default()),
            None,
            &Backend::ALL,
        )
        .unwrap();
        assert_eq!(
            (p.choice.backend, p.uses, p.fit),
            (Backend::VelloCpu, Backend::VelloCpu, Fit::Fits)
        );
        assert_eq!(
            p.to_string(),
            "vello-cpu (automatic: no GPU, so Vello on the CPU)"
        );
        let pc = device(1920, 1080, Some(PC), Render::default());
        let p = plan(&pc, None, &[Backend::VelloCpu]).unwrap();
        assert_eq!(
            (p.choice.backend, p.uses),
            (Backend::VelloGpu, Backend::VelloCpu)
        );
        assert!(
            p.to_string()
                .contains("vello-gpu is not built in, using vello-cpu")
        );
        assert!(
            plan(&pc, None, &[]).is_none(),
            "nothing built in, nothing to draw with"
        );
    }

    #[test]
    fn the_plan_line_names_a_memory_problem() {
        let tight = device(240, 320, Some(C6), Render::default());
        let line = plan(&tight, None, &Backend::ALL).unwrap().to_string();
        assert!(
            line.contains("needs 360 KB") && line.contains("offers 204 KB"),
            "{line}"
        );
    }
}
