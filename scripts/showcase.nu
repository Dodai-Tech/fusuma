#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — the components showcase on ESP32 boards (examples/showcase-esp)
# ──────────────────────────────────────────────────────────────────────────────
# One firmware crate, one build per board: a board is a directory under crates/fusuma-platform-esp/boards with its
# board file (`board.toml`), sdkconfig and partition table; the chip gives the Rust target and toolchain (`CHIPS`).
# `install` without `--board` asks the connected chip what it is and picks the board for it.
# The firmware is not a workspace member (it needs the ESP toolchain), so its generated file, build
# and flashing live here, tested. Flashing is non-destructive: the image goes into an OTA slot and an
# `otadata` entry selects it; `restore` erases `otadata` so the board boots its factory app again.
#
#   nu scripts/showcase.nu detect  [--port P]
#   nu scripts/showcase.nu build   --board B [--features render-vello-cpu]
#   nu scripts/showcase.nu install [--board B] [--slot ota_0] [--port /dev/ttyACM0] [--features render-vello-cpu]
#   nu scripts/showcase.nu restore [--port P]
#   nu scripts/showcase.nu monitor [--port P]
# ──────────────────────────────────────────────────────────────────────────────

const ROOT = path self ..
const CRATE = "examples/showcase-esp"
const BIN = "fusuma-showcase-esp"

# Where the board files are: one directory per board, its `board.toml` the one description of it (see
# crates/fusuma-platform/board.schema.json), with its `sdkconfig.defaults.in` and `partitions.csv` next to it.
export const BOARDS_DIR = path self ../crates/fusuma-platform-esp/boards

# What an ESP-IDF build needs for each chip: the Rust target and the toolchain (`espup` means it comes from espup and
# needs its environment, `~/export-esp.sh`). The chip is the board's; nothing here is a board.
export const CHIPS = {
    esp32p4: { target: "riscv32imafc-esp-espidf", toolchain: "nightly-2026-09-15", espup: false }
    esp32s3: { target: "xtensa-esp32s3-espidf", toolchain: "esp", espup: true }
    esp32c6: { target: "riscv32imac-esp-espidf", toolchain: "nightly-2026-09-15", espup: false }
}

# A board from its board file's text, with what the chip adds; `dir` is where its files are.
export def board_from [text: string, dir: string]: nothing -> record {
    let f = $text | from toml
    let chip = $f.firmware.chip
    let c = $CHIPS | get -o $chip
    if $c == null { error make { msg: $"($dir): no ESP-IDF target known for chip ($chip); known: ($CHIPS | columns | str join ', ')" } }
    {
        name: $f.name
        about: ($f.about? | default $f.name)
        chip: $chip
        flash_mb: $f.firmware.flash_mb
        boot_limit_mb: ($f.firmware.boot_limit_mb? | default 0)
        target: $c.target
        toolchain: $c.toolchain
        espup: $c.espup
        dir: $dir
    }
}

# Every board under `dir`, sorted by name.
export def load_boards [dir: string = $BOARDS_DIR]: nothing -> list<record> {
    ls $dir | where type == dir | get name | sort | each { |d|
        let file = $d | path join board.toml
        if ($file | path exists) { board_from (open --raw $file) $d }
    } | compact
}

# The board called `name`; error listing the known ones.
export def board_named [name: string, boards?: list]: nothing -> record {
    let boards = $boards | default (load_boards)
    let b = $boards | where name == $name
    if ($b | is-empty) { error make { msg: $"no board '($name)'; known: ($boards.name | str join ', ')" } }
    $b | first
}

# The chip and flash size in `espflash board-info`'s output.
export def parse_board_info [text: string]: nothing -> record {
    let chip = $text | parse --regex 'Chip type:\s+(?<chip>esp32[a-z0-9]*)' | get chip.0? | default ""
    let flash = $text | parse --regex 'Flash size:\s+(?<mb>\d+)MB' | get mb.0? | default "0" | into int
    if ($chip | is-empty) { error make { msg: "espflash board-info did not say which chip this is" } }
    { chip: $chip, flash_mb: $flash }
}

