# Upstream report draft for GTK (gitlab.gnome.org/GNOME/gtk)

Status: FINAL draft, ready to file. All placeholders filled (kernel and
compositor versions provided by the owner, 2026-10-08). Duplicate search
done the same day: three candidate issues reviewed, none matches this case
(see "Related prior reports") - filing a new issue is the right path.
Nothing gets filed until he has read it.

---

## ISSUE TITLE

Wayland: GL and Vulkan renderers 3-10x slower than cairo and X11 for texture-heavy frames (Intel Gen9 / Gemini Lake, Mesa 26.2.4)

## ISSUE BODY

### Summary

On a low-end Intel Gemini Lake machine (Gen9), GTK 4's GPU renderers under
Wayland paint texture-heavy frames 3-10x slower than the same app under
`GDK_BACKEND=x11`, and also slower than the software cairo renderer. The
pattern is consistent across two compositors (sway and a sway fork), across
both GPU renderers (GL and Vulkan), and across warm and cold machine states.
Light frames without images are fast in every configuration. Our measurements
suggest the cost sits in presenting GPU-rendered frames to the compositor,
not in rendering itself.

### Environment

- CPU/GPU: Intel Celeron (Gemini Lake), Intel UHD Graphics 605 (GLK),
  PCI ID 8086:0x3184, Gen9; 3729 MB unified video memory
- Distro: Arch Linux (rolling)
- GTK: gtk4 1:4.22.5-1
- Mesa: 26.2.4-arch1.1 (with vulkan-intel)
- Compositor: swayfx 0.6 (based on sway 1.12.0); the same behavior was
  reproduced under plain sway
- Kernel: 7.2.8-arch1-2
- XWayland: running on :0 (used only for the `GDK_BACKEND=x11` comparison)
- Realized renderer under Wayland (default): GskGLRenderer, verified by
  logging the realized renderer's type name in every session

### Symptom and measurements

The app is a GTK4 book-library reader (Rust / gtk-rs) whose pages show grids
of cover images. Per-frame paint-phase durations (ms) come from
GdkFrameClock timestamps: the app marks UI events and logs the paint duration
of the frame that lands them. Same tour, same library (~13 covers), same
window size, same machine:

| Surface (frame paint, ms)      | Wayland GL (default) | Wayland Vulkan | Wayland cairo | X11 GL   |
|--------------------------------|----------------------|----------------|---------------|----------|
| Home (few textures)            | 50-64                | 50-56          | ~52-62        | ~60      |
| Startup frame, 12 covers       | 283-297 best, 1610 worst (cold) | 232 | 227  | 170      |
| Library page (cover grid)      | 297 best, 305-445 typical, 601 cold | 198 | 194 | 91       |
| Book detail dialog (1 cover)   | 114-130 best, 247-362 typical, 1321 cold | 118-317, 944 during background texture churn | ~205 | 62 |
| Book page (several covers)     | 128 best, 640-809 typical | 128-189   | 263           | 142-173  |
| Author page (covers)           | 174-191 best, 246-358 typical | 174  | 198           | 66       |
| Reader open (mostly text)      | 208                  | 208-332        | 203           | 210      |

Notes:

- The X11 column was captured on the coldest machine state of the whole test
  campaign (first launch of the day, HDD busy, every service 5-20x its warm
  cost) - i.e. the X11 numbers are pessimistic and the gap is conservative.
- The worst Wayland-GPU numbers happen when background image decoding lands
  textures during a paint; UI-thread stalls of 250-950 ms accompany those
  frames.
- Picture-free pages (settings forms, lists) are fast everywhere, with cairo
  often the fastest of all on them.
- Reader open (initially mostly text) reaches parity across renderers; the
  gap is specific to frames that upload many new textures.

### What we ruled out

- Software rendering: `eglinfo -B` reports the Intel UHD 605 on every
  platform (GBM, Wayland, X11, surfaceless, device 0); llvmpipe is present
  only as an unused device 1. GL is hardware-accelerated.
