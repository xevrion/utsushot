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

## Capture

Currently shells out to `grim -o winit` against the nested display. Tracked in
issue #3 to replace with wlr-screencopy through `wayland-client`.

## Status

Implemented against the nested-instance approach described above.
