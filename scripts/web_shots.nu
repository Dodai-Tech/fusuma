#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — screenshot tests of the web showcase
# ──────────────────────────────────────────────────────────────────────────────
# Builds the web showcase, loads each page in a headless Chromium (`?renderer=cpu&still=1&page=..&dark=..`) and
# compares the screenshot with the picture the native Vello CPU renderer draws for the same start
# (`FUSUMA_WEB_REFERENCE`, see `examples/showcase`). Both run the same code, so the pictures are expected to be the
# same pixel for pixel; `--tolerance` is how many pixels (as a fraction of the screen) may differ (default 0.1 %).
#
#   nu scripts/web_shots.nu                    # build, shoot, compare; exit 1 on a difference
#   nu scripts/web_shots.nu --no-build         # use the pkg that is there
#   nu scripts/web_shots.nu --update           # write the screenshots over nothing: only list what would be shot
#
# Needs a Chromium-family browser (chromium, google-chrome, brave) and ImageMagick (`magick`, or `compare` of version 6). Pictures, references and
# differences are kept in `target/web-shots/` for CI to upload.
# ──────────────────────────────────────────────────────────────────────────────

const ROOT = path self ..
export const OUT = "target/web-shots"
export const WIDTH = 1024
export const HEIGHT = 600
export const PORT = 18741

# The browsers to look for, in order.
export const BROWSERS = ["chromium" "chromium-browser" "google-chrome" "google-chrome-stable" "brave" "microsoft-edge"]

# Every picture: its name, and the query that asks the page for it.
export def shots [pages: list<string>]: nothing -> list<record<name: string, query: string>> {
    let looks = [{ mode: "dark", dark: 1 } { mode: "light", dark: 0 }]
    let pictures = $looks | each { |l|
        $pages | each { |p| { name: $"($l.mode)-($p)", query: $"renderer=cpu&still=1&page=($p)&dark=($l.dark)" } }
    } | flatten
    $pictures | append { name: "dark-menu", query: "renderer=cpu&still=1&menu=1" }
}

# The browser's arguments to take one screenshot of `url` into `file`.
export def browser_args [url: string, file: string]: nothing -> list<string> {
    [
        "--headless=new" "--disable-gpu" "--no-sandbox" "--hide-scrollbars" "--force-device-scale-factor=1"
        $"--screenshot=($file)" $"--window-size=($WIDTH),($HEIGHT)" "--virtual-time-budget=8000" $url
    ]
}

# ImageMagick's command to compare: `magick compare` (version 7), or `compare` (version 6, which has no `magick`).
export def compare_command [has_magick: bool]: nothing -> list<string> {
    if $has_magick { ["magick" "compare"] } else { ["compare"] }
}

# The arguments after that command: count the pixels that differ between two pictures by more than a small colour
# tolerance, writing the difference to `diff`.
export def compare_args [a: string, b: string, diff: string]: nothing -> list<string> {
    ["-metric" "AE" "-fuzz" "2%" $a $b $diff]
}

# How many pixels differ, from what `magick compare -metric AE` prints (a count, maybe followed by `(fraction)`).
export def differing [printed: string]: nothing -> int {
    $printed | str trim | split row " " | first | into float | math round | into int
}

# Whether `count` differing pixels is acceptable at `tolerance` (a fraction of the picture).
export def acceptable [count: int, tolerance: float]: nothing -> bool {
    $count <= ($WIDTH * $HEIGHT * $tolerance | math floor)
}

# The first of `candidates` that `exists` says is installed, or null.
export def first_installed [candidates: list<string>, exists: closure]: nothing -> any {
    $candidates | where { |c| do $exists $c } | first | default null
}

def installed [name: string]: nothing -> bool { (which $name | is-not-empty) }

def main [
    --no-build        # use the pkg that is there
    --tolerance: float = 0.001
] {
    cd $ROOT
    let browser = first_installed $BROWSERS { |c| installed $c }
    if $browser == null { error make { msg: $"no browser: install one of ($BROWSERS | str join ', ')" } }
    let has_magick = installed "magick"
    if not ($has_magick or (installed "compare")) { error make { msg: "ImageMagick is missing: `magick` or `compare` must be on the PATH" } }
    let cmp = compare_command $has_magick
    if not $no_build { nu scripts/web.nu build }

    mkdir $OUT
    let refs = $"($OUT)/reference"
    mkdir $refs
    print -e "reference pictures (native Vello CPU)"
    with-env { FUSUMA_WEB_REFERENCE: ($refs | path expand) } {
        ^cargo test -p fusuma-showcase web_reference
    }

    let pages = ["Buttons" "Selection" "List" "Progress" "Controls" "Text" "Typography" "Navigation" "Reading"]
    let server = job spawn { ^python3 -m http.server $PORT --directory examples/showcase-web out+err> /dev/null }
    sleep 1sec
    mut failed = []
    for s in (shots $pages) {
        let file = $"($OUT)/($s.name).png"
        let url = $"http://localhost:($PORT)/index.html?($s.query)"
        ^$browser ...(browser_args $url ($file | path expand)) out+err> /dev/null
        let diff = $"($OUT)/($s.name)-diff.png"
        # `compare` exits 1 when the pictures differ, and prints the count on stderr.
        let r = ^($cmp | first) ...($cmp | skip 1) ...(compare_args $file $"($refs)/($s.name).png" $diff) | complete
        let count = differing $r.stderr
        let ok = acceptable $count $tolerance
        print -e $"(if $ok { '  ok  ' } else { ' FAIL ' }) ($s.name): ($count) pixels differ"
        if $ok { rm -f $diff } else { $failed = ($failed | append $s.name) }
    }
    job kill $server
    if ($failed | is-not-empty) {
        print -e $"failed: ($failed | str join ', ') — pictures and differences are in ($OUT)"
        exit 1
    }
    print -e $"web screenshots ok: (shots $pages | length) pictures"
}
