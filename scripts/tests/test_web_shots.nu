#!/usr/bin/env nu
# ── Fusuma · test_web_shots.nu ────────────────────────────────────
# The screenshot script's pure parts: the matrix of pictures, the arguments, the verdict.

use std/assert
use runner.nu *
use ../web_shots.nu [shots browser_args compare_args compare_command differing acceptable first_installed WIDTH HEIGHT]

def "test web shots: every page in both modes, and the open dropdown" [] {
    let s = shots ["Buttons" "Text"]
    assert equal ($s | get name) ["dark-Buttons" "dark-Text" "light-Buttons" "light-Text" "dark-menu"]
    assert equal ($s | first | get query) "renderer=cpu&still=1&page=Buttons&dark=1"
    assert equal ($s | get 2 | get query) "renderer=cpu&still=1&page=Buttons&dark=0"
    assert ($s | last | get query | str contains "menu=1")
}

def "test web shots: the browser is headless, the size is the references and the picture goes where asked" [] {
    let a = browser_args "http://x/" "/tmp/o.png"
    assert ("--headless=new" in $a)
    assert ($"--window-size=($WIDTH),($HEIGHT)" in $a)
    assert ("--screenshot=/tmp/o.png" in $a)
    assert equal ($a | last) "http://x/"
    assert ("--force-device-scale-factor=1" in $a)
}

def "test web shots: the comparison counts pixels and writes the difference" [] {
    let a = compare_args "a.png" "b.png" "d.png"
    assert equal ($a | last) "d.png"
    assert ("AE" in $a)
    assert equal (compare_command true) ["magick" "compare"]
    assert equal (compare_command false) ["compare"]
}

def "test web shots: the count is read from what compare prints" [] {
    assert equal (differing "0") 0
    assert equal (differing "8500.29 (0.0138351)") 8500
    assert equal (differing " 12 (2e-05)\n") 12
}

def "test web shots: a few differing pixels are acceptable and a page of them is not" [] {
    assert (acceptable 0 0.001)
    assert (acceptable 600 0.001)
    assert (not (acceptable 8500 0.001))
    assert (acceptable 8500 0.02)
}

def "test web shots: the first installed browser is chosen, and none is null" [] {
    assert equal (first_installed ["a" "b" "c"] { |c| $c in ["b" "c"] }) "b"
    assert equal (first_installed ["a"] { |c| false }) null
}

def main [] { run-tests }
