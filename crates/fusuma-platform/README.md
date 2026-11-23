# fusuma-platform

The out-of-the-box renderer choice of Fusuma. Given a `Machine` (screen size, RAM, PSRAM, FPU, GPU and what the
settings say) it picks the backend a device should draw with, and says why: the build beats the settings,
which beat an automatic rule; a memory check keeps or steps down an explicit choice; and a choice that is not
linked is replaced by the nearest lighter one. `no_std`, no dependencies, so a platform crate, a build
script and a flashing wizard share one answer.
