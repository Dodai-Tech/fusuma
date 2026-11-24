//! What Fusuma is running on: the architecture and instruction-set features, which the compiler knows, and
//! presets for the ESP family, whose memory is an estimate. Memory is not knowable from a chip name in general, so
//! for any other chip (a Cortex-M, another RISC-V part) a runtime probes its heap or a board file states it.

use fusuma::platform::Hardware;

/// A processor architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Arch {
    /// Xtensa: the ESP32, S2 and S3.
    Xtensa,
    /// 32-bit RISC-V: the ESP32 C and H series and P4, and most RISC-V microcontrollers.
    RiscV32,
    /// 64-bit RISC-V: Linux boards.
    RiscV64,
    /// 32-bit ARM: Cortex-M microcontrollers and older Cortex-A.
    Arm,
    /// 64-bit ARM: Apple Silicon, Raspberry Pi 3 and later, phones.
    Arm64,
    /// 32-bit x86.
    X86,
    /// 64-bit x86: most desktops and laptops.
    X86_64,
    /// WebAssembly.
    Wasm32,
    /// Anything else.
    Other,
}

impl Arch {
    /// The architecture this program was compiled for.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_arch = "xtensa") {
            Self::Xtensa
        } else if cfg!(target_arch = "riscv32") {
            Self::RiscV32
        } else if cfg!(target_arch = "riscv64") {
            Self::RiscV64
        } else if cfg!(target_arch = "arm") {
            Self::Arm
        } else if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else if cfg!(target_arch = "x86") {
            Self::X86
        } else if cfg!(target_arch = "x86_64") {
            Self::X86_64
        } else if cfg!(target_arch = "wasm32") {
            Self::Wasm32
        } else {
            Self::Other
        }
    }

    /// The name, as `rustc` spells the architecture.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Xtensa => "xtensa",
            Self::RiscV32 => "riscv32",
            Self::RiscV64 => "riscv64",
            Self::Arm => "arm",
            Self::Arm64 => "aarch64",
            Self::X86 => "x86",
            Self::X86_64 => "x86_64",
            Self::Wasm32 => "wasm32",
            Self::Other => "other",
        }
    }
}

/// Whether the build can use a hardware floating-point unit: what the compiler was told it may emit. A build
/// for a chip that has one but was told not to (a soft-float target) reports false, which is right: it will not
/// be used.
#[must_use]
pub const fn has_fpu() -> bool {
    cfg!(any(
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "aarch64",
        target_arch = "wasm32",
        target_feature = "f",
        target_feature = "vfp2",
        target_feature = "vfp3",
        target_feature = "vfp4",
    ))
}

/// Whether the build can use SIMD instructions: NEON, SSE2, WebAssembly SIMD, or a vector extension.
#[must_use]
pub const fn has_simd() -> bool {
    cfg!(any(
        target_feature = "neon",
        target_feature = "sse2",
        target_feature = "simd128",
        target_feature = "v",
        target_arch = "aarch64",
    ))
}

/// The facts the renderer choice needs for this build: the floating-point unit from the compiler, and the memory
/// from the caller (a runtime's heap probe, or a board file), since no chip name says it.
#[must_use]
pub const fn hardware(ram_kb: u32, psram_kb: u32) -> Hardware {
    Hardware {
        ram_kb,
        psram_kb,
        fpu: has_fpu(),
        gpu: false,
    }
}

/// An ESP32 family member. A preset for the chips Fusuma has a runtime for; another family (a Cortex-M or another
/// RISC-V part) does not need an entry here, since its runtime probes or reads a board file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Chip {
    /// ESP32 (Xtensa, two cores), with optional PSRAM.
    Esp32,
    /// ESP32-S2 (Xtensa, one core), with optional PSRAM.
    Esp32S2,
    /// ESP32-S3 (Xtensa, two cores), with optional PSRAM.
    Esp32S3,
    /// ESP32-C2 (RISC-V), very little RAM.
    Esp32C2,
    /// ESP32-C3 (RISC-V), no PSRAM.
    Esp32C3,
    /// ESP32-C6 (RISC-V), no PSRAM.
    Esp32C6,
    /// ESP32-H2 (RISC-V), no PSRAM and no display worth the name; listed so a headless board can say so.
    Esp32H2,
    /// ESP32-P4 (RISC-V, two cores, floating point), with PSRAM.
    Esp32P4,
}

impl Chip {
    /// Every chip.
    pub const ALL: [Self; 8] = [
        Self::Esp32,
        Self::Esp32S2,
        Self::Esp32S3,
        Self::Esp32C2,
        Self::Esp32C3,
        Self::Esp32C6,
        Self::Esp32H2,
        Self::Esp32P4,
    ];

