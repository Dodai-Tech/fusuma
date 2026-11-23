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

# What a platform crate must not contain: a component exists once, in the toolkit, and runs on every platform.
# A platform builds no widget tree and draws nothing itself; it brings up a screen and input and uses the
# shared shell.
export const PLATFORM_FORBIDDEN = [
    { pattern: "Tree<", why: "a platform builds no widget tree: the app does" }
    { pattern: "Tree::<", why: "a platform builds no widget tree: the app does" }
    { pattern: "Style::new", why: "a platform lays nothing out: the toolkit does" }
    { pattern: "TextRole::", why: "a platform draws no text of its own: the toolkit does" }
    { pattern: "ButtonVariant", why: "a platform defines no widgets: they live in fusuma-ui" }
    { pattern: "impl Renderer for", why: "a renderer is a fusuma-render-* crate, not part of a platform" }
    { pattern: "fn fill_rect(", why: "a platform draws no shapes: a renderer does" }
]

# Whether `path` is the source of a platform crate (`fusuma-platform-*`, not the chooser `fusuma-platform`).
export def is_platform_source [path: string]: nothing -> bool {
    $path =~ 'fusuma-platform-[^/]+/src/'
}

# `{file, pattern, why}` for each forbidden pattern in a platform crate's source. `files` is a list of
# `{path, text}`; comment lines are ignored.
export def platform_violations [files: list]: nothing -> list<record> {
    $files
    | where { |f| is_platform_source $f.path }
    | each { |f|
        let code = $f.text | lines | where { |l| not ($l | str trim | str starts-with "//") } | str join "\n"
        $PLATFORM_FORBIDDEN
        | where { |r| $code | str contains $r.pattern }
        | each { |r| { file: $f.path, pattern: $r.pattern, why: $r.why } }
    }
    | flatten
}

def main [] {
    let packages = cargo metadata --format-version 1 --no-deps | from json | get packages
    let bad = violations $packages
    let sources = glob crates/fusuma-platform-*/src/**/*.rs | each { |p| { path: $p, text: (open --raw $p) } }
    let dup = platform_violations $sources
    if ($bad | is-empty) and ($dup | is-empty) {
        print -e "layers ok: no renderer crate depends on a toolkit crate, and no platform crate defines components"
    } else {
        for v in $bad { print -e $"($v.from) depends on ($v.to): a renderer must not know the toolkit" }
        for v in $dup { print -e $"($v.file) contains '($v.pattern)': ($v.why)" }
        exit 1
    }
}
