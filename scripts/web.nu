#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — the showcase in a web page (examples/showcase-web)
# ──────────────────────────────────────────────────────────────────────────────
# WebAssembly with Vello on the CPU drawing one canvas. `build` compiles the crate for `wasm32-unknown-unknown` and runs
# `wasm-bindgen` over it into `examples/showcase-web/pkg`; `serve` serves that directory.
#
#   nu scripts/web.nu install              # the wasm target and wasm-bindgen's CLI, if missing
#   nu scripts/web.nu build [--debug]
#   nu scripts/web.nu serve [--port 8080]
#   nu scripts/web.nu size                 # the .wasm and the pkg, in bytes
# ──────────────────────────────────────────────────────────────────────────────

const ROOT = path self ..
export const CRATE = "fusuma-showcase-web"
export const TARGET = "wasm32-unknown-unknown"
export const DIR = "examples/showcase-web"
# The wasm-bindgen CLI must be the version of the crate (Cargo.toml of the web showcase pins it with `=`).
export const BINDGEN = "0.2.130"

# The cargo arguments that compile the crate for the web.
export def cargo_args [debug: bool]: nothing -> list<string> {
    let profile = if $debug { [] } else { ["--release"] }
    ["build" "--target" $TARGET "-p" $CRATE ...$profile]
}

# Where cargo leaves the .wasm file.
export def wasm_path [debug: bool]: nothing -> string {
    let profile = if $debug { "debug" } else { "release" }
    $"target/($TARGET)/($profile)/($CRATE | str replace --all '-' '_').wasm"
}

# The wasm-bindgen arguments that make the JavaScript glue next to the page.
export def bindgen_args [wasm: string]: nothing -> list<string> {
    [$wasm "--target" "web" "--no-typescript" "--out-dir" $"($DIR)/pkg"]
}

# What is missing to build, as a message, or an empty string.
export def missing [targets: list<string>, has_bindgen: bool]: nothing -> string {
    if $TARGET not-in $targets { return $"the Rust target is missing: rustup target add ($TARGET)" }
    if not $has_bindgen { return "wasm-bindgen is missing: cargo install wasm-bindgen-cli --locked (its version must match the crate's wasm-bindgen)" }
    ""
}

# Installs wasm-bindgen's command line tool when it is not there (CI; a developer has it or follows `missing`).
def "main install" [] {
    if (which wasm-bindgen | is-empty) {
        ^cargo install wasm-bindgen-cli --locked --version $BINDGEN
    }
    ^rustup target add $TARGET
}

def "main build" [--debug] {
    cd $ROOT
    let installed = rustup target list --installed | lines | str trim
    let problem = missing $installed (which wasm-bindgen | is-not-empty)
    if $problem != "" { error make { msg: $problem } }
    ^cargo ...(cargo_args $debug)
    ^wasm-bindgen ...(bindgen_args (wasm_path $debug))
    print -e $"built ($DIR)/pkg"
}

def "main serve" [--port: int = 8080] {
    cd $"($ROOT)/($DIR)"
    print -e $"http://localhost:($port)/index.html"
    ^python3 -m http.server $port
}

def "main size" [] {
    cd $ROOT
    ls $"($DIR)/pkg" | select name size
}

def main [] {
    print "nu scripts/web.nu build | serve | size"
}
