#!/usr/bin/env nu
# ── Fusuma · test_showcase.nu ─────────────────────────────────────
# The P4 showcase's helpers: the generated sdkconfig, the OTA entry and the partition table.

use std/assert
use runner.nu *
use ../showcase.nu [sdkconfig crc32 le_bytes otadata_entry slot_seq parse_ptable partition check_fits port_args feature_args]

def "test showcase: the partition path is filled in everywhere" [] {
    let out = sdkconfig "A=1\nP=\"@PARTITIONS@\"\nQ=\"@PARTITIONS@\"\n" "/x/p.csv"
    assert equal $out "A=1\nP=\"/x/p.csv\"\nQ=\"/x/p.csv\"\n"
}

def "test showcase: crc32 matches the standard check value" [] {
    assert equal (crc32 ("123456789" | into binary)) 0xCBF43926
}

def "test showcase: little-endian bytes" [] {
    assert equal (le_bytes 0x01020304 4) 0x[04 03 02 01]
    assert equal (le_bytes 1 4) 0x[01 00 00 00]
}

def "test showcase: an otadata entry is 32 bytes with the sequence, a blank label and its CRC" [] {
    let e = otadata_entry 1
    assert equal ($e | bytes length) 32
    assert equal ($e | bytes at 0..<4) 0x[01 00 00 00]
    assert equal ($e | bytes at 4..<24) (0..<20 | each { 0x[ff] } | bytes collect)
    assert equal ($e | bytes at 24..<28) 0x[ff ff ff ff]
    assert equal ($e | bytes at 28..<32) (le_bytes (crc32 0x[01 00 00 00] 0xFFFFFFFF) 4)
    assert ((otadata_entry 2 | bytes at 28..<32) != ($e | bytes at 28..<32))
}

def "test showcase: a sequence below 1 is refused" [] {
    assert error { otadata_entry 0 }
}

def "test showcase: ota_0 is sequence 1 and others are rejected" [] {
    assert equal (slot_seq "ota_0") 1
    assert equal (slot_seq "ota_1") 2
    assert error { slot_seq "factory" }
    assert error { slot_seq "ota_" }
}

def entry [name: string, offset: int, size: int]: nothing -> binary {
    let label = ($name | into binary) | append (0..<(16 - ($name | str length)) | each { 0x[00] } | bytes collect) | bytes collect
    [0x[aa 50 01 00] (le_bytes $offset 4) (le_bytes $size 4) $label 0x[00 00 00 00]] | bytes collect
}

def "test showcase: a partition table is read until the first blank entry" [] {
    let raw = [(entry "factory" 0x110000 0x900000) (entry "ota_0" 0xa10000 0x3f0000) (0..<32 | each { 0x[ff] } | bytes collect) (entry "ghost" 1 1)] | bytes collect
    let t = parse_ptable $raw
    assert equal ($t | length) 2
    assert equal (partition $t "ota_0").offset 0xa10000
    assert equal (partition $t "ota_0").size 0x3f0000
    assert error { partition $t "otadata" }
}

def "test showcase: an image that is too big for its slot is refused" [] {
    let slot = { name: "ota_0", offset: 0, size: 100 }
    check_fits $slot 100
    assert error { check_fits $slot 101 }
}

def "test showcase: the port is passed only when given" [] {
    assert equal (port_args "") []
    assert equal (port_args "/dev/ttyACM0") ["--port" "/dev/ttyACM0"]
}

def "test showcase: cargo features are passed through, in any list format" [] {
    assert equal (feature_args "") []
    assert equal (feature_args "render-skia") ["--features" "render-skia"]
    assert equal (feature_args "a b , c") ["--features" "a,b,c"]
}

def main [] { run-tests }
