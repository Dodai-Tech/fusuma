# fusuma-platform-mobile

What every phone and tablet host has in common, so that `fusuma-platform-android`, `fusuma-platform-ios` and
`fusuma-platform-linux-mobile` only have to translate their own system into these values. `no_std`, no allocator, and tested
on the desktop: no phone is needed to check the rules.

- **`Session`** turns the system's lifecycle events (created, resumed, paused, surface created, surface destroyed, low memory)
  into what the host must do: make the GPU surface, drop it, draw, stop drawing, or release memory. A phone destroys the
  surface when the app goes to the background and gives a new one on return; drawing to a dead surface is a crash.
- **`Display`** is the screen as the system describes it (pixels, scale, notch and gesture-bar insets, the soft keyboard's
  height) and gives the rectangle an app may lay out in, with the keyboard counted as an inset.
- **`SoftKeyboard`** says when to show or hide the keyboard, from whether a text field has the focus, and only once per change.
- **`back`** maps the system back gesture or button to Fusuma's cancel key (which closes a dialog or a menu first).

Status: the rules and their tests are done; the hosts that use them are scaffolds (see the roadmap).
