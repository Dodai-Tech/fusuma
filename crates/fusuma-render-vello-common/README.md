# fusuma-render-vello-common

[![Crates.io](https://img.shields.io/crates/v/fusuma-render-vello-common.svg)](https://crates.io/crates/fusuma-render-vello-common)
[![docs.rs](https://img.shields.io/docsrs/fusuma-render-vello-common)](https://docs.rs/fusuma-render-vello-common)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The shared half of the Fusuma Vello backends. `vello_cpu` and `vello_gpu` have the same drawing
methods and the same types, so the mapping from Fusuma's `Renderer` trait to Vello is written once,
here, as `Backend<S>` over a small `Scene` trait. `fusuma-render-vello-cpu` and
`fusuma-render-vello` only say how to record, render and read back a scene.

You do not use this crate directly: depend on one of those two.
