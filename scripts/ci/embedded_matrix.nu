#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — the embedded platform builds for other architectures
# ──────────────────────────────────────────────────────────────────────────────
# `fusuma-platform-embedded` is the part every board shares, so it must build `no_std` for the kinds of chip Fusuma
# claims: 32-bit RISC-V, Cortex-M0+ (no atomics with compare-and-swap, so no Vello), Cortex-M4F and bare-metal ARM64.
#
#   nu scripts/ci/embedded_matrix.nu       # exit 1 and name the failing builds
# ──────────────────────────────────────────────────────────────────────────────

# Every target and the cargo features to build it with. Vello needs atomics a Cortex-M0+ lacks, so it is left out
# there; `libm` is software floating point, which `no_std` Vello needs.
export const MATRIX = [
    { target: "riscv32imac-unknown-none-elf", features: "skia" }
    { target: "riscv32imac-unknown-none-elf", features: "vello-cpu,libm" }
    { target: "thumbv7em-none-eabihf", features: "skia" }
    { target: "thumbv7em-none-eabihf", features: "vello-cpu,libm" }
    { target: "thumbv6m-none-eabi", features: "skia" }
    { target: "aarch64-unknown-none", features: "skia" }
    { target: "aarch64-unknown-none", features: "vello-cpu,libm" }
]

# The cargo arguments that build one entry.
export def build_args [entry: record]: nothing -> list<string> {
    ["build" "-p" "fusuma-platform-embedded" "--features" $entry.features "--target" $entry.target]
}

# The targets of `entries` the toolchain does not have installed yet.
export def missing_targets [entries: list, installed: list<string>]: nothing -> list<string> {
    $entries | get target | uniq | where { |t| $t not-in $installed }
}

def main [] {
    let installed = rustup target list --installed | lines | str trim
    for t in (missing_targets $MATRIX $installed) { ^rustup target add $t | ignore }
    mut failed = []
    for e in $MATRIX {
        let r = ^cargo ...(build_args $e) | complete
        if $r.exit_code != 0 { $failed = $failed | append $"($e.target) [($e.features)]" }
    }
    if ($failed | is-empty) {
        print -e $"embedded platform ok on ($MATRIX | get target | uniq | length) targets"
    } else {
        for f in $failed { print -e $"failed: ($f)" }
        exit 1
    }
}
