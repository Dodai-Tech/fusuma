# fusuma-platform-android

Fusuma on Android phones and tablets. **A scaffold**: what is here is the translation of Android's own values into Fusuma's,
tested on the desktop; the activity, the surface and the event loop that would use it are not written.

Done and tested: `MotionEvent` actions to `Pointer`, `KeyEvent` codes to `Key` and `Edit` (the system **back** button is
`Key::Cancel`, which closes a dialog or a menu first), over `fusuma-platform-mobile` (lifecycle, safe areas, the soft keyboard).

How it will run (not built): a `NativeActivity` or a `GameActivity` (`android-activity`) gives a `winit` window; Vello draws on
it with Vulkan through `wgpu` (the CPU renderer is the fallback); `InputMethodManager` shows the keyboard when a text field has
the focus; `WindowInsets` feed `Display`. Packaging is a Gradle module, or Fusuma as a surface inside an existing app
framework (see `android-app-framework`).

Not decided: `GameActivity` against a Kotlin host, and how the clipboard and haptics reach a `Task`.
