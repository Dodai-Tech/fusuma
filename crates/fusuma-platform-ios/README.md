# fusuma-platform-ios

Fusuma on iPhone and iPad. **A scaffold**: the translation of UIKit's touch phases and hardware key strings into Fusuma input,
tested on the desktop; the view, the Metal surface and the run loop are not written.

Done and tested: `UITouch` phases to `Pointer`, `UIKeyCommand` input strings to `Key` and `Edit`, over `fusuma-platform-mobile`
(lifecycle, safe areas, the soft keyboard, the edge-swipe back).

How it will run (not built): a `UIView` backed by a `CAMetalLayer` gives a `winit` window; Vello draws on Metal through `wgpu`;
`UIKeyInput` and `UITextInput` carry typing and marked text (the preedit); `safeAreaInsets` feed `Display`; the display link
paces frames. Building needs macOS and Xcode, so CI for this host needs a Mac runner (none yet).
