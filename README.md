# utsushot

[![CI](https://github.com/xevrion/utsushot/actions/workflows/ci.yml/badge.svg)](https://github.com/xevrion/utsushot/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/utsushot.svg)](https://crates.io/crates/utsushot)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

Supersampled screenshots on Wayland. Instead of capturing your display's pixels and enlarging them, utsushot builds a temporary high-resolution *phantom* output, moves the target onto it so the toolkit re-renders everything at true N× pixel density, captures that, and puts your session back.

Named after utsushi-e (写し絵), the Edo-era magic-lantern shows that projected phantom images onto screens.

> **Status: early development.** The scaffold, CLI, and backend architecture are in place; no backend can complete a capture yet. See [What's left](#whats-left).

## Why not just upscale?

Because upscaling has nothing to work with. A 1280x720 screenshot enlarged 4× is interpolation: the glyph edges were already rasterized at 720p, and no filter recovers detail that was never sampled.

utsushot changes the input instead. Wayland-native toolkits render text and vector UI from source at whatever scale the output declares, so a phantom output at 5120x2880 with `scale 4` produces genuinely sharp glyphs, not smoothed ones.

```
  ┌─────────────┐   move    ┌──────────────────────┐
  │ your screen │ ────────► │   phantom output     │
  │ 1280x720    │  target   │   5120x2880 @ 4x     │
  │ scale 1     │           │   (never displayed)  │
  └─────────────┘           └──────────┬───────────┘
         ▲                             │ toolkit re-renders
         │      restore                │ at true 4x density
         └─────────────────────────────┤
                                       ▼
                                  capture → PNG
```

It is the same idea as a game's photo mode rendering at higher-than-display resolution, or the Minecraft [Fabrishot](https://github.com/ramidzkh/fabrishot) mod, which tells the game its window is 8K, re-renders into a framebuffer that size, then resizes back. [docs/prior-art.md](docs/prior-art.md) breaks down how Fabrishot works and what utsushot borrows from it.

## Install

Nothing is published yet. From source:

```sh
git clone https://github.com/xevrion/utsushot
cd utsushot
cargo install --path .
```

Runtime dependencies: `grim` for capture, plus `wl-clipboard` for `--copy` and `libnotify` for `--notify`.

## Usage

```sh
utsushot                        # 4x, to ~/Pictures/utsushot_<timestamp>.png
utsushot --scale 2 --copy       # 2x, straight to the clipboard
utsushot --list-backends        # what's supported, what's detected here
```

Exit codes: `0` success, `2` usage, `3` no backend, `4` backend unimplemented, `5` missing dependency, `6` capture failed, `7` restore failed.

## Backends

| Backend  | Status      | Approach |
|----------|-------------|----------|
| niri     | in progress | Pre-configured disabled output, resized and enabled per capture ([details](docs/backends/niri.md)) |
| sway     | planned     | `swaymsg create_output` ([details](docs/backends/sway.md)) |
| hyprland | planned     | `hyprctl output create headless` ([details](docs/backends/hyprland.md)) |
| GNOME / KDE | researching | Likely the xdg-desktop-portal ScreenCast virtual source |
| X11      | not planned | No per-output scaling to exploit; see [docs/backends/x11.md](docs/backends/x11.md) |

niri cannot create outputs over IPC as of 26.04, which is why its approach differs from the others. The [niri backend doc](docs/backends/niri.md) records that finding in full.

## What's left

Roughly in order.

- [ ] **#1 niri phantom output** — locate a disabled `utsushot-phantom` output, apply custom mode + scale, enable and disable around the capture
- [ ] **#2 niri move target** — move window/workspace to the phantom and record the origin for restore
- [ ] **#4 real output geometry** — derive phantom size from the actual focused output instead of the hardcoded 1920x1080
- [ ] **#6 wait for clients to settle** — clients need a roundtrip to re-render at the new scale; capturing immediately yields a stale buffer (Fabrishot hits the same problem and solves it with a frame delay)
- [ ] **#5 live integration tests** — assert captured dimensions and that a failed capture restores the session
- [ ] **#3 native capture** — wlr-screencopy via `wayland-client`, dropping the `grim` dependency
- [ ] **sway backend** — the first one that gets to create an output at runtime
- [ ] **hyprland backend**
- [ ] Window/region selection rather than whole-output only
- [ ] Publish `0.0.1` to crates.io to reserve the name

Contributions are very welcome, and a new backend is the most useful one. [CONTRIBUTING.md](CONTRIBUTING.md) has a walkthrough of the `Backend` trait.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