# The board for a connected chip: the only one with its chip (and, when several share it, its flash size). Error when
# none or several fit, naming them, so `--board` can settle it.
export def pick_board [info: record, boards?: list]: nothing -> record {
    let boards = $boards | default (load_boards)
    let same_chip = $boards | where chip == $info.chip
    if ($same_chip | is-empty) {
        error make { msg: $"no showcase board has an ($info.chip); known: ($boards | each { |b| $'($b.name) [($b.chip)]' } | str join ', ')" }
    }
    let fit = if ($same_chip | length) == 1 { $same_chip } else { $same_chip | where flash_mb == $info.flash_mb }
    if ($fit | length) != 1 {
        error make { msg: $"several boards have an ($info.chip): ($same_chip.name | str join ', '); choose one with --board" }
    }
    $fit | first
}

# Error unless the slot lies where the board's bootloader boots an app from.
export def check_bootable [board: record, target: record] {
    let limit = $board.boot_limit_mb * 1024 * 1024
    if $limit > 0 and ($target.offset + $target.size) > $limit {
        error make { msg: $"($target.name) ends past ($board.boot_limit_mb) MB, where ($board.name)'s bootloader does not boot; use another slot" }
    }
}

# The variables an espup toolchain needs, from the `export K="V"` lines of `export-esp.sh` (PATH entries are put in
# front of `path`).
export def espup_env [text: string, path: list<string>]: nothing -> record {
    let pairs = $text | lines | parse --regex '^export (?<k>[A-Z_]+)="(?<v>[^"]*)"'
    let extra = $pairs | where k == PATH | get v | each { |v| $v | str replace ':$PATH' '' }
    $pairs | where k != PATH | reduce --fold { PATH: ($extra | append $path) } { |p, acc| $acc | insert $p.k $p.v }
}

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

# The environment an ESP build for `board` needs: a clean PATH, so nothing from other toolchains leaks in, the chip
# and the board's sdkconfig, and espup's variables for a toolchain from espup.
def esp_env [home: string, board: record]: nothing -> record {
    let path = ["/usr/bin" ($home | path join .cargo bin) "/usr/local/bin"]
    let base = if $board.espup {
        let file = $home | path join export-esp.sh
        if not ($file | path exists) { error make { msg: $"($board.name) needs the esp toolchain: run `espup install` \(no ($file))" } }
        espup_env (open --raw $file) $path
    } else { { PATH: $path } }
    $base | merge {
        HOME: $home
        MCU: $board.chip
        ESP_IDF_SDKCONFIG_DEFAULTS: (generated_sdkconfig $board)
        FUSUMA_BOARD: $board.name
        # The firmware's settings: fusuma-fonts reads `[fonts] sizes` from it to make the glyph atlases.
        FUSUMA_SETTINGS: ($ROOT | path join $CRATE fusuma.toml)
    }
}

# The board's sdkconfig with its partition table filled in, generated next to its build (never into the board's
# directory, which stays only what a person wrote).
def generated_sdkconfig [board: record]: nothing -> path {
    target_dir $board | path join sdkconfig.defaults
}

# Each board builds into its own directory, so two boards with the same chip never share an ESP-IDF build.
def target_dir [board: record]: nothing -> path {
    $ROOT | path join $CRATE target $board.name
}

def elf_path [board: record]: nothing -> path {
    target_dir $board | path join $board.target release $BIN
}

def "main gen-sdkconfig" [--board: string = "p4-7in"] {
    let b = board_named $board
    let csv = $b.dir | path join partitions.csv | path expand
    let out = generated_sdkconfig $b
    mkdir ($out | path dirname)
    sdkconfig (open --raw ($b.dir | path join sdkconfig.defaults.in)) $csv | save -f $out
    print -e $"Generated ($out) \(partitions: ($csv))"
}

# What is connected on `port`: the chip and the board the showcase would build for it.
def detect [port: string]: nothing -> record {
    let out = ^espflash board-info ...(port_args $port) | complete
    if $out.exit_code != 0 { error make { msg: $"espflash board-info failed: ($out.stderr | str trim)" } }
    pick_board (parse_board_info $out.stdout)
}

def "main detect" [--port: string = ""] {
    let b = detect $port
    print $"($b.name): ($b.about) \(($b.chip), ($b.target))"
}

# The cargo arguments for a comma or space separated list of features: `[--features a,b]`, or nothing.
export def feature_args [features: string]: nothing -> list<string> {
    let list = $features | split row --regex '[,\s]+' | where { |f| $f | is-not-empty }
    if ($list | is-empty) { [] } else { ["--features" ($list | str join ",")] }
}

