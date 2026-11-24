#!/usr/bin/env nu
# ── Fusuma · test_showcase.nu ─────────────────────────────────────
# The ESP showcase's helpers: the boards and how a connected one is recognised, the generated sdkconfig, the OTA
# entry and the partition table.

use std/assert
use runner.nu *
use ../showcase.nu [sdkconfig crc32 le_bytes otadata_entry slot_seq parse_ptable partition check_fits port_args feature_args BOARDS_DIR CHIPS board_from load_boards board_named parse_board_info pick_board check_bootable espup_env build_args]

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
    assert equal (feature_args "render-vello-cpu") ["--features" "render-vello-cpu"]
    assert equal (feature_args "a b , c") ["--features" "a,b,c"]
}


const BOARD_INFO_S3 = "[INFO ] Connecting...
Chip type:         esp32s3 (revision v0.2)
Crystal frequency: 40 MHz
Flash size:        32MB
Features:          WiFi, BLE, Embedded Flash"

def "test showcase: board-info gives the chip and the flash size" [] {
    assert equal (parse_board_info $BOARD_INFO_S3) { chip: "esp32s3", flash_mb: 32 }
    assert equal (parse_board_info "Chip type:   esp32p4 (revision v1.3)\nFlash size:  16MB") { chip: "esp32p4", flash_mb: 16 }
    assert error { parse_board_info "Error: no device" }
}

def "test showcase: each connected chip picks its board" [] {
    assert equal (pick_board { chip: "esp32s3", flash_mb: 32 }).name "s3-amoled-206"
    assert equal (pick_board { chip: "esp32p4", flash_mb: 32 }).name "p4-7in"
    assert error { pick_board { chip: "esp32c6", flash_mb: 4 } }
}

def "test showcase: two boards with one chip are told apart by flash, else --board is needed" [] {
    let boards = [
        { name: "big", chip: "esp32s3", flash_mb: 32 }
        { name: "small", chip: "esp32s3", flash_mb: 8 }
        { name: "twin", chip: "esp32c6", flash_mb: 4 }
        { name: "twin2", chip: "esp32c6", flash_mb: 4 }
    ]
    assert equal (pick_board { chip: "esp32s3", flash_mb: 8 } $boards).name "small"
    assert error { pick_board { chip: "esp32s3", flash_mb: 16 } $boards }
    assert error { pick_board { chip: "esp32c6", flash_mb: 4 } $boards }
}

def "test showcase: the boards come from their board files, with the chip target and toolchain" [] {
    let boards = load_boards
    assert equal ($boards.name) ["p4-7in" "s3-amoled-206"]
    for b in $boards {
        assert ($b.dir | path join sdkconfig.defaults.in | path exists) $"($b.name) has an sdkconfig template"
        assert ($b.dir | path join partitions.csv | path exists) $"($b.name) has a partition table"
        let idf = open --raw ($b.dir | path join sdkconfig.defaults.in)
        assert ($idf | str contains $'CONFIG_IDF_TARGET="($b.chip)"') $"($b.name)'s sdkconfig is for its chip"
        assert equal $b.target ($CHIPS | get $b.chip | get target)
    }
    let s3 = board_named "s3-amoled-206"
    assert equal [$s3.chip $s3.flash_mb $s3.boot_limit_mb $s3.toolchain $s3.espup] ["esp32s3" 32 16 "esp" true]
    assert equal (board_named "p4-7in").boot_limit_mb 0 "no limit when the file says none"
    assert error { board_named "nope" }
}

def "test showcase: a new board is a file, and an unknown chip is refused" [] {
    let text = "name = \"c6-lcd\"\nabout = \"a C6 board\"\n[firmware]\nchip = \"esp32c6\"\nflash_mb = 4\n"
    let b = board_from $text "/t/c6-lcd"
    assert equal [$b.name $b.about $b.target $b.boot_limit_mb $b.dir] ["c6-lcd" "a C6 board" "riscv32imac-esp-espidf" 0 "/t/c6-lcd"]
    assert error { board_from ($text | str replace "esp32c6" "esp32h2") "/t/h2" }
    let dir = mktemp -d
    mkdir ($dir | path join one) ($dir | path join notes)
    $text | save ($dir | path join one board.toml)
    assert equal (load_boards $dir | get name) ["c6-lcd"] "a directory without a board.toml is not a board"
    rm -rf $dir
}

def "test showcase: a slot past where the bootloader boots is refused" [] {
    let s3 = board_named "s3-amoled-206"
    check_bootable $s3 { name: "ota_0", offset: 0xa00000, size: 0x600000 }
    assert error { check_bootable $s3 { name: "ota_1", offset: 0x1000000, size: 0x600000 } }
    check_bootable (board_named "p4-7in") { name: "ota_1", offset: 0xe00000, size: 0x3f0000 }
}

def "test showcase: the espup exports become the environment, its PATH in front" [] {
    let text = "export LIBCLANG_PATH=\"/t/esp-clang/lib\"\nexport PATH=\"/t/xtensa/bin:$PATH\"\n"
    let vars = espup_env $text ["/usr/bin"]
    assert equal $vars.LIBCLANG_PATH "/t/esp-clang/lib"
    assert equal $vars.PATH ["/t/xtensa/bin" "/usr/bin"]
}

def "test showcase: a board builds with its toolchain, target, directory and feature" [] {
    let a = build_args (board_named "s3-amoled-206") "render-vello-cpu, fonts" "/t/s3"
    assert equal $a ["+esp" "build" "--release" "--target" "xtensa-esp32s3-espidf" "--target-dir" "/t/s3" "--features" "render-vello-cpu,fonts"]
    let p = build_args (board_named "p4-7in") "" "/t/p4"
    assert equal ($p | last) "/t/p4" "no features, and the board is not one"
    assert equal $p.0 "+nightly-2026-09-15"
}

def main [] { run-tests }
