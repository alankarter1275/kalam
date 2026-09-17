# Architecture Plan: Scroll Auto-Hide, Ctrl+F Reliability, and Pixman Bug

## 1. Top and Bottom Pills Auto-Hide on Scroll
- **Problem**: When the user scrolls with a mouse wheel or touchpad, the top and bottom pills don't hide immediately.
- **Cause**: In `src/pages/reader/mod.rs`, the `scroll_ctrl` event controller on `root` uses the default `Bubble` propagation phase. Inside the reader, `ReaderView` (`kalam-reader/src/view.rs`, line 1471) attaches its own `EventControllerScroll` to `self.area` and returns `glib::Propagation::Stop` whenever it handles scroll steps. Because `ReaderView` consumes and stops the scroll event, it never bubbles up to `root`. In addition, `ReaderMsg::UserScrolled` had edge checks (`!mouse_in_top_edge`) that prevented hiding if the pointer lingered near the edge.
- **Solution**:
  - Configure `scroll_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);` on `root`. The capture phase intercepts the event before child widgets consume it.
  - In `ReaderMsg::UserScrolled`, unconditionally set `show_back_button = false` and `show_bottom_pill = false`, and remove any pending timers.

## 2. Ctrl+F Search Shortcut Reliability
- **Problem**: Pressing `Ctrl+F` opened the search bar, but after closing it with `Escape`, pressing `Ctrl+F` again failed to open search until clicking/tapping on the reader area.
- **Cause**:
  - Focus lifecycle in GTK4: When the user pressed `Ctrl+F`, `widgets.search_entry.grab_focus()` moved window focus into the search text field. When `Escape` was pressed, the search revealer hid `search_entry`. In GTK4, when a currently focused widget is hidden or unmapped, GTK clears focus from it without automatically redirecting it, leaving `window.focus()` as `None`.
  - EventControllerKey scope: `gtk::EventControllerKey` attached to `root` (a `gtk::Overlay`) only receives keyboard events when either `root` or one of its descendants has focus. When focus was `None`, GTK dropped subsequent key events, so `Ctrl+F` was never received. Clicking the book gave focus to `ReaderView` (`view.widget()`), re-enabling keyboard delivery.
- **Solution**:
  - Installed a `gtk::ShortcutController` with `gtk::ShortcutScope::Global` on `root` bound to `<Control>f`. In GTK4, `ShortcutScope::Global` triggers across the whole window regardless of whether any widget has focus.
  - In `Key::Escape` handler, `ReaderMsg::CloseSearch`, and `ReaderMsg::ToggleSearch` (closing branch), explicitly restore focus to `view.widget()` immediately and via `glib::idle_add_local_once` so reader navigation ('n', 'p', 's', 'w') works immediately after closing search without requiring a mouse click.

## 3. Pixman Bug: `In pixman_region32_init_rect: Invalid rectangle passed`
- **Problem**: Repeated warnings in stderr:
  ```text
  *** BUG ***
  In pixman_region32_init_rect: Invalid rectangle passed
  Set a breakpoint on '_pixman_log_error' to debug
  ```
- **Cause**:
  - Pixman explicitly logs this warning whenever `width == 0` or `height == 0` is passed to `pixman_region32_init_rect`.
  - In Adwaita, `scrollbar > range > trough > slider:disabled` ships `opacity: 0`. Any scrollbar on content that does not overflow has a disabled slider, which forced GTK to render through an offscreen surface with zero dimensions.
  - In `resources/style.css`, `.kalam-lib-scroll scrollbar` and `.kalam-float-tags-scroll > scrollbar` had `min-width: 0; min-height: 0;` forcing 0-pixel allocations.
  - In `src/pages/library.rs:607`, `continue_strip` was configured with `hscrollbar_policy(PolicyType::Automatic)` and `vscrollbar_policy(PolicyType::Never)`, and `src/pages/book_float.rs` had `vscrollbar_policy` defaulted to `Automatic`.
- **Solution**:
  - In `resources/style.css`:
    - Overrode `scrollbar slider:disabled`, `scrollbar.overlay-indicator slider:disabled`, and `scrollbar > range > trough > slider:disabled` with `background-color: transparent; opacity: 1;` so Adwaita's `opacity: 0` never causes zero-sized offscreen surfaces.
    - Zeroed `outline: none; margin: 0; padding: 0;` on `scrollbar slider`, `scrollbar trough`, and `scrollbar range`.
    - Removed `min-width: 0; min-height: 0;` from `.kalam-lib-scroll scrollbar` and `.kalam-float-tags-scroll > scrollbar`.
  - In `src/pages/library.rs:607`: configured `continue_strip` with `.hscrollbar_policy(gtk::PolicyType::External)` so GTK never allocates or draws a scrollbar widget, while wheel/touchpad horizontal scrolling continues to function smoothly.
  - In `src/pages/book_float.rs:466`: configured `tags_scroll` with `set_vscrollbar_policy: gtk::PolicyType::External`.
  - Kept all existing scrollbar widths, styling, and transparent visuals 100% intact.
