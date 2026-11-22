#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — Layer rule: the renderer never depends on the toolkit
# ──────────────────────────────────────────────────────────────────────────────
# `tokonoma*` crates (the renderer) must not depend on `fusuma*` crates (the toolkit), so a project
# that only needs drawing, such as a microcontroller firmware, never pulls in toolkit code.
#
#   nu scripts/ci/layers.nu      # exit 1 and list the offending dependencies on a violation
# ──────────────────────────────────────────────────────────────────────────────

# `{from, to}` pairs where a renderer crate depends on a toolkit crate. `packages` is the
# `packages` list of `cargo metadata`.
export def violations [packages: list]: nothing -> list<record> {
    $packages
    | where { |p| $p.name | str starts-with "tokonoma" }
    | each { |p|
        $p.dependencies
        | where { |d| $d.name | str starts-with "fusuma" }
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
