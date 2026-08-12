# niri backend

## Finding: niri cannot create outputs at runtime

Checked against niri **26.04** and `niri-ipc` **26.4.0** (2026-08-12).

`niri-ipc`'s `Request` enum has no output-creation variant. The full set is
`Outputs`, `Workspaces`, `Windows`, `Layers`, `KeyboardLayouts`, `FocusedOutput`,
`FocusedWindow`, `PickWindow`, `PickColor`, `Action`, `Output`, `EventStream`,
`Version`, `ReturnError`, `OverviewState`, `Casts`.

`Request::Output` reconfigures an output that already exists. Its actions, as
exposed by `niri msg output <OUTPUT> <ACTION>`:

```
off  on  mode  custom-mode  modeline  scale  transform  position  vrr
```

There is no `create`, no `headless`, and no virtual-output verb. `niri --help`
has no headless backend flag either, and there is no open upstream issue
proposing one.

## Two approaches that do not work

**Pre-configured phantom output.** The idea was to have the user declare a
disabled output in their config and enable it around the capture. niri accepts
the configuration but reports:

```console
$ niri msg output utsushot-phantom on
Output "utsushot-phantom" is not connected.
The change will apply when it is connected.
```

Nothing ever connects it. There is no hardware behind the name and no way to
supply any, so there is nothing for `grim` to target and nowhere to move a
window. Dead end.

**Portal ScreenCast VIRTUAL.** `AvailableSourceTypes` on this machine returns
`7` (`MONITOR | WINDOW | VIRTUAL`), which initially looked promising. It is
misleading. Checking which implementation actually answers:

```console
$ busctl --user list | grep impl.portal.desktop
org.freedesktop.impl.portal.desktop.gtk        ...
org.freedesktop.impl.portal.desktop.hyprland   ...
```

`xdg-desktop-portal-gnome` is not installed, the gtk backend exposes no
ScreenCast interface at all, and the `7` comes from
**xdg-desktop-portal-hyprland**, which talks to Hyprland rather than niri. So
the VIRTUAL bit says nothing about niri's capabilities. Dead end.

## What does work: a nested niri instance

niri runs as a nested Wayland client under an existing compositor, using its
winit backend. That nested instance gets its own output, its own IPC socket,
and its own Wayland display, entirely independent of the host session.

The key fact, verified on a 1920x1080 physical screen: **the nested output is
not clamped to the host display size.** The winit backend ignores the `mode`
declared in the config and instead sizes its output to its host window, so the
size is set by resizing that window from the host compositor. Sizing it to
5120x2880 produces exactly that:

```console
$ NIRI_SOCKET=<nested.sock> niri msg -j outputs
winit buffer: 5120 x 2880 | logical: 1280 x 720 @ scale 4.0
```

That is the phantom output from the project's original design, running on real
hardware a quarter of its size. Clients inside it re-render at true 4x density,
and `grim` captures the full 5120x2880 buffer.

### Verified end-to-end recipe

```sh
# 1. Config declaring the scale. The mode is ignored by winit but the scale is not.
cat > phantom.kdl <<'EOF'
output "winit" {
    mode "5120x2880"
    scale 4
}
hotkey-overlay { skip-at-startup; }
EOF

# 2. Start the nested compositor. It prints its socket and display to the log.
niri -c phantom.kdl &
#    INFO niri: listening on Wayland socket: wayland-2
#    INFO niri: IPC listening on: /run/user/1000/niri.wayland-2.<pid>.sock

# 3. Resize its window from the HOST compositor to set the output size.
niri msg action move-window-to-floating --id <id>
niri msg action set-window-width  --id <id> 5120
niri msg action set-window-height --id <id> 2880

# 4. Launch the target application INTO the nested instance.
WAYLAND_DISPLAY=wayland-2 kitty

# 5. Capture the phantom output.
WAYLAND_DISPLAY=wayland-2 grim -o winit shot.png   # -> 5120x2880 PNG

# 6. Kill the nested instance. The host session is never modified.
kill <nested-pid>
```

Confirmed on 2026-08-12: the resulting PNG is 5120x2880 and text is genuinely
re-rendered, with crisp glyph edges rather than interpolation blur.

