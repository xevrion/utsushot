# utsushot

[![CI](https://github.com/xevrion/utsushot/actions/workflows/ci.yml/badge.svg)](https://github.com/xevrion/utsushot/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/utsushot.svg)](https://crates.io/crates/utsushot)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

Supersampled screenshots on Wayland. Instead of capturing your display's pixels and enlarging them, utsushot runs an application on a temporary high-resolution *phantom* output, so the toolkit re-renders everything at true N× pixel density, captures that, and tears the phantom down.

Named after utsushi-e (写し絵), the Edo-era magic-lantern shows that projected phantom images onto screens.

> **Status: early development, working on niri.** Two modes:
>
> - `utsushot -- <command>` runs an application on a phantom output at N× and captures it. Genuine N× at any factor, and your session is never touched.
> - `utsushot --live` captures the desktop you are actually looking at. Limited to the largest mode your monitor advertises, and the screen blanks briefly while the mode changes.
>
> Neither can give you an unlimited-resolution capture of your live desktop without a compositor change. [Scope](#scope) explains why, and what would fix it.

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

## Scope

Every mode trades something, because a live Wayland window has exactly one buffer at one scale, and the pixels utsushot wants do not exist until a client renders them.

| mode | captures | true N×? | disruption | ceiling |
|---|---|---|---|---|
| `utsushot -- <cmd>` | a fresh app instance | yes, any factor | none | GPU texture size |
| `utsushot -w` | one live window | yes | desktop zooms briefly | client cooperation |
| `utsushot` (screen) | the live desktop | no, borrowed mode | black flash | monitor's best mode |
| compositor patch | the live desktop | yes, 2x proven | none | client cooperation |

A running window cannot be moved onto a phantom output (no protocol migrates a live surface between compositors), which is why the fresh-instance mode exists. The compositor patch row requires the patched niri in `docs/niri-supersample.patch`; everything else works on stock niri. Genuine virtual outputs (niri PR #3800, our [#10](https://github.com/xevrion/utsushot/issues/10)) would collapse most of these tradeoffs.

### Why not `grim -s 4`?

Because it upscales. The flag is applied after the frame has already been captured: grim resamples the buffer with pixman, so `grim -s 4` writes a 7680x4320 file that carries no more detail than the 1920x1080 one.

Measured on a 1920x1080 output: downscaling `grim -s 4` output back to native and comparing against a plain capture gives an RMSE of 0.018, meaning the two are 98% identical. The larger file is interpolation, not information. The man page wording ("Set the output image's scale factor") makes this easy to misread.

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
utsushot                             # capture the screen, to ~/Pictures/
utsushot --copy                      # same, straight to the clipboard
utsushot -w --scale 4                # focused window at TRUE 4x (stock niri)
utsushot --output-name HDMI-A-1       # a specific display
utsushot -- foot                     # run foot on a 4x phantom and capture that
utsushot --scale 8 -- $BROWSER        # same, at 8x
utsushot --list-backends              # what's supported, what's detected here
```

With no command, utsushot captures the display you are looking at. Naming a command after `--` runs it on a phantom output instead. `-w` captures the focused window at genuine N times density on stock niri: the window is floated with its logical size pinned while the output scale is briefly raised, so the client itself re-renders denser. No black flash, though the desktop visibly zooms for the settle duration. The approach was suggested by niri's maintainer in [discussion #4436](https://github.com/niri-wm/niri/discussions/4436).

The phantom is sized from your focused output, so on a 1920x1080 screen `--scale 4` renders into 7680x4320. Slow-starting applications may need a longer `--settle` than the 600ms default.

Exit codes: `0` success, `2` usage, `3` no backend, `4` backend unimplemented, `5` missing dependency, `6` capture failed, `7` restore failed.

## Backends

| Backend  | Status      | Approach |
|----------|-------------|----------|
| niri     | working     | Nested niri instance sized beyond the physical display ([details](docs/backends/niri.md)) |
| sway     | planned     | `swaymsg create_output` ([details](docs/backends/sway.md)) |
| hyprland | planned     | `hyprctl output create headless` ([details](docs/backends/hyprland.md)) |
| GNOME / KDE | researching | Likely the xdg-desktop-portal ScreenCast virtual source |
| X11      | not planned | No per-output scaling to exploit; see [docs/backends/x11.md](docs/backends/x11.md) |

niri cannot create outputs over IPC as of 26.04, and neither the pre-configured-output nor the portal VIRTUAL approach works there. The [niri backend doc](docs/backends/niri.md) records both dead ends and the nested-instance approach that replaced them.

## What's left

Roughly in order.

- [x] **#1 niri phantom output**: nested niri instance, sized past the physical display
- [x] **#2 niri target launch**: run the application inside the phantom and reap it afterwards
- [x] **#4 real output geometry**: phantom derived from the focused output's logical size
- [x] **#6 settle delay**: `--settle`, defaulting to 600ms, before capturing
- [ ] **#9 capture the live session**: reconfigure the real output instead of nesting, so your actual desktop can be captured
- [ ] **#10 virtual outputs in niri (upstream)**: the fix that makes both modes clean
- [ ] **#5 live integration tests**: assert captured dimensions and that a failed capture cleans up
- [ ] **#3 native capture**: wlr-screencopy via `wayland-client`, dropping the `grim` dependency
- [ ] **#7 sway backend**: `create_output` makes this the easy one
- [ ] **#8 hyprland backend**
- [ ] Publish `0.0.1` to crates.io to reserve the name

Contributions are very welcome, and a new backend is the most useful one. [CONTRIBUTING.md](CONTRIBUTING.md) has a walkthrough of the `Backend` trait.

## Support

If utsushot is useful to you, consider buying me a coffee or sponsoring on GitHub. A star on the repo helps others find it too.

[![Ko-fi](https://img.shields.io/badge/Ko--fi-support-ff5e5b?style=flat-square&logo=ko-fi)](https://ko-fi.com/xevrion)
[![GitHub Sponsors](https://img.shields.io/badge/GitHub-sponsor-ea4aaa?style=flat-square&logo=github-sponsors)](https://github.com/sponsors/xevrion)

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
