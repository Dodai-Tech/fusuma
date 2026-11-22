#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — Layer rule: the renderer never depends on the toolkit
# ──────────────────────────────────────────────────────────────────────────────
# `fusuma-render*` crates (the renderer) must not depend on any other `fusuma-*` crate (the
# toolkit), so a project that only needs drawing, such as a microcontroller firmware, never pulls
# in toolkit code. The renderer crates may depend on each other.
#
#   nu scripts/ci/layers.nu      # exit 1 and list the offending dependencies on a violation
# ──────────────────────────────────────────────────────────────────────────────

# Whether `name` is a renderer crate.
export def is_renderer [name: string]: nothing -> bool {
    ($name == "fusuma-render") or ($name | str starts-with "fusuma-render-")
}

# Whether `name` is a toolkit crate: a `fusuma` crate that is not a renderer crate.
export def is_toolkit [name: string]: nothing -> bool {
    (($name == "fusuma") or ($name | str starts-with "fusuma-")) and not (is_renderer $name)
}

# `{from, to}` pairs where a renderer crate depends on a toolkit crate. `packages` is the
# `packages` list of `cargo metadata`.
export def violations [packages: list]: nothing -> list<record> {
    $packages
    | where { |p| is_renderer $p.name }
    | each { |p|
        $p.dependencies
        | where { |d| is_toolkit $d.name }
        | each { |d| { from: $p.name, to: $d.name } }
    }
    | flatten
}

def main [] {
    let packages = cargo metadata --format-version 1 --no-deps | from json | get packages
    let bad = violations $packages
    if ($bad | is-empty) {
        print -e "layers ok: no renderer crate depends on a toolkit crate"
    } else {
        for v in $bad { print -e $"($v.from) depends on ($v.to): a renderer must not know the toolkit" }
        exit 1
    }
}