# The cargo arguments that build `board` with the extra `features`.
export def build_args [board: record, features: string, target_dir: string]: nothing -> list<string> {
    # The board itself is not a feature: FUSUMA_BOARD names it (see `esp_env`).
    [$"+($board.toolchain)" "build" "--release" "--target" $board.target "--target-dir" $target_dir ...(feature_args $features)]
}

def "main build" [--board: string, --features: string = ""] {
    let b = board_named $board
    main gen-sdkconfig --board $board
    cd ($ROOT | path join $CRATE)
    with-env (esp_env $nu.home-dir $b) { ^cargo ...(build_args $b $features (target_dir $b)) }
}

# Where a backup of `board`'s whole flash goes: ~/.fusuma/backups/<board>-<stamp>.bin (`stamp` like 20261126-0130).
export def backup_path [board: record, stamp: string, home: string]: nothing -> path {
    $home | path join ".fusuma" "backups" $"($board.name)-($stamp).bin"
}

# The espflash arguments that read the whole flash of `board` into `file`.
export def backup_args [board: record, port: string, file: string]: nothing -> list<string> {
    ["read-flash" ...(port_args $port) "0" $"(($board.flash_mb | into int) * 1048576)" $file]
}

# The arguments that write a backup `file` back over the whole flash.
export def restore_args [port: string, file: string]: nothing -> list<string> {
    ["write-bin" ...(port_args $port) "0" $file]
}

# Reads the whole flash of the connected board (the factory firmware, its partitions, its settings) into a file with a
# checksum next to it, before anything is written. A board that ships with firmware you want back (the Sticky's, the
# watch's) is backed up once, here.
def "main backup" [--board: string = "", --port: string = "", --out: string = ""] {
    let b = if ($board | is-empty) { detect $port } else { board_named $board }
    let file = if ($out | is-empty) { backup_path $b (date now | format date "%Y%m%d-%H%M") $nu.home-dir } else { $out }
    mkdir ($file | path dirname)
    print -e $"Reading ($b.flash_mb) MB of ($b.name) into ($file) ..."
    ^espflash ...(backup_args $b $port $file)
    let sum = open --raw $file | hash sha256
    $"($sum)  ($file | path basename)\n" | save -f $"($file).sha256"
    print -e $"Backup done: ($file) \(sha256 ($sum | str substring 0..11)...). `nu scripts/showcase.nu restore-flash --file ($file)` writes it back."
}

# Writes a backup made by `backup` over the whole flash again (the board is back as it was shipped), after checking the
# checksum next to the file.
def "main restore-flash" [--file: string, --port: string = ""] {
    let sumfile = $"($file).sha256"
    if ($sumfile | path exists) {
        let want = open $sumfile | split row " " | first
        let got = open --raw $file | hash sha256
        if $want != $got { error make { msg: $"($file) does not match its checksum: the backup is damaged" } }
    } else {
        print -e $"No checksum next to ($file): writing it unchecked."
    }
    ^espflash ...(restore_args $port $file)
    ^espflash reset ...(port_args $port)
    print -e "Whole flash restored."
}

def "main install" [--board: string = "", --slot: string = "ota_0", --port: string = "", --features: string = "", --backup] {
    if $backup { main backup --board $board --port $port }
    let seq = slot_seq $slot
    let b = if ($board | is-empty) { detect $port } else { board_named $board }
    print -e $"Board: ($b.name), ($b.about)"
    main build --board $b.name --features $features
    let image = mktemp --suffix .bin
    ^espflash save-image --chip $b.chip --flash-size $"($b.flash_mb)mb" (elf_path $b) $image
    let table_file = mktemp --suffix .bin
    ^espflash read-flash ...(port_args $port) 0x8000 0xc00 $table_file
    let table = parse_ptable (open --raw $table_file | into binary)
    let target = partition $table $slot
    let otadata = partition $table otadata
    check_fits $target (ls $image | get size.0 | into int)
    check_bootable $b $target
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

def main [] { print -e "usage: nu scripts/showcase.nu <detect|gen-sdkconfig|build|backup|restore-flash|install|restore|monitor>" }
