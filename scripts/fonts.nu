# Fusuma — fetch the font families in crates/fusuma-fonts/fonts/catalog.toml
# ──────────────────────────────────────────────────────────────────────────────
# Downloads each family, cuts static Regular and Bold instances from a variable font (fonttools varLib.instancer),
# subsets them to the catalog's coverage (pyftsubset) and writes `fonts/<id>/regular.ttf`, `bold.ttf` (sans only)
# and its licence (`OFL.txt`, or `LICENSE.txt` from `licence_url`). An icon family is cut to its `icons` names
# instead, and `names.txt` lists them with their characters. The results are committed, so a build needs no network. Needs `fonttools` (pip: fonttools).
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

# The name table of an icon font as rows `name`, `hex`, from the text its `codepoints_url` serves, in its `format`:
# `codepoints` (`name hex` a line, Material), `lucide` (lucide-static's info.json, `encodedCode` per icon) or `css`
# (a stylesheet with `.ph-name:before { content: "\e900" }` rules, Phosphor).
export def parse_codepoints [text: string, format: string = "codepoints"]: nothing -> table<name: string, hex: string> {
    match $format {
        "codepoints" => { $text | lines | where { |l| $l | is-not-empty } | parse "{name} {hex}" }
        "lucide" => {
            $text | from json | transpose name info | each { |r|
                { name: $r.name, hex: ($r.info.encodedCode | str replace --regex '^\\' '' | str lowercase) }
            }
        }
        "css" => {
            $text | parse --regex '\.ph-(?P<name>[a-z0-9-]+):before\s*\{\s*content:\s*"\\(?P<hex>[0-9a-fA-F]+)"' | each { |r| { name: $r.name, hex: ($r.hex | str lowercase) } }
        }
        _ => { error make { msg: $"unknown codepoints format '($format)'; codepoints, lucide or css" } }
    }
}

# An icon family's names with their codepoints (hex), in the catalog's order. An entry is a name the font has, or
# `logical=actual`: the app asks for `logical` (a Material name such as `home`) and the font calls it `actual` (`house`).
# Error naming the first name the font does not have.
export def icon_codes [names: list<string>, codepoints: string, format: string = "codepoints"]: nothing -> list<record> {
    let table = parse_codepoints $codepoints $format
    $names | each { |entry|
        let parts = $entry | split row "="
        let logical = $parts | first
        let actual = if ($parts | length) > 1 { $parts | get 1 } else { $logical }
        let hit = $table | where name == $actual
        if ($hit | is-empty) { error make { msg: $"icon '($actual)' is not in the font" } }
        { name: $logical, hex: ($hit | first | get hex) }
    }
}

# The coverage that cuts an icon font to `codes` (from `icon_codes`).
export def icon_coverage [codes: list<record>]: nothing -> string {
    $codes | get hex | uniq | each { |h| $"U+($h)" } | str join ", "
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

# The `fonttools ttx` arguments that write a font as XML without the tables a family lists in `drop_tables` (a font whose
# GSUB fonttools cannot read is cut by dropping it first; an icon font is addressed by character and needs none).
export def ttx_dump_args [input: string, output: string, tables: list<string>]: nothing -> list<string> {
    ["ttx" "-x" ($tables | str join ",") "-o" $output $input]
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
    let coverage = if $family.role == "icons" {
        let codes = icon_codes $family.icons (http get --raw $family.codepoints_url | into string) ($family.codepoints_format? | default "codepoints")
        $codes | each { |c| $"($c.name) ($c.hex)" } | str join "\n" | $in + "\n" | save -f ($dir | path join names.txt)
        icon_coverage $codes
    } else { $coverage }
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
        let raw = if ($family.drop_tables? | is-not-empty) {
            let xml = $tmp | path join $"($w).ttx"
            let stripped = $tmp | path join $"($w)-stripped.ttf"
            ^fonttools ...(ttx_dump_args $raw $xml $family.drop_tables)
            ^fonttools ttx -o $stripped $xml
            $stripped
        } else { $raw }
        # An icon font is addressed by character, so it keeps no layout features (some have ligature tables fonttools cannot cut).
        let layout = if $family.role == "icons" { "" } else { "*" }
        ^pyftsubset ...(subset_args $raw ($dir | path join $"($w).ttf") $coverage $layout)
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
