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

## Game photo modes

Console and PC photo modes commonly render at higher-than-display resolution
before downsampling, which is supersampling in the same sense. They differ in
purpose: they downsample back to display resolution for antialiasing, whereas
utsushot keeps the full N× buffer, since the output is a file rather than a
frame.
