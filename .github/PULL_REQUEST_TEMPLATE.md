## What this changes

<!-- What it does and why. Link the issue it closes, if any. -->

## How it was tested

<!-- Which compositor, and what you actually ran. "cargo test passes" alone is
     not enough for a backend change; say what you observed on screen. -->

## Checklist

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] New behaviour has tests
- [ ] `CHANGELOG.md` updated under `[Unreleased]`

For backend changes:

- [ ] Verified the session is restored when the capture *fails*, not only when it succeeds
- [ ] `cleanup` cannot panic and is safe to call after a partial `create_phantom`
- [ ] Docs under `docs/backends/` updated
