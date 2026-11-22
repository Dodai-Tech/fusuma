# Changelog

All notable changes to this project are documented here. It is generated from
[Conventional Commits](https://www.conventionalcommits.org) by
[git-cliff](https://git-cliff.org) — do not edit it by hand.

## [0.2.0](https://github.com/Dodai-Tech/fusuma/compare/v0.1.0...v0.2.0) — 2026-11-22

### ✨ Features

- Fusuma-render-vello (GPU backend on vello_gpu and wgpu), image upload hook, GPU-vs-CPU benchmark; publish list with the new crates ([`a9d6fde`](https://github.com/Dodai-Tech/fusuma/commit/a9d6fdef4218e0b8a59823e426604f0f80c3aa5a))
- Fusuma-render-vello-cpu and -vello-common (Vello backends), fusuma-anim (easing, tween, keyframes, spring, decay); drop the tiny-skia backend; roadmap and docs ([`fc0641c`](https://github.com/Dodai-Tech/fusuma/commit/fc0641cafe9f2bbc61587ed73b403e017b8cb6fb))

### 🔧 Chores

- Drop the unused png workspace dependency ([`993ba42`](https://github.com/Dodai-Tech/fusuma/commit/993ba42954709214e2e851a2a10148a2b11ced46))
- Drop the unused workspace dependency entry for fusuma-anim ([`ca5b30a`](https://github.com/Dodai-Tech/fusuma/commit/ca5b30a4b991e0c869c414d72fcbd968936ce0e9))

## [0.1.0](https://github.com/Dodai-Tech/fusuma/releases/tag/v0.1.0) — 2026-11-22

### ✨ Features

- Tokonoma renderer trait, embedded (no_std) and tiny-skia backends, conformance suite and fuzzing ([`f03ce48`](https://github.com/Dodai-Tech/fusuma/commit/f03ce48d596639cdbf87369224d130a97bc39279))

### ♻️ Refactor

- Rename the renderer crates to fusuma-render, fusuma-render-embedded and fusuma-render-skia ([`e33e9ea`](https://github.com/Dodai-Tech/fusuma/commit/e33e9eadac528c9dd8739edb1fc5e218e809805d))
- Rename the project to Fusuma, keep the Tokonoma crates; add the layer rule (renderer never depends on toolkit) ([`b86c79b`](https://github.com/Dodai-Tech/fusuma/commit/b86c79b52430e2b59199f6102fd4ff72763878cb))

### 📚 Documentation

- Crates.io, docs.rs, MSRV and no_std badges; badges in each crate README ([`01e2ee8`](https://github.com/Dodai-Tech/fusuma/commit/01e2ee82f33155a89cc54b98cff04d2dfc092fef))

### 🔧 Chores

- Ship the MIT LICENSE inside each published crate ([`b023826`](https://github.com/Dodai-Tech/fusuma/commit/b023826ae6c67ad0b348f5796d3509ddf741cbb4))
- Gitea repo lives in the Dodai org ([`8037e2f`](https://github.com/Dodai-Tech/fusuma/commit/8037e2fcfa32ee5dcc6144f429739204d6a97534))
- Drop homepage, it duplicates repository ([`90ed47f`](https://github.com/Dodai-Tech/fusuma/commit/90ed47f15387e364448cb5c4ddaad35a51fbcde5))


