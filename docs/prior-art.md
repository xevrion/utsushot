# Prior art

## Fabrishot

[ramidzkh/fabrishot](https://github.com/ramidzkh/fabrishot) (MIT) captures
Minecraft screenshots far above window resolution. Read at commit depth 1 on
2026-08-12. It is the closest existing implementation of utsushot's idea, and
it works in three steps that map onto ours directly.

**1. Lie about the surface size.** `WindowMixin` intercepts `getScreenWidth`,
`getScreenHeight`, `getGuiScaledWidth` and `getGuiScaledHeight`, returning the
configured capture size instead of the real window, but only while
`Fabrishot.isInCapture()` is true.

**2. Force a re-render at that size.** `Fabrishot.refresh()` calls
`framebuffer.resize(window.getScreenWidth(), window.getScreenHeight())`, which
now reads the inflated values. The game re-renders the scene at the larger size
rather than upscaling a display-resolution frame. This is the same distinction
utsushot draws between re-rendering and interpolation.

**3. Restore by inverting the lie.** When the task finishes, `task` is set to
null so `isInCapture()` goes false, and `refresh()` is called again. The mixin
now returns the true window size and the framebuffer resizes back.

### Where the analogy stops

Fabrishot owns its renderer, so it can lie to it in-process through mixins. Its
"phantom output" is nothing more than a pair of intercepted getters.

utsushot cannot do that. The renderers are every GTK/Qt/wlroots client on the
system, in separate processes, and the only channel for telling them to
re-render at a new density is the output scale the compositor advertises. Hence
a real phantom output where Fabrishot needs only a fib. This is also why the
project is compositor-specific and Fabrishot is not.

### What to borrow

**Wait frames before capturing.** `CaptureTask.onRenderTick` does not capture on
the frame it starts. It counts up to `Config.CAPTURE_DELAY` first, because a
just-resized framebuffer is not ready immediately.

utsushot has the same hazard in a worse form. After enabling the phantom output
and moving the target, each client needs a Wayland roundtrip to learn the new
scale, re-layout, and repaint. Capturing immediately will yield a stale,
half-drawn, or wrongly-scaled buffer. A fixed delay is the crude fix; watching
niri's event stream for the window to settle on the new output is the better
one. Tracked in #6.

**Save and restore incidental state.** `CaptureTask` stores `hideGui` before
overriding it and puts it back afterwards, which is precisely what
`RestoreToken` exists for.

## Hotsampling

The technique the screenshot community actually uses, and mechanically the same
idea as utsushot's nested backend:

> Hotsampling works by resizing the game window past the bounds of your monitor.
> This resizing forces the game to render at the new resolution.

That is exactly what the niri backend does: size a surface beyond the physical
display and let the client re-render into it. Two groups converged on the same
trick independently, which is reassuring about the design.

Worth correcting a common belief, including one this document previously
repeated: in-game photo modes do *not* generally render above display
resolution. Cyberpunk 2077's does not, which is why a "Hot-Sampled Photomode
Renders" mod exists at all, and the well-known 5K GTA V galleries were made with
NVIDIA custom resolutions rather than a photo-mode feature. Supersampling in
games usually comes from DSR/VSR, which are fake display modes applied to the
whole desktop, not from photo mode itself.

## Tiled rendering: NVIDIA Ansel and TR

Ansel produces its very large captures by rendering the scene in tiles and
stitching them, offsetting the projection matrix per tile. Its SDK says so
directly, in `ansel/Camera.h`:

> The amount that the projection matrix needs to be offset by. These values are
> applied directly as translations to the projection matrix. These values are
> only non-zero during Highres capture.

Unreal's plugin confirms the same in `r.Photography.EnableMultipart`:
"high-resolution shots that need to be rendered in tiles which are later
stitched together". It is the same algorithm as Brian Paul's TR library from
thirty years earlier.

**This cannot be applied to a compositor**, and the reason is worth stating
because it looks tempting. Tiling works by moving a camera: you render the same
scene from a shifted frustum and paste the results together. A compositor has no
camera and no scene, only finished client buffers at a fixed resolution.
Shifting a viewport over them crops, it does not add detail.

Tiling is also lossy for anything that depends on screen position. Unreal
disables bloom dirt, lens flare, vignette and chromatic aberration under the
comment "these effects tile poorly", and freezes auto-exposure so tiles do not
each adapt to local brightness.
