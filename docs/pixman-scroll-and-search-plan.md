# Architecture Plan: Scroll Auto-Hide, Ctrl+F Reliability, and Pixman Bug

## 1. Top and Bottom Pills Auto-Hide on Scroll
- **Problem**: When the user scrolls with a mouse wheel or touchpad, the top and bottom pills don't hide immediately.
- **Cause**: In `src/pages/reader/mod.rs`, the `scroll_ctrl` event controller on `root` uses the default `Bubble` propagation phase. Inside the reader, `ReaderView` (`kalam-reader/src/view.rs`, line 1471) attaches its own `EventControllerScroll` to `self.area` and returns `glib::Propagation::Stop` whenever it handles scroll steps. Because `ReaderView` consumes and stops the scroll event, it never bubbles up to `root`. In addition, `ReaderMsg::UserScrolled` had edge checks (`!mouse_in_top_edge`) that prevented hiding if the pointer lingered near the edge.
- **Solution**:
  - Configure `scroll_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);` on `root`. The capture phase intercepts the event before child widgets consume it.
  - In `ReaderMsg::UserScrolled`, unconditionally set `show_back_button = false` and `show_bottom_pill = false`, and remove any pending timers.

## 2. Ctrl+F Search Shortcut Reliability
- **Problem**: Pressing `Ctrl+F` sometimes opens the search bar, but other times seems to do nothing.
- **Cause**:
  - Event propagation: The keyboard controller on `root` uses `Bubble` phase. If focus is captured by `ReaderView` or a sidebar button/entry, or lost from `root`, keyboard combinations may not bubble up.
  - Widget state / focus: The search bar is inside a `gtk::Revealer` with `#[watch] set_visible: model.search_active`. In `ToggleSearch`, `widgets.search_entry.grab_focus()` was called immediately during `update()`, when the revealer in the GTK tree still had `visible: false`. In GTK4, grabbing focus on an unmapped/invisible widget fails silently.
- **Solution**:
  - Set `key.set_propagation_phase(gtk::PropagationPhase::Capture);` on the root key controller.
  - Remove `set_visible: model.search_active` from the search revealer (the revealer handles visibility smoothly).
  - Schedule `search_entry.grab_focus()` with `glib::idle_add_local_once` so the GTK layout pass finishes mapping the entry before focus is requested.

## 3. Pixman Bug: `In pixman_region32_init_rect: Invalid rectangle passed`
- **Problem**: Repeated warnings in stderr:
  ```
  *** BUG ***
  In pixman_region32_init_rect: Invalid rectangle passed
  Set a breakpoint on '_pixman_log_error' to debug
  ```
- **Cause**:
  - Pixman is Cairo's low-level 2D rasterization library. It logs this bug when a clip rectangle or surface allocation has a width or height < 0 (or overflow).
  - In GTK4 / Libadwaita, default scrollbars have `.overlay-indicator` with `opacity: 0` when inactive.
  - Any widget with `opacity < 1` forces GTK to render through an offscreen surface.
  - When a `ScrolledWindow` has zero scroll range or has not yet completed layout measurement, the scrollbar has a 0x0 allocation. An offscreen surface for a 0x0 widget triggers `pixman_region32_init_rect`.
  - Kalam's `style.css` originally hid scrollbars via transparent backgrounds, but never explicitly set `opacity: 1;` on `scrollbar`. Thus, Libadwaita's built-in `.overlay-indicator:not(.hovering) { opacity: 0; }` was still active.
  - Additionally, progress bars with rounded corners (`.kalam-nr-prog progress`, `.kalam-goal-bar progress`) had `border-radius: 999px;` without a `min-width: 4px;` clamp. At 0% progress, GTK attempts to clip a 0-width rounded rectangle, passing invalid coordinates to Pixman.
- **Solution**:
  - Set `opacity: 1;` on `scrollbar`, `scrollbar.overlay-indicator`, `scrollbar trough`, and `scrollbar slider` in `resources/style.css`.
  - Add `min-width: 4px;` to all progress bars with `border-radius: 999px`.
  - Preserve all existing dimensions, colors, and transparent backgrounds so scrollbar aesthetics remain completely unchanged.
