#!/usr/bin/env nu
# Tests for scripts/icons.nu: how Lucide's name table becomes Rust.
use std/assert
use ../icons.nu [variant_name parse_info render]
use runner.nu *

def "test icons: a name becomes a variant, and a digit cannot start one" [] {
    assert equal (variant_name "arrow-left") "ArrowLeft"
    assert equal (variant_name "house") "House"
    assert equal (variant_name "2k") "N2k"
    assert equal (variant_name "arrow-down-0-1") "ArrowDown0_1"
    assert equal (variant_name "arrow-down-01") "ArrowDown01"
}

def "test icons: the table is read from info.json and sorted by name" [] {
    let info = '{"x": {"encodedCode": "\\e1B2"}, "house": {"encodedCode": "\\e0f5"}}'
    assert equal (parse_info $info) [{ name: "house", hex: "e0f5" } { name: "x", hex: "e1b2" }]
}

def "test icons: the file has the enum, the count and the two tables" [] {
    let out = render [{ name: "arrow-left", hex: "e048" } { name: "house", hex: "e0f5" }]
    assert ($out | str contains "pub const COUNT: usize = 2;")
    assert ($out | str contains "    ArrowLeft,\n    /// `house`.\n    House,")
    assert ($out | str contains "    \"house\",")
    assert ($out | str contains "    '\\u{e0f5}',")
    assert ($out | str contains "Self::ArrowLeft,\n        Self::House,")
}

def main [] { run-tests }
