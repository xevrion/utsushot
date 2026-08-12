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

There is no `create`, no `headless`, and no virtual-output verb. This is the
primitive sway and Hyprland both have and niri does not, and it is the single
fact that shapes this backend.

## Consequence: the output must pre-exist

Since we cannot conjure one, the user declares it. utsushot looks for an output
named `utsushot-phantom` that is disabled in the niri config, then drives it
through `custom-mode` → `scale` → `on` for the capture and back to `off`
afterwards.

The config side looks like this:

```kdl
output "utsushot-phantom" {
    off
}
```

That entry alone is inert. niri only applies configuration to outputs it can
see, so this raises a second open question tracked in #1: whether a named output
with no backing hardware is addressable at all, or whether the phantom has to be
a real headless output supplied by something else. This needs testing against a
live session before the backend can be called working.

## Promising alternative: portal ScreenCast VIRTUAL

xdg-desktop-portal's ScreenCast interface advertises source types as a bitmask,
where bit 2 (`4`) is `VIRTUAL`. Probed on a live niri 26.04 session
(2026-08-12):

```console
$ busctl --user get-property org.freedesktop.portal.Desktop \
    /org/freedesktop/portal/desktop \
    org.freedesktop.portal.ScreenCast AvailableSourceTypes
u 7
```

`7` is `MONITOR | WINDOW | VIRTUAL`, so **VIRTUAL is advertised here**, on
ScreenCast interface version 5.

That makes this the more attractive path of the two, because it needs nothing
from the user's config and no hardware behind the output. What the probe does
*not* establish is whether niri's implementation actually honours a VIRTUAL
request with a caller-chosen resolution and scale, or merely lists the bit.
Answering that means driving a real `CreateSession` → `SelectSources` →
`Start` exchange and inspecting the resulting PipeWire stream. Tracked in #1
alongside the config-based approach; whichever proves out first becomes the
default, and the other stays as fallback.

Note that the portal path also changes the capture side: it yields a PipeWire
stream rather than something `grim` can address, so it pairs naturally with the
native capture work in #3.

## Capture

Currently shells out to `grim -o <output>`. Tracked in #3 to replace with
wlr-screencopy through `wayland-client`, which removes the runtime dependency
and avoids a round trip through a temp file.

## Status

`create_phantom` and `move_target` return `Error::Unimplemented` with a pointer
to this document. `cleanup` is already wired into the panic-safe guard, so once
the first two land there is no separate restore path to write.
