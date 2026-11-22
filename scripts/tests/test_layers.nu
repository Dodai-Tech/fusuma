#!/usr/bin/env nu
# ── Fusuma · test_layers.nu ───────────────────────────────────────────────

use std/assert
use runner.nu *
use ../ci/layers.nu [violations]

def pkg [name: string, deps: list<string>]: nothing -> record {
    { name: $name, dependencies: ($deps | each { |d| { name: $d } }) }
}

def "test layers: a renderer crate that depends on the toolkit is reported" [] {
    let bad = violations [(pkg tokonoma-skia [tiny-skia fusuma-core]) (pkg tokonoma [])]
    assert equal $bad [{ from: tokonoma-skia, to: fusuma-core }]
}

def "test layers: the toolkit may depend on the renderer" [] {
    assert equal (violations [(pkg fusuma-core [tokonoma tokonoma-embedded]) (pkg fusuma-widgets [fusuma-core])]) []
}

def "test layers: unrelated crates and empty lists are fine" [] {
    assert equal (violations []) []
    assert equal (violations [(pkg something [fusuma-core])]) []
    assert equal (violations [(pkg tokonoma [])]) []
}

def "test layers: every offending dependency is listed" [] {
    let bad = violations [(pkg tokonoma [fusuma-a fusuma-b]) (pkg tokonoma-embedded [fusuma-a])]
    assert equal ($bad | length) 3
}

def "test layers: this workspace follows the rule" [] {
    let packages = cargo metadata --format-version 1 --no-deps | from json | get packages
    assert equal (violations $packages) []
}

def main [] { run-tests }
