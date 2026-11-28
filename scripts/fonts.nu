# Fusuma — fetch the font families in crates/fusuma-fonts/fonts/catalog.toml
# ──────────────────────────────────────────────────────────────────────────────
# Downloads each family, cuts static Regular and Bold instances from a variable font (fonttools varLib.instancer),
# subsets them to the catalog's coverage (pyftsubset) and writes `fonts/<id>/regular.ttf`, `bold.ttf` (sans only)
# and its licence (`OFL.txt`, or `LICENSE.txt` from `licence_url`). The results are committed, so a build needs no
# network. Needs `fonttools` (pip: fonttools). Icons are not here: `scripts/icons.nu` fetches those.
#
#   nu scripts/fonts.nu fetch                     # every family not fetched yet
#   nu scripts/fonts.nu fetch --family roboto --force
#   nu scripts/fonts.nu list
# ──────────────────────────────────────────────────────────────────────────────

const FONTS = path self ../crates/fusuma-fonts/fonts

# The catalog, read from its file.
export def catalog [file: string = ($FONTS | path join catalog.toml)]: nothing -> record {
    open $file
}

# The weights a family is made of: `regular`, and `bold` for a sans (a mono has one weight).
export def weights [family: record]: nothing -> list<string> {
    if $family.role == "sans" { ["regular" "bold"] } else { ["regular"] }
}

# How one weight of a family is obtained: from a zip (`member` is the file in it), from a variable font (instanced
# at `wght` with the family's other axes), or a static file (its own url, the bold one from `bold_url`).
export def origin [family: record, weight: string]: nothing -> record {
    let url = if $weight == "bold" and ($family.bold_url? != null) { $family.bold_url } else { $family.url }
    let wght = if $weight == "bold" { 700 } else { 400 }
    if ($url | str ends-with ".zip") {
        let member = $family | get -o $weight
        if $member == null { error make { msg: $"($family.id): a zip source needs `($weight)`, the file name in the zip" } }
        { kind: "zip", url: $url, member: $member }
    } else if ($url | str contains "%5B") {
        { kind: "variable", url: $url, axes: (($family.axes? | default {}) | merge { wght: $wght }) }
    } else if $weight == "bold" and ($family.bold_url? == null) {
        error make { msg: $"($family.id): a static sans needs `bold_url` for its bold weight" }
    } else {
        { kind: "static", url: $url }
    }
}

# The `fonttools varLib.instancer` arguments that pin every axis of a variable font.
export def instance_args [input: string, output: string, axes: record]: nothing -> list<string> {
    let pins = $axes | transpose k v | sort-by k | each { |a| $"($a.k)=($a.v)" }
    ["varLib.instancer" $input ...$pins "--static" "--update-name-table" "-o" $output]
}

# The `pyftsubset` arguments that cut a font to `coverage` ("U+0020-007E, U+00A0-017F, ...").
export def subset_args [input: string, output: string, coverage: string, layout: string = "*"]: nothing -> list<string> {
    let unicodes = $coverage | str replace --all " " ""
    [$input $"--unicodes=($unicodes)" $"--layout-features=($layout)" "--drop-tables+=DSIG" $"--output-file=($output)"]
}

# Where the licence of a family is next to its font file.
export def licence_url [family: record]: nothing -> string {
    if ($family.licence_url? != null) {
        $family.licence_url
    } else if ($family.url | str contains "github.com/google/fonts/raw/main/") {
        ($family.url | path dirname) + "/OFL.txt"
    } else { "" }
}

def fetch_one [family: record, coverage: string, force: bool] {
    let dir = $FONTS | path join $family.id
    let wanted = weights $family | each { |w| $dir | path join $"($w).ttf" }
    if not $force and ($wanted | all { |f| $f | path exists }) {
        print -e $"($family.id): already there"
        return
    }
    mkdir $dir
    let tmp = mktemp -d
    for w in (weights $family) {
        let src = origin $family $w
        let raw = $tmp | path join $"($w)-raw.ttf"
        match $src.kind {
            "zip" => {
                let zip = $tmp | path join "font.zip"
                if not ($zip | path exists) { http get --raw $src.url | save -f $zip }
                let path = ^unzip -Z1 $zip | lines | where { |l| ($l | path basename) == $src.member } | first
                ^unzip -p $zip $path | save -f $raw
            }
            "variable" => {
                let var = $tmp | path join "variable.ttf"
                if not ($var | path exists) { http get --raw $src.url | save -f $var }
                ^fonttools ...(instance_args $var $raw $src.axes)
            }
            _ => { http get --raw $src.url | save -f $raw }
        }
        ^pyftsubset ...(subset_args $raw ($dir | path join $"($w).ttf") $coverage)
    }
    let lic = licence_url $family
    let lic_file = if ($family.licence_url? != null) { "LICENSE.txt" } else { "OFL.txt" }
    if ($lic | is-not-empty) {
        http get --raw $lic | save -f ($dir | path join $lic_file)
    } else if not ($dir | path join OFL.txt | path exists) {
        error make { msg: $"($family.id): no OFL.txt next to its fonts; add it by hand" }
    }
    rm -rf $tmp
    print -e $"($family.id): fetched \(($wanted | each { |f| ls $f | get size.0 } | math sum))"
}

def "main fetch" [--family: string = "", --force] {
    let c = catalog
    let chosen = $c.family | where { |f| ($family | is-empty) or $f.id == $family }
    if ($chosen | is-empty) { error make { msg: $"no family '($family)' in the catalog" } }
    for f in $chosen { fetch_one $f $c.defaults.coverage $force }
}

def "main list" [] {
    let c = catalog
    $c.family | select id role name about | print
    $c.preset | select id sans mono about | print
}

def main [] { print -e "usage: nu scripts/fonts.nu <fetch|list>" }
