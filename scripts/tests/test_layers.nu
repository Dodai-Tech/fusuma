#!/usr/bin/env nu
# ── Fusuma · test_layers.nu ───────────────────────────────────────────────

use std/assert
use runner.nu *
use ../ci/layers.nu [violations is_renderer is_toolkit platform_violations is_platform_source]

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
    assert (not (is_toolkit "fusuma-render-integer"))
    assert (not (is_toolkit "vello_cpu"))
}

def "test layers: a renderer crate that depends on the toolkit is reported" [] {
    let bad = violations [(pkg fusuma-render-vello-cpu [vello_cpu fusuma]) (pkg fusuma-render [])]
    assert equal $bad [{ from: fusuma-render-vello-cpu, to: fusuma }]
}

def "test layers: renderer crates may depend on each other, and the toolkit on the renderer" [] {
    assert equal (violations [(pkg fusuma-render-vello-cpu [fusuma-render fusuma-render-integer])]) []
    assert equal (violations [(pkg fusuma [fusuma-render fusuma-render-integer]) (pkg fusuma-widgets [fusuma])]) []
}

def "test layers: text and font crates are drawing support, which a renderer may use" [] {
    assert (not (is_toolkit "fusuma-text"))
    assert (not (is_toolkit "fusuma-fonts"))
    assert (not (is_toolkit "fusuma-icons"))
    assert (not (is_toolkit "fusuma-atlas"))
    assert (is_toolkit "fusuma-ui")
    assert equal (violations [(pkg fusuma-render-vello-common [fusuma-render fusuma-text fusuma-fonts])]) []
    assert equal (violations [(pkg fusuma-render-vello-cpu [fusuma-text fusuma-ui])] | length) 1
}

def "test layers: unrelated crates and empty lists are fine" [] {
    assert equal (violations []) []
    assert equal (violations [(pkg something [fusuma])]) []
    assert equal (violations [(pkg fusuma-render [])]) []
}

def "test layers: every offending dependency is listed" [] {
    let bad = violations [(pkg fusuma-render [fusuma-a fusuma-b]) (pkg fusuma-render-integer [fusuma-a])]
    assert equal ($bad | length) 3
}

def "test layers: this workspace follows the rule" [] {
    let packages = cargo metadata --format-version 1 --no-deps | from json | get packages
    assert equal (violations $packages) []
}

def "test layers: platform crates are told apart from the chooser" [] {
    assert (is_platform_source "crates/fusuma-platform-esp/src/lib.rs")
    assert (is_platform_source "/home/x/fusuma/crates/fusuma-platform-desktop/src/window.rs")
    assert (not (is_platform_source "crates/fusuma-platform/src/lib.rs"))
    assert (not (is_platform_source "crates/fusuma-ui/src/tree.rs"))
}

def "test layers: a platform crate that builds widgets or renders is reported" [] {
    let files = [
        { path: "crates/fusuma-platform-esp/src/a.rs", text: "let t: Tree<8> = make();" }
        { path: "crates/fusuma-platform-esp/src/d.rs", text: "let t = Tree::<8>::new();" }
        { path: "crates/fusuma-platform-esp/src/b.rs", text: "impl Renderer for Mine {}" }
        { path: "crates/fusuma-platform-desktop/src/c.rs", text: "let s = Style::new();\nButtonVariant::Primary" }
    ]
    let bad = platform_violations $files
    assert equal ($bad | length) 5
    assert equal ($bad | where file == "crates/fusuma-platform-esp/src/a.rs" | get pattern) ["Tree<"]
    assert equal ($bad | where file == "crates/fusuma-platform-esp/src/d.rs" | get pattern) ["Tree::<"]
}

def "test layers: comments, other crates and clean platform code are fine" [] {
    let files = [
        { path: "crates/fusuma-platform-esp/src/a.rs", text: "// a Tree<N> is built by the app\nlet shell = Shell::new(app);" }
        { path: "crates/fusuma-ui/src/tree.rs", text: "pub struct Tree<const N: usize>;" }
        { path: "crates/fusuma-platform/src/lib.rs", text: "Tree<" }
    ]
    assert equal (platform_violations $files) []
    assert equal (platform_violations []) []
}

def "test layers: no platform crate in this workspace defines components" [] {
    let sources = glob crates/fusuma-platform-*/src/**/*.rs | each { |p| { path: $p, text: (open --raw $p) } }
    assert (($sources | length) > 0)
    assert equal (platform_violations $sources) []
}

def main [] { run-tests }