    /// The name used in board files and on command lines.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Esp32 => "esp32",
            Self::Esp32S2 => "esp32s2",
            Self::Esp32S3 => "esp32s3",
            Self::Esp32C2 => "esp32c2",
            Self::Esp32C3 => "esp32c3",
            Self::Esp32C6 => "esp32c6",
            Self::Esp32H2 => "esp32h2",
            Self::Esp32P4 => "esp32p4",
        }
    }

    /// The chip called `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// Whether the chip has a floating-point unit (the original ESP32, the S3 and the P4 do; the S2, and the C and H
    /// series, do not). From the datasheets; the build's own answer is [`has_fpu`].
    #[must_use]
    pub const fn fpu(self) -> bool {
        matches!(self, Self::Esp32 | Self::Esp32S3 | Self::Esp32P4)
    }

    /// Whether the chip can have external PSRAM at all (a board still has to fit it).
    #[must_use]
    pub const fn psram_capable(self) -> bool {
        matches!(
            self,
            Self::Esp32 | Self::Esp32S2 | Self::Esp32S3 | Self::Esp32P4
        )
    }

    /// The internal RAM a program can use, in kilobytes: **measured** for the C6 (320 KB heap in the benchmark) and
    /// the P4 (about 475 KB at boot), **an estimate** for every other chip (datasheet SRAM less a guess for the
    /// system). A runtime that can ask its heap should, and use this only when it cannot.
    #[must_use]
    pub const fn ram_kb(self) -> u32 {
        match self {
            Self::Esp32 => 320,
            Self::Esp32S2 => 240,
            Self::Esp32S3 => 400,
            Self::Esp32C2 => 150,
            Self::Esp32C3 => 280,
            Self::Esp32C6 => 320,
            Self::Esp32H2 => 280,
            Self::Esp32P4 => 475,
        }
    }
}

/// What an ESP chip has, given how much PSRAM the board fitted (0 for none; ignored on a chip that cannot have any):
/// the input to the renderer choice for a runtime that cannot probe its heap.
#[must_use]
pub const fn esp_hardware(chip: Chip, psram_kb: u32) -> Hardware {
    Hardware {
        ram_kb: chip.ram_kb(),
        psram_kb: if chip.psram_capable() { psram_kb } else { 0 },
        fpu: chip.fpu(),
        gpu: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma::platform::{Backend, Machine, Render, automatic, check, choose};

    #[test]
    fn the_build_knows_its_architecture_and_float_unit() {
        let a = Arch::current();
        assert_ne!(a, Arch::Other, "this host is a known architecture");
        assert!(!a.name().is_empty());
        if cfg!(target_arch = "x86_64") {
            assert_eq!(a, Arch::X86_64);
            assert!(has_fpu() && has_simd());
        }
        if cfg!(target_arch = "aarch64") {
            assert_eq!(a, Arch::Arm64);
            assert!(has_fpu() && has_simd());
        }
        let h = hardware(300, 0);
        assert_eq!(
            (h.ram_kb, h.psram_kb, h.fpu, h.gpu),
            (300, 0, has_fpu(), false)
        );
    }

    #[test]
    fn names_round_trip_and_are_unique() {
        for c in Chip::ALL {
            assert_eq!(Chip::from_name(c.name()), Some(c));
        }
        assert_eq!(Chip::from_name("esp32s4"), None);
        let mut names: alloc::vec::Vec<_> = Chip::ALL.iter().map(|c| c.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Chip::ALL.len());
    }

    #[test]
    fn psram_only_counts_on_chips_that_can_have_it() {
        assert_eq!(esp_hardware(Chip::Esp32S3, 8192).psram_kb, 8192);
        assert_eq!(
            esp_hardware(Chip::Esp32C6, 8192).psram_kb,
            0,
            "a C6 has no PSRAM whatever the board says"
        );
        assert!(esp_hardware(Chip::Esp32P4, 32768).fpu && !esp_hardware(Chip::Esp32C3, 0).fpu);
        assert!(Chip::ALL.iter().all(|c| c.ram_kb() >= 100));
    }

    fn pick(chip: Chip, psram: u32, w: u32, h: u32) -> (Backend, bool) {
        let m = Machine {
            screen: Some((w, h)),
            hardware: Some(esp_hardware(chip, psram)),
            render: Render::default(),
        };
        let c = automatic(&m).unwrap();
        let (c, fit) = check(&m, choose(&m, None).unwrap_or(c));
        (c.backend, matches!(fit, fusuma::platform::Fit::Fits))
    }

    /// What each chip gets by itself for a screen of its class: the rule's answer, written down so a change to
    /// the rule or to the estimates shows up here.
    #[test]
    fn each_chip_gets_the_renderer_that_fits_its_class_of_screen() {
        assert_eq!(
            pick(Chip::Esp32P4, 32768, 1024, 600),
            (Backend::VelloCpu, true)
        );
        assert_eq!(
            pick(Chip::Esp32S3, 8192, 410, 502),
            (Backend::VelloCpu, true),
            "the S3 watch"
        );
        assert_eq!(
            pick(Chip::Esp32C6, 0, 172, 320),
            (Backend::Integer, true),
            "the C6's own LCD: tiny-skia's pixmap and clip mask (269 KB) leave less than 100 KB free"
        );
        assert_eq!(
            pick(Chip::Esp32C6, 0, 160, 160),
            (Backend::Skia, true),
            "a smaller C6 display gets anti-aliasing"
        );
        assert_eq!(pick(Chip::Esp32C6, 0, 240, 320), (Backend::Integer, true));
        assert_eq!(pick(Chip::Esp32C3, 0, 240, 240), (Backend::Integer, true));
        assert!(pick(Chip::Esp32C2, 0, 128, 160).1, "whatever it picks fits");
    }
}