- "The other GL renderer": this GTK has a single GL renderer (the old one was
  removed in 4.18; `GSK_RENDERER=ngl` prints "The new GL renderer has been
  renamed to gl" and realizes the same renderer).
- A GL-driver-specific problem: `GSK_RENDERER=vulkan` (ANV, no GL/EGL driver
  involvement) shows the same slowness on the same surfaces (library page
  198 ms warm vs GL 297 ms warm vs X11 91 ms cold; and a 944 ms first dialog
  during background texture churn).
- The app itself: `GSK_RENDERER=cairo` (software, same widget tree, same
  images, same window) paints the heavy surfaces in 194-263 ms and never
  shows the catastrophic frames.
- The compositor: same behavior under sway and swayfx 0.6 (compared across
  sessions, not a same-session A/B; happy to run a stricter test if useful).
- Machine state: verified on the warmest state (all caches hot) and the
  coldest; the gap persists in every state.
- Older Mesa: could not be tested on Arch - Mesa builds are pinned to their
  build-day LLVM soname; both 26.1.8 and 25.3.5 fail to load
  (`ldd libgallium-26.1.8-arch1.1.so` reports `libLLVM.so.22.1 => not
  found`). Mentioned in case it matters for triage.

### Related prior reports (checked before filing)

We searched the tracker and reviewed three candidates; none matches this
case:

- #4704 "`gdk_x11_gl_context_texture_from_surface` is not fit for purpose,
  slower than the software fallback" (GTK3, X11): the mirror image of our
  case - there the dedicated X11 GLX texture-from-pixmap path was slower
  than the software fallback, i.e. X11 slow and Wayland fine. Here it is
  the reverse, on GTK4.
- #4112 "OpenGL show window performance is significantly slower than
  cairo" (GTK 4.2.1, NVIDIA, closed): closest in spirit, but it measured
  the first window shown - one-time GL context creation, per the
  maintainer's analysis - and GL was slow under X11 too. Our case is a
  sustained per-frame cost on every texture-heavy frame, X11 is our
  fastest configuration, and the hardware is Intel Gen9.
- #8114 "Images for recolored icons are constantly being reloaded"
  (GTK3/gdk-pixbuf, closed): an icon reload loop; different mechanism
  entirely - our images decode once off the UI thread and are cached.

### Hypothesis (and what we cannot see from outside)

Both GPU renderers pay a large per-frame cost that cairo does not, and only
on Wayland. That points at the GDK-Wayland present path for GPU-rendered
buffers - for example a wl_shm readback being taken instead of a zero-copy
dmabuf handoff. On this weak CPU, a full-window readback per frame would
produce exactly this pattern: cost proportional to window size and texture
churn, catastrophic when several textures land in one frame, invisible for
light frames. We found no way to verify from outside GTK which buffer path
is actually taken.

Questions:

1. Is there a supported way to log which present path the Wayland backend
   chose for a GPU renderer (dmabuf vs wl_shm) - an env var, a debug flag,
   or a log line?
2. Could a failed dmabuf negotiation with the compositor silently fall back
   to a readback path, and would it look like this?
3. Is this known on Gen9-era Intel, or reported elsewhere?
4. What additional data would help (WAYLAND_DEBUG trace, GTK_DEBUG output,
   a specific demo app to test)? We can produce any of it.

### Reproduction

The app is open source and builds cleanly in CI:
https://github.com/alankarter1275/kalam

```
git clone https://github.com/alankarter1275/kalam
cd kalam
KALAM_TIMING=1 cargo run --release
```

Open the library page (a grid of book cover images), then compare renderers
via environment variables:

- default (Wayland, GskGLRenderer): as above
- `GSK_RENDERER=cairo` (needs nothing extra)
- `GSK_RENDERER=vulkan` (needs the Intel Vulkan driver installed)
- `GDK_BACKEND=x11` (through XWayland)

`KALAM_TIMING=1` prints per-frame paint durations (`frame_paint:` lines),
UI-thread stall warnings, and the realized renderer's type name
(`render_backend`) in every session, so the comparison is directly
reproducible. On this machine the library page paints ~297 ms (GL), ~198 ms
(Vulkan), ~194 ms (cairo), ~91 ms (X11).

We are happy to run any variant or collect any trace the maintainers
consider useful.
