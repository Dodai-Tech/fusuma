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
    assert (is_renderer "fusuma-render-vello-cpu")
    assert (not (is_renderer "fusuma-renderer-ui"))
    assert (not (is_renderer "fusuma"))
    assert (is_toolkit "fusuma")
    assert (is_toolkit "fusuma")
    assert (is_toolkit "fusuma-renderer-ui")
    assert (not (is_toolkit "fusuma-render-vello-cpu"))
    assert (not (is_toolkit "vello_cpu"))
}

def "test layers: a renderer crate that depends on the toolkit is reported" [] {
    let bad = violations [(pkg fusuma-render-vello-cpu [vello_cpu fusuma]) (pkg fusuma-render [])]
    assert equal $bad [{ from: fusuma-render-vello-cpu, to: fusuma }]
}

def "test layers: renderer crates may depend on each other, and the toolkit on the renderer" [] {
    assert equal (violations [(pkg fusuma-render-vello-cpu [fusuma-render fusuma-render-vello-common])]) []
    assert equal (violations [(pkg fusuma [fusuma-render fusuma-render-vello-common]) (pkg fusuma-widgets [fusuma])]) []
}

def "test layers: unrelated crates and empty lists are fine" [] {
    assert equal (violations []) []
    assert equal (violations [(pkg something [fusuma])]) []
    assert equal (violations [(pkg fusuma-render [])]) []
}

def "test layers: every offending dependency is listed" [] {
    let bad = violations [(pkg fusuma-render [fusuma-a fusuma-b]) (pkg fusuma-render-vello-common [fusuma-a])]
    assert equal ($bad | length) 3
}

def "test layers: this workspace follows the rule" [] {
    let packages = cargo metadata --format-version 1 --no-deps | from json | get packages
    assert equal (violations $packages) []
}

def main [] { run-tests }
