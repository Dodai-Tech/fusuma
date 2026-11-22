#!/usr/bin/env nu
# Tests for scripts/forgejo_sync.nu

use std/assert
use runner.nu *
use ../forgejo_sync.nu [convert SKIP]

def "test forgejo_sync: paths, secret and bot are renamed" [] {
    let out = convert "token: ${{ secrets.GITEA_TOKEN || github.token }}\npaths: .gitea/workflows\nuser: gitea-actions[bot]"
    assert ($out | str contains "secrets.CODEBERG_TOKEN")
    assert ($out | str contains ".forgejo/workflows")
    assert ($out | str contains "forgejo-actions[bot]")
    assert not ($out | str contains "gitea")
}

def "test forgejo_sync: the github mirror is not copied" [] {
    assert ("mirror.yml" in $SKIP)
}

def main [] { run-tests }
