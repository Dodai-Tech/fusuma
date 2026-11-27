#!/usr/bin/env nu
# Build the design-tool files (OpenPencil / Figma `.fig`) from the same constants the toolkit paints with.
#
#   nu scripts/design.nu tokens   write design/tokens/<system>/*.tokens.json and design/spec/<system>.md (W3C design tokens
#                                 and the written spec), from `Theme` and `Palette` (crates/fusuma-ui/tests/design.rs)
#   nu scripts/design.nu build    write dist/design/fusuma.fig: the tokens as variables, drawn by design/build.js
#   nu scripts/design.nu verify   fail when the built .fig does not carry exactly the token variables (names and values)
#                                 the Foundations and Controls frames, and what the Controls page draws (tick arm, hatch step, dot step)
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
    # The palettes: a collection each (dark and light mode), drawn on the Palettes page.
    for entry in (ls design/palettes | where type == dir | sort-by name) {
        let dir = ($entry.name | path expand)
        let name = ($dir | path basename)
        let files = (system_files $name (glob $"($dir)/*.tokens.json") | each { |f|
            { path: $"palette-($name)/($f | path basename)", text: (open --raw $f) }
        })
        { files: $files } | to json | save --force $"($tmp)/args.json"
        ^bunx @open-pencil/cli tool call import_design_tokens $fig --args-file $"($tmp)/args.json" --write | ignore
    }
    print (open --raw design/build.js | ^bunx @open-pencil/cli eval $fig --stdin --write)
    mkdir ($out | path dirname)
    cp $fig $out
    print $"wrote ($out)"
}

# The token leaves of a parsed tree as `{ name: "group/name", value: <the value> }` rows.
export def flat [node: record, prefix: string = ""]: nothing -> list<record> {
    $node | transpose key val | each { |row|
        let name = (if $prefix == "" { $row.key } else { $"($prefix)/($row.key)" })
        if ($row.val | describe | str starts-with "record") {
            if ("$value" in ($row.val | columns)) { [{ name: $name, value: ($row.val | get '$value') }] } else { flat $row.val $name }
        } else { [] }
    } | flatten
}

def "main verify" [--fig: string = "dist/design/fusuma.fig"] {
    let doc = (^bunx @open-pencil/cli variables $fig --json | from json)
    mut errors = []
    mut count = 0
    for entry in (ls design/tokens | where type == dir | sort-by name) {
        let system = ($entry.name | path basename)
        let want = (flat (open $"($entry.name)/($system).tokens.json"))
        let col = ($doc.collections | where name == $system)
        if ($col | is-empty) { $errors = ($errors | append $"collection ($system) is missing"); continue }
        let have = ($col | first | get variables)
        $count += ($have | length)
        if ($want | length) != ($have | length) {
            $errors = ($errors | append $"($system): ($have | length) variables, the tokens define ($want | length)")
        }
        # Every token is a variable of the same name and the same (default mode) value.
        for w in $want {
            let h = ($have | where name == $w.name)
            if ($h | is-empty) { $errors = ($errors | append $"($system): ($w.name) is missing") } else if (($h | first | get value | into string) != ($w.value | into string)) {
                $errors = ($errors | append $"($system): ($w.name) is ($h | first | get value), the tokens say ($w.value)")
            }
        }
    }
    # Every palette is a collection of 16 colours whose dark and light values are the tokens.
    for entry in (ls design/palettes | where type == dir | sort-by name) {
        let name = ($entry.name | path basename)
        let col = ($doc.collections | where name == $"palette-($name)")
        if ($col | is-empty) { $errors = ($errors | append $"palette ($name) is missing"); continue }
        $count += ($col | first | get variables | length)
        for w in (flat (open $"($entry.name)/($name).tokens.json")) {
            let h = ($col | first | get variables | where name == $w.name)
            if ($h | is-empty) { $errors = ($errors | append $"palette ($name): ($w.name) is missing") } else if (($h | first | get value | into string) != ($w.value | into string)) {
                $errors = ($errors | append $"palette ($name): ($w.name) is ($h | first | get value), the tokens say ($w.value)")
            }
        }
    }
    let palettes = (ls design/palettes | where type == dir | length)
    let rows = (^bunx @open-pencil/cli find $fig --type FRAME --name "Palette row" --json | from json | length)
    if $rows < $palettes { $errors = ($errors | append $"($rows) palette rows for ($palettes) palettes") }
    # What the pages draw follows the decoration tokens: tick arm, hatch step and dot step (design/check.js).
    let drawn = (open --raw design/check.js | ^bunx @open-pencil/cli eval $fig --stdin | from json | from json)
    for entry in (ls design/tokens | where type == dir | sort-by name) {
        let system = ($entry.name | path basename)
        let decor = (open $"($entry.name)/($system).tokens.json" | get decor)
        let got = ($drawn | get -o $system)
        if $got == null { $errors = ($errors | append $"($system): no Controls page"); continue }
        for pair in [[ticks arm] [hatch hatch] [dots dots]] {
            let want = ($decor | get -o ($pair | first) | default { '$value': 0 } | get '$value')
            let have = ($got | get ($pair | last))
            if $want != $have { $errors = ($errors | append $"($system): the page draws ($pair | last) ($have), decor/($pair | first) says ($want)") }
        }
    }
    # Every system draws both pages.
    let systems = (ls design/tokens | where type == dir | length)
    for frame in ["Foundations" "Controls"] {
        let n = (^bunx @open-pencil/cli find $fig --type FRAME --name $frame --json | from json | length)
        if $n < $systems { $errors = ($errors | append $"($n) ($frame) frames for ($systems) systems") }
    }
    if ($errors | is-not-empty) {
        error make { msg: $"($fig) is out of step with design/tokens, run `just design`:\n(($errors | str join (char nl)))" }
    }
    print $"design file: ($count) variables match the token files by name and value"
}

def main [] {
    print "usage: nu scripts/design.nu tokens | build | verify | check"
}
