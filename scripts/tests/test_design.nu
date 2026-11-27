#!/usr/bin/env nu
# ── Fusuma · test_design.nu ───────────────────────────────────────
# The design-file helpers: counting token leaves and picking a system's default-mode file first.

use std/assert
use runner.nu *
use ../design.nu [leaves system_files flat]

def "test design: a token leaf is a node with a value, groups are counted through" [] {
    assert equal (leaves { color: { a: { "$type": "color", "$value": "#000000" }, b: { "$type": "color", "$value": "#ffffff" } }, size: { x: { "$type": "number", "$value": 1 } } }) 3
    assert equal (leaves { "$type": "number", "$value": 4 }) 1
    assert equal (leaves {}) 0
}

def "test design: a system own file comes first and is the default mode" [] {
    let got = (system_files "cyberdeck" ["/d/cyberdeck-light.tokens.json" "/d/cyberdeck.tokens.json" "/d/a.tokens.json"])
    assert equal ($got | first) "/d/cyberdeck.tokens.json"
    assert equal ($got | length) 3
}

def "test design: the checked-in tokens carry the same number of leaves in every mode of a system" [] {
    for d in (ls design/tokens | where type == dir) {
        let counts = (glob $"($d.name | path expand)/*.tokens.json" | each { |f| leaves (open $f) } | uniq)
        assert equal ($counts | length) 1
    }
}

def "test design: flat names a token group/name with its value" [] {
    let got = (flat { color: { a: { "$type": "color", "$value": "#000000" } }, size: { x: { "$type": "number", "$value": 1 } } })
    assert equal $got [{ name: "color/a", value: "#000000" } { name: "size/x", value: 1 }]
}

def main [] { run-tests }
