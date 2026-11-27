# How the crates are split (the default rule)

**One crate per toolchain or required dependency stack; features for everything inside it; a runtime crate per chip family.**

## Own crate when

* it needs a different toolchain or build environment (`fusuma-platform-esp`: the Xtensa fork, ESP-IDF, C components; outside the
  workspace, built by `scripts/showcase.nu`);
* it needs a different standard-library situation (`std` on Linux or the web against `no_std` on a chip);
* it needs its own dependency stack (`winit`, `wasm-bindgen`, `android-activity`, a vendor HAL).

## Feature when

the same toolchain builds it and only the compiled code differs: a controller (`ssd1680`, `uc8179`, ...), `fonts`, `vello-cpu`,
`bitmap-extended`, the mobile host of one OS.

## Embedded is one crate

`fusuma-platform-embedded` holds everything that is portable `no_std` and testable on a PC: the board and driver core, the surface,
and the e-paper module `eink` (planner, packing, `EinkDriver`, `Panel`, `Bus`, the controllers). Drivers know nothing about a chip: they
talk through `Bus` (command, data, reset, busy, delay). A chip family adds a **runtime crate** that implements `Bus` and the run loop for
it, named after the vendor and HAL, not the CPU architecture (`fusuma-platform-esp` covers the RISC-V C6 and the Xtensa S3 alike; an
RP2040 or STM32 would be its own crate). A Linux computer with an e-paper HAT is a host crate that implements `Bus` over SPI and GPIO.

## Rules

* The module boundary is the future crate boundary: `eink` and its controllers are the only code that knows panels; nothing reaches into
  another module's internals.
* CPU architectures (ARM, RISC-V, Xtensa) are **build targets in CI** (`scripts/ci/embedded_matrix.nu`), not crates.
* Controller features are tested **alone and together**: features add up in a workspace build, so `--all-features` cannot show that one
  builds without another. One matrix cell per feature on the small targets.
* Split a crate only for a concrete reason: a driver needs its own dependency (e.g. `embedded-hal`), someone outside the project wants one
  piece, or a part needs a different MSRV or licence. Never one crate per controller or per architecture.
* 8-bit AVR boards are out of scope (no allocator, 2 KB of RAM).
