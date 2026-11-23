#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — the ESP32-P4 components showcase (examples/showcase-p4)
# ──────────────────────────────────────────────────────────────────────────────
# The firmware is not a workspace member (it needs the ESP toolchain), so its generated file, build
# and flashing live here, tested. Flashing is non-destructive: the image goes into an OTA slot and an
# `otadata` entry selects it; `restore` erases `otadata` so the board boots its factory app again.
#
#   nu scripts/showcase.nu gen-sdkconfig
#   nu scripts/showcase.nu build
#   nu scripts/showcase.nu install [--slot ota_0] [--port /dev/ttyACM0]
#   nu scripts/showcase.nu restore [--port P]
#   nu scripts/showcase.nu monitor [--port P]
# ──────────────────────────────────────────────────────────────────────────────

const ROOT = path self ..
const CRATE = "examples/showcase-p4"
const BIN = "fusuma-showcase-p4"

# The template with its @PARTITIONS@ placeholder filled in with an absolute path.
export def sdkconfig [template: string, partitions: string]: nothing -> string {
    $template | str replace --all "@PARTITIONS@" $partitions
}

# CRC-32 (IEEE) with the ROM's calling convention: `init` is the "previous result". The bootloader checks
# an otadata entry with init 0xFFFFFFFF; a wrong value makes it erase otadata and boot the factory app.
export def crc32 [data: binary, init: int = 0]: nothing -> int {
    mut crc = $init bit-xor 0xFFFFFFFF
    for i in 0..<($data | bytes length) {
        $crc = $crc bit-xor ($data | bytes at $i..<($i + 1) | into int)
        for _ in 0..<8 {
            $crc = if ($crc bit-and 1) == 1 { ($crc bit-shr 1) bit-xor 0xEDB88320 } else { $crc bit-shr 1 }
        }
    }
    $crc bit-xor 0xFFFFFFFF
}

# `n` as `width` little-endian bytes.
export def le_bytes [n: int, width: int]: nothing -> binary {
    0..<$width | each {|i| (($n bit-shr ($i * 8)) bit-and 0xFF) | into binary --compact | bytes at 0..<1 } | bytes collect
}

# The otadata entry that makes the bootloader boot OTA slot number `seq - 1` (32 bytes).
export def otadata_entry [seq: int]: nothing -> binary {
    if $seq < 1 { error make { msg: "otadata sequence starts at 1" } }
    let label = 0..<20 | each { 0x[ff] } | bytes collect
    [(le_bytes $seq 4) $label (le_bytes 0xFFFFFFFF 4) (le_bytes (crc32 (le_bytes $seq 4) 0xFFFFFFFF) 4)] | bytes collect
}

# The otadata sequence that selects `ota_N`.
export def slot_seq [slot: string]: nothing -> int {
    let m = $slot | parse --regex '^ota_(?<n>\d+)$'
    if ($m | is-empty) { error make { msg: $"'($slot)' is not an OTA slot like ota_0" } }
    ($m.0.n | into int) + 1
}

# Little-endian unsigned integer from `n` bytes at `at`.
export def le [bytes: binary, at: int, n: int]: nothing -> int {
    $bytes | bytes at $at..<($at + $n) | into int --endian little
}

# Partition table entries from the raw table bytes (0x8000 in flash, 32 B per entry).
export def parse_ptable [bytes: binary]: nothing -> table {
    0..(($bytes | bytes length) // 32 - 1)
    | each {|i|
        let e = $bytes | bytes at ($i * 32)..<($i * 32 + 32)
        # `each` drops a null, so the end of the table is marked with a record instead.
        if ($e | bytes at 0..<2) != 0x[aa 50] { { end: true } } else {
            {
                end: false
                name: ($e | bytes at 12..<28 | bytes split 0x[00] | first | decode utf-8)
                offset: (le $e 4 4)
                size: (le $e 8 4)
            }
        }
    }
    | take while {|r| not $r.end }
    | reject end
}

# The partition called `name`; error listing the known ones.
export def partition [table: table, name: string]: nothing -> record {
    let r = $table | where name == $name
    if ($r | is-empty) { error make { msg: $"no partition '($name)' on the board; it has: ($table.name | str join ', ')" } }
    $r | first
}

# Error unless an image of `bytes` fits in `target`.
export def check_fits [target: record, bytes: int] {
    if $bytes > $target.size {
        error make { msg: $"the image is ($bytes) bytes but ($target.name) holds ($target.size); build with --release" }
    }
}

# espflash's port arguments (`port` empty = let espflash pick).
export def port_args [port: string]: nothing -> list<string> {
    if ($port | is-empty) { [] } else { ["--port" $port] }
}

# The environment an ESP build needs: a clean PATH, so nothing from other toolchains leaks in.
def esp_env [home: string]: nothing -> record {
    { HOME: $home, PATH: ["/usr/bin" ($home | path join .cargo bin) "/usr/local/bin"] }
}

def elf_path []: nothing -> path {
    $ROOT | path join $CRATE target riscv32imafc-esp-espidf release $BIN
}

def "main gen-sdkconfig" [] {
    let dir = $ROOT | path join $CRATE
    let csv = $dir | path join partitions.csv | path expand
    sdkconfig (open --raw ($dir | path join sdkconfig.defaults.in)) $csv | save -f ($dir | path join sdkconfig.defaults)
    print -e $"Generated sdkconfig.defaults \(partitions: ($csv))"
}

def "main build" [] {
    main gen-sdkconfig
    cd ($ROOT | path join $CRATE)
    with-env (esp_env $nu.home-dir) { ^cargo build --release }
}

def "main install" [--slot: string = "ota_0", --port: string = ""] {
    let seq = slot_seq $slot
    main build
    let image = mktemp --suffix .bin
    ^espflash save-image --chip esp32p4 --flash-size 32mb (elf_path) $image
    let table_file = mktemp --suffix .bin
    ^espflash read-flash ...(port_args $port) 0x8000 0xc00 $table_file
    let table = parse_ptable (open --raw $table_file | into binary)
    let target = partition $table $slot
    let otadata = partition $table otadata
    check_fits $target (ls $image | get size.0 | into int)
    let entry = mktemp --suffix .bin
    otadata_entry $seq | save -f $entry
    ^espflash write-bin ...(port_args $port) $target.offset $image
    ^espflash write-bin ...(port_args $port) $otadata.offset $entry
    ^espflash reset ...(port_args $port)
    print -e $"Showcase installed in ($slot); `nu scripts/showcase.nu restore` boots the factory app again."
}

def "main restore" [--port: string = ""] {
    let table_file = mktemp --suffix .bin
    ^espflash read-flash ...(port_args $port) 0x8000 0xc00 $table_file
    let otadata = partition (parse_ptable (open --raw $table_file | into binary)) otadata
    ^espflash erase-region ...(port_args $port) $otadata.offset $otadata.size
    ^espflash reset ...(port_args $port)
    print -e "Factory app selected."
}

def "main monitor" [--port: string = ""] {
    ^espflash monitor ...(port_args $port)
}

def main [] { print -e "usage: nu scripts/showcase.nu <gen-sdkconfig|build|install|restore|monitor>" }
