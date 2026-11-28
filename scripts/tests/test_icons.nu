#!/usr/bin/env nu
# Tests for scripts/icons.nu: how Lucide's name table becomes Rust.
use std/assert
use ../icons.nu [const_name parse_info render]
use runner.nu *

def "test icons: a name becomes a constant, and a digit cannot start one" [] {
    assert equal (const_name "arrow-left") "ARROW_LEFT"
    assert equal (const_name "house") "HOUSE"
    assert equal (const_name "2k") "N2K"
}

def "test icons: the table is read from info.json and sorted by name" [] {
    let info = '{"x": {"encodedCode": "\\e1B2"}, "house": {"encodedCode": "\\e0f5"}}'
    assert equal (parse_info $info) [{ name: "house", hex: "e0f5" } { name: "x", hex: "e1b2" }]
}

def "test icons: the file has the table, the count and a constant each" [] {
    let out = render [{ name: "house", hex: "e0f5" } { name: "arrow-left", hex: "e048" }]
    assert ($out | str contains "pub const COUNT: usize = 2;")
    assert ($out | str contains "(\"house\", '\\u{e0f5}'),")
    assert ($out | str contains "pub const ARROW_LEFT: char = '\\u{e048}';")
}

def main [] { run-tests }