### Tradeoffs

The target application is launched fresh inside the nested compositor rather
than being an existing window from the live session. This is a hard protocol
limit, not an implementation gap: a Wayland client is bound to the compositor it
connected to at startup, and no protocol exists for migrating a live surface to
a different compositor. An existing window therefore cannot be moved onto the
phantom, whatever the compositor supports.

Capturing the *live* desktop at N× would require the opposite approach: putting
the real output into an oversized custom mode at a higher scale, capturing, then
restoring it. niri's IPC does expose `custom-mode` and `scale`, so this is
plausible, but it visibly disrupts the session and a failure part-way through
can leave the display in a mode the panel cannot display. Tracked in #9, and not
implemented for that reason.

In exchange, the host session is never touched at all. No mode changes, no
scale changes, no windows moved. If utsushot crashes mid-capture the worst
outcome is an orphaned nested process, which cannot strand the user's real
display. That is a considerably safer failure mode than reconfiguring the
physical output would be.

The nested window is briefly visible on screen while it is sized and the target
renders. Positioning it offscreen, or capturing quickly, mitigates this.

## Why scale alone does nothing

An obvious idea is to raise only the output scale and leave the mode alone, on
the theory that clients would re-render at higher density into the same display.
Measured on niri 26.04, that is not what happens:

```console
$ niri msg output winit scale 4
buffer 1262x1386 | logical 315x346 @ scale 4.0
```

The framebuffer is unchanged and the logical desktop *shrinks*. Clients do
re-render at 4x density, but the compositor then downsamples them into the same
scanout buffer, so the extra detail is rendered and immediately discarded. The
capture is identical in size and no sharper.

This is the crux of the whole problem. The pixels utsushot wants are being drawn
and thrown away. Keeping them would mean compositing into an offscreen buffer
larger than the scanout buffer, which is a change inside the compositor.

## Why `--live` is capped by the monitor

`--live` raises the *mode*, which is the only thing that adds real pixels, and
raises the scale alongside it so the logical layout is preserved. Its ceiling is
therefore whatever modes the panel advertises. A monitor that lists a 3840x2160
mode it does not normally use gives a genuine 2.25x capture; a laptop panel that
lists only its native resolution gives nothing, and utsushot skips the modeset
entirely rather than flickering the display for no gain.

The mode change also blanks the screen while the link retrains. That is a
hardware property of changing the signal timing and cannot be avoided from
userspace.

## What actually solves this: a 16-line compositor patch

Everything above is a workaround for the compositor not offering the one thing
needed. That thing turns out to be very small. `Niri::screenshot` in
`src/niri.rs` already renders into an offscreen texture:

```rust
let size = output.current_mode().unwrap().size;
let scale = Scale::from(output.current_scale().fractional_scale());
let pixels = render_to_vec(renderer, size, scale, ..)?;
```

Multiplying `size` and `scale` by N renders the live desktop into a texture N
times larger. The display is never touched, so there is no modeset, no blanking,
and no ceiling from the panel. `docs/niri-supersample.patch` is that change.

Verified against a patched niri 26.04 build on 2026-08-12. A nested instance
with a 1262x1386 output produced a **5048x5544** screenshot, exactly 4x, with no
mode change. Downscaling that result back to native and comparing against a
plain capture gives an RMSE of 0.124, against 0.018 for `grim -s 4` upscaling of
the same scene: roughly seven times more divergence, which is detail that only
exists because the scene was re-rendered.

utsushot asks for this by writing the factor to
`$XDG_RUNTIME_DIR/niri-screenshot-supersample` before triggering
`screenshot-screen`. A stock niri ignores the file and writes an output-sized
image, which utsushot detects from the PNG header and discards before falling
back. One limitation: `screenshot-screen` always captures the focused output, so
this path is only used when the requested output is the focused one.

Upstreaming this, or niri PR #3800 (virtual outputs), would remove the need for
both the nested instance and the mode switching.

## Capture

Currently shells out to `grim -o winit` against the nested display. Tracked in
issue #3 to replace with wlr-screencopy through `wayland-client`.

## Status

Implemented against the nested-instance approach described above.
