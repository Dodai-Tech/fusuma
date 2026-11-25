#!/usr/bin/env nu
# ── Fusuma · test_web.nu ──────────────────────────────────────────
# The web showcase's build: the arguments, the paths and what is reported missing.

use std/assert
use runner.nu *
use ../web.nu [cargo_args wasm_path bindgen_args missing]

def "test web: the release build targets wasm and the crate" [] {
    assert equal (cargo_args false) ["build" "--target" "wasm32-unknown-unknown" "-p" "fusuma-showcase-web" "--release"]
    assert equal (cargo_args true) ["build" "--target" "wasm32-unknown-unknown" "-p" "fusuma-showcase-web"]
}

def "test web: the wasm file is where cargo puts it" [] {
    assert equal (wasm_path false) "target/wasm32-unknown-unknown/release/fusuma_showcase_web.wasm"
    assert equal (wasm_path true) "target/wasm32-unknown-unknown/debug/fusuma_showcase_web.wasm"
}

def "test web: bindgen writes the glue into the page's pkg directory" [] {
    let a = bindgen_args "x.wasm"
    assert equal ($a | first) "x.wasm"
    assert ("examples/showcase-web/pkg" in $a)
    assert ("web" in $a)
}

def "test web: a missing target or tool is named" [] {
    assert ((missing ["x86_64-unknown-linux-gnu"] true) | str contains "rustup target add wasm32-unknown-unknown")
    assert ((missing ["wasm32-unknown-unknown"] false) | str contains "wasm-bindgen")
    assert equal (missing ["wasm32-unknown-unknown"] true) ""
}

def "test web: the bindgen version is the one the crate pins" [] {
    use ../web.nu [BINDGEN]
    let manifest = open examples/showcase-web/Cargo.toml
    assert ($manifest.dependencies.wasm-bindgen | str contains $BINDGEN)
}
