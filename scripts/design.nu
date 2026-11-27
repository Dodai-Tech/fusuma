#!/usr/bin/env nu
# Build the design-tool files (OpenPencil / Figma `.fig`) from the same constants the toolkit paints with.
#
#   nu scripts/design.nu tokens   write design/tokens/<system>/*.tokens.json and design/spec/<system>.md (W3C design tokens
#                                 and the written spec), from `Theme` and `Palette` (crates/fusuma-ui/tests/design.rs)
#   nu scripts/design.nu build    write dist/design/fusuma.fig: the tokens as variables, drawn by design/build.js
#   nu scripts/design.nu verify   fail when the built .fig does not carry exactly the token variables
#   nu scripts/design.nu check    fail when the checked-in tokens or spec are stale (what CI runs, no bun needed)
#
# A folder under design/tokens is a variable collection, a file a mode; the system's own file (`<system>.tokens.json`)
# is the default mode. `build` needs bun (it runs `@open-pencil/cli`).

# How many token leaves a parsed token tree holds (a node with `$value` is a leaf).
export def leaves [node: record]: nothing -> int {
    if ("$value" in ($node | columns)) {
        1
    } else {
        $node | values | each { |v| if ($v | describe | str starts-with "record") { leaves $v } else { 0 } } | append 0 | math sum
    }
}

# The token files of a system, its own (default mode) first: `[<system>.tokens.json, ...the others sorted]`.
export def system_files [system: string, all: list<string>]: nothing -> list<string> {
    let own = ($all | where { ($in | path basename) == $"($system).tokens.json" })
    let rest = ($all | where { ($in | path basename) != $"($system).tokens.json" } | sort)
    $own | append $rest
}

def "main tokens" [] {
    with-env { FUSUMA_DESIGN_WRITE: "1" } { ^cargo test -q -p fusuma-ui --test design }
    print "wrote design/tokens and design/spec"
}

def "main check" [] {
    ^cargo test -q -p fusuma-ui --test design
    print "design tokens and spec: up to date"
}

def "main build" [--out: string = "dist/design/fusuma.fig"] {
    let tmp = (mktemp --directory)
    let base = $"($tmp)/base.html"
    "<div></div>" | save --force $base
    let fig = $"($tmp)/fusuma.fig"
    ^bunx @open-pencil/cli import $base -o $fig | ignore
    # One import per system: each folder is a collection, each file a mode.
    for entry in (ls design/tokens | where type == dir | sort-by name) {
        let dir = ($entry.name | path expand)
        let system = ($dir | path basename)
        let files = (system_files $system (glob $"($dir)/*.tokens.json") | each { |f|
            { path: $"($system)/($f | path basename)", text: (open --raw $f) }
        })
        { files: $files } | to json | save --force $"($tmp)/args.json"
        let res = (^bunx @open-pencil/cli tool call import_design_tokens $fig --args-file $"($tmp)/args.json" --write | lines | where { $in !~ '^Written to' } | str join "\n" | from json)
        print $"($system): ($res.added) variables, ($res.skipped | length) skipped"
    }
    print (open --raw design/build.js | ^bunx @open-pencil/cli eval $fig --stdin --write)
    mkdir ($out | path dirname)
    cp $fig $out
    print $"wrote ($out)"
}

def "main verify" [--fig: string = "dist/design/fusuma.fig"] {
    let want = (ls design/tokens | where type == dir | each { |d|
        leaves (open $"($d.name)/(($d.name | path basename)).tokens.json")
    } | math sum)
    let got = (^openpencil variables $fig | lines | where { $in =~ '^\[[0-9]+\]' } | length)
    if $want != $got {
        error make { msg: $"($fig) has ($got) variables, the token files define ($want): run `just design`" }
    }
    print $"design file: ($got) variables match the token files"
}

def main [] {
    print "usage: nu scripts/design.nu tokens | build | verify | check"
}
