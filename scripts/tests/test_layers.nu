#!/usr/bin/env nu
# ── Fusuma · test_layers.nu ───────────────────────────────────────────────

use std/assert
use runner.nu *
use ../ci/layers.nu [violations is_renderer is_toolkit]

def pkg [name: string, deps: list<string>]: nothing -> record {
    { name: $name, dependencies: ($deps | each { |d| { name: $d } }) }
}

def "test layers: crates are told apart by name" [] {
    assert (is_renderer "fusuma-render")
    assert (is_renderer "fusuma-render-skia")
    assert (not (is_renderer "fusuma-renderer-ui"))
    assert (not (is_renderer "fusuma-core"))
    assert (is_toolkit "fusuma")
    assert (is_toolkit "fusuma-core")
    assert (is_toolkit "fusuma-renderer-ui")
    assert (not (is_toolkit "fusuma-render-embedded"))
    assert (not (is_toolkit "tiny-skia"))
}

def "test layers: a renderer crate that depends on the toolkit is reported" [] {
    let bad = violations [(pkg fusuma-render-skia [tiny-skia fusuma-core]) (pkg fusuma-render [])]
    assert equal $bad [{ from: fusuma-render-skia, to: fusuma-core }]
}

def "test layers: renderer crates may depend on each other, and the toolkit on the renderer" [] {
    assert equal (violations [(pkg fusuma-render-skia [fusuma-render fusuma-render-embedded])]) []
    assert equal (violations [(pkg fusuma-core [fusuma-render fusuma-render-embedded]) (pkg fusuma-widgets [fusuma-core])]) []
}

def "test layers: unrelated crates and empty lists are fine" [] {
    assert equal (violations []) []
    assert equal (violations [(pkg something [fusuma-core])]) []
    assert equal (violations [(pkg fusuma-render [])]) []
}

def "test layers: every offending dependency is listed" [] {
    let bad = violations [(pkg fusuma-render [fusuma-a fusuma-b]) (pkg fusuma-render-embedded [fusuma-a])]
    assert equal ($bad | length) 3
}

def "test layers: this workspace follows the rule" [] {
    let packages = cargo metadata --format-version 1 --no-deps | from json | get packages
    assert equal (violations $packages) []
}

def main [] { run-tests }
