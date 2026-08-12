# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Working niri backend. Runs a nested niri instance whose output is sized past
  the physical display, launches the target application into it, and captures
  it at true N× density. Verified producing 7680x4320 from a 1920x1080 screen.
- `--settle` to control how long to wait for the target to finish painting.
- Trailing `-- <command>` argument naming the application to capture.
- Phantom dimensions are now derived from the focused output rather than
  assumed.

### Notes

- The niri backend captures an application it launches, not the existing
  desktop. Wayland offers no way to move a live surface between compositors;
  see `docs/backends/niri.md` and issue #9.

### Previously

- Project scaffold: CLI, environment detection, backend trait, and error type
  with documented exit codes.
- `PhantomGuard`, a `Drop`-based guard that restores the session even if a
  capture panics.
- niri, sway, and Hyprland backend skeletons; all return a clear
  "not implemented" error rather than failing obscurely.
- Backend documentation under `docs/backends/`, recording that niri 26.04
  cannot create outputs over IPC and that its portal advertises VIRTUAL source
  support.
- Unit tests for detection and CLI parsing; opt-in integration test skeleton
  gated behind `UTSUSHOT_LIVE_TESTS`.

[Unreleased]: https://github.com/xevrion/utsushot/commits/main
