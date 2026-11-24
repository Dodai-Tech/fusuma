#!/usr/bin/env nu
# ── Fusuma · test_embedded_matrix.nu ───────────────────────────────

use std/assert
use runner.nu *
use ../ci/embedded_matrix.nu [MATRIX build_args missing_targets]

def "test embedded matrix: one cargo build per entry, for the right package target and features" [] {
    let a = build_args { target: "thumbv7em-none-eabihf", features: "vello-cpu,libm" }
    assert equal $a ["build" "-p" "fusuma-platform-embedded" "--features" "vello-cpu,libm" "--target" "thumbv7em-none-eabihf"]
    let i = build_args { target: "thumbv6m-none-eabi", features: "" }
    assert equal $i ["build" "-p" "fusuma-platform-embedded" "--target" "thumbv6m-none-eabi"]
    let c = build_args { target: "thumbv6m-none-eabi", features: "", package: "fusuma-cells" }
    assert equal $c ["build" "-p" "fusuma-cells" "--target" "thumbv6m-none-eabi"]
}

def "test embedded matrix: the text UI cells build for the smallest chips" [] {
    let cells = $MATRIX | where { |e| ($e.package? | default "") == "fusuma-cells" } | get target
    assert ("thumbv6m-none-eabi" in $cells)
    assert ("riscv32imac-unknown-none-elf" in $cells)
}

def "test embedded matrix: Vello is never built for a Cortex-M0+" [] {
    let m0 = $MATRIX | where target == "thumbv6m-none-eabi"
    assert (($m0 | length) > 0)
    assert ($m0 | all { |e| not ($e.features | str contains "vello") })
}

def "test embedded matrix: every claimed architecture family is covered" [] {
    let targets = $MATRIX | get target | uniq
    for want in ["riscv32" "thumbv7em" "thumbv6m" "aarch64"] {
        assert ($targets | any { |t| $t | str starts-with $want }) $"no target for ($want)"
    }
}

def "test embedded matrix: only the missing targets are installed" [] {
    let m = [{ target: "a", features: "" } { target: "b", features: "" } { target: "a", features: "x" }]
    assert equal (missing_targets $m ["b"]) ["a"]
    assert equal (missing_targets $m ["a" "b"]) []
    assert equal (missing_targets [] ["a"]) []
}

def main [] { run-tests }
