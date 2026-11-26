# fusuma-platform-linux-mobile

Fusuma on Linux phones and tablets: postmarketOS with Phosh, Plasma Mobile or Sxmo, Mobian, and the like. **A scaffold**: it
holds what is different from a desktop, tested; the window itself comes from `fusuma-platform-desktop` (`winit` on Wayland), so a
Linux phone already runs the desktop host. What this adds when it is built: the shell (which decides whether a virtual keyboard
is needed and whether the shell draws the status bar), the output rotation (`wl_output` transforms swap width and height) and the
touch-only rules.

Done and tested: shell detection from `XDG_CURRENT_DESKTOP`, rotation to size and degrees.
