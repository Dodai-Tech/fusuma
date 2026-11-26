# fusuma-access

Fusuma for assistive technology. The toolkit already describes every widget on the screen with no pixels in it (`Tree::semantics`:
a role, a name, a state, a value, a rectangle). This crate turns that into an [AccessKit](https://accesskit.dev) tree and turns
what a screen reader asks for back into calls on the tree:

- `tree_update(&tree, window_name)` is a full `accesskit::TreeUpdate`: a window, and under it every widget with its role, name,
  checked/selected/disabled state, slider value and range, text field content and placeholder, bounds and children, and the
  focused widget. It is rebuilt whole each time (a screen is a few hundred nodes at most); AccessKit finds what changed.
- `perform(&mut tree, &request)` does what the request says: *click* presses the widget, *focus* focuses it, *increment* and
  *decrement* move a slider by a step, *replace selected text* types into the focused field, *scroll into view* scrolls to it.
  It says what it did (`Done`, `Ignored`, `Gone` for a node that is no longer there).

Node numbers are the tree's own handles (`NodeId::to_raw`), so a request made about an older screen finds a node that is gone
and is refused, never the wrong widget.

Platform adapters (`accesskit_winit` and the Android, iOS and Unix ones) call these two functions; `fusuma-platform-desktop` has
the winit one behind its `accessibility` feature. **Not tried with a real screen reader yet**: the tests check the tree and the
requests, not what Orca, NVDA, VoiceOver or TalkBack say.
