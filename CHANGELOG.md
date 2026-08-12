# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
