#!/usr/bin/env nu
# Tests for scripts/fonts.nu: the catalog's consistency and how each family is obtained.
use std/assert
use ../fonts.nu [catalog weights origin instance_args subset_args licence_url icon_codes icon_coverage]
use runner.nu *

def "test fonts: every family has an id, a role and a source, and every preset names linked roles" [] {
    let c = catalog
    assert (($c.family.id | uniq | length) == ($c.family | length)) "ids are unique"
    for f in $c.family {
        assert ($f.role in ["sans" "mono" "icons"]) $"($f.id): role"
        if $f.role == "icons" { assert (($f.icons | length) > 0) $"($f.id) names its icons" }
        for w in (weights $f) { origin $f $w }
        assert ($f.id =~ '^[a-z0-9-]+$') $"($f.id) is a cargo feature name"
    }
    for p in $c.preset {
        let sans = $c.family | where id == $p.sans
        let mono = $c.family | where id == $p.mono
        assert (($sans | length) == 1 and ($sans.0.role == "sans")) $"($p.id).sans"
        assert (($mono | length) == 1 and ($mono.0.role == "mono")) $"($p.id).mono"
    }
    assert equal ($c.preset | first | get id) "fusuma" "the default preset comes first"
}

def "test fonts: a sans has a bold, a mono does not" [] {
    assert equal (weights { role: "sans" }) ["regular" "bold"]
    assert equal (weights { role: "mono" }) ["regular"]
}

def "test fonts: a family is fetched from a zip, a variable font or static files" [] {
    let zip = { id: "z", url: "https://x/f.zip", regular: "F-Regular.ttf", bold: "F-Bold.ttf", role: "sans" }
    assert equal (origin $zip "bold") { kind: "zip", url: "https://x/f.zip", member: "F-Bold.ttf" }
    assert error { origin ($zip | reject bold) "bold" }
    let var = { id: "v", url: "https://x/F%5Bwdth,wght%5D.ttf", axes: { wdth: 100 }, role: "sans" }
    assert equal (origin $var "regular") { kind: "variable", url: $var.url, axes: { wdth: 100, wght: 400 } }
    assert equal (origin $var "bold").axes.wght 700
    let st = { id: "s", url: "https://x/F-Regular.ttf", bold_url: "https://x/F-Bold.ttf", role: "sans" }
    assert equal (origin $st "bold") { kind: "static", url: "https://x/F-Bold.ttf" }
    assert error { origin ($st | reject bold_url) "bold" }
}

def "test fonts: the fonttools arguments pin every axis and keep the coverage" [] {
    assert equal (instance_args "in.ttf" "out.ttf" { wght: 700, wdth: 100 }) ["varLib.instancer" "in.ttf" "wdth=100" "wght=700" "--static" "--update-name-table" "-o" "out.ttf"]
    let a = subset_args "in.ttf" "out.ttf" "U+0020-007E, U+00A0-017F"
    assert ("--unicodes=U+0020-007E,U+00A0-017F" in $a)
    assert ("--output-file=out.ttf" in $a)
    assert equal (licence_url { url: "https://github.com/google/fonts/raw/main/ofl/b612/B612-Regular.ttf" }) "https://github.com/google/fonts/raw/main/ofl/b612/OFL.txt"
    assert equal (licence_url { url: "https://github.com/rsms/inter/releases/x.zip" }) ""
}

def "test fonts: icon names become codepoints, and a missing one is named" [] {
    let table = "home e88a\nsettings e8b8\nhouse e88a\n"
    let codes = icon_codes ["settings" "home" "house"] $table
    assert equal $codes [{ name: "settings", hex: "e8b8" } { name: "home", hex: "e88a" } { name: "house", hex: "e88a" }]
    assert equal (icon_coverage $codes) "U+e8b8, U+e88a" "one range a character"
    assert error { icon_codes ["nope"] $table }
    assert equal (licence_url { url: "https://x/f.ttf", licence_url: "https://x/LICENSE" }) "https://x/LICENSE"
}

def main [] { run-tests }
