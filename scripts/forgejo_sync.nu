#!/usr/bin/env nu
# ──────────────────────────────────────────────────────────────────────────────
# Fusuma — Derive the Codeberg (Forgejo) workflows from the Gitea ones
# ──────────────────────────────────────────────────────────────────────────────
# Codeberg runs Forgejo Actions, which reads .forgejo/workflows/ first. Those files
# are the .gitea/workflows/ ones with Forgejo names; .gitea/ stays the source.
# `mirror.yml` (Gitea fast-forwards from GitHub) is skipped: Codeberg is pushed to.
# Re-run after editing .gitea/workflows/. The runner labels (`runs-on`) are kept:
# change them if Codeberg's runners use another label.
#
# Usage: nu scripts/forgejo_sync.nu
# ──────────────────────────────────────────────────────────────────────────────

export const SKIP = ["mirror.yml"]

# One workflow's text with Forgejo names, secret and bot identity.
export def convert [text: string] {
    $text
    | str replace --all ".gitea/workflows" ".forgejo/workflows"
    | str replace --all "secrets.GITEA_TOKEN" "secrets.CODEBERG_TOKEN"
    | str replace --all "gitea-actions[bot]@noreply.gitea" "forgejo-actions[bot]@noreply.codeberg.org"
    | str replace --all "gitea-actions[bot]" "forgejo-actions[bot]"
    | str replace --all "Gitea" "Forgejo"
}

def main [] {
    let root = $env.FILE_PWD | path dirname
    let src = $root | path join .gitea workflows
    let dst = $root | path join .forgejo workflows
    mkdir $dst
    for f in (glob $"($src)/*.yml" | where {|f| ($f | path basename) not-in $SKIP }) {
        open --raw $f | decode utf-8 | convert $in | save --force --raw ($dst | path join ($f | path basename))
        print $"→ .forgejo/workflows/($f | path basename)"
    }
}
