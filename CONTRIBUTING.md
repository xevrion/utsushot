# Contributing to utsushot

The most useful contribution right now is a new compositor backend. The trait is
small and deliberately so, and this document walks through implementing one.

## Development setup

### rustup

```sh
rustup toolchain install stable
cargo build
cargo test
```

MSRV is 1.75. Runtime tools you will want: `grim`, `wl-clipboard`, and the
compositor you are targeting.

### Nix

```sh
nix develop     # rust toolchain, rust-analyzer, cargo-deny, grim, wl-clipboard
nix build       # build the package
nix flake check # fmt + clippy
```

## Writing a backend

Backends live in `src/backend/`. Start by copying `sway.rs`, which is a stub
whose shape is already correct, and read `docs/backends/<your-compositor>.md` if
one exists; the sway and Hyprland docs already record the commands to use.

### 1. The trait

```rust
pub trait Backend {
    fn name(&self) -> &'static str;
    fn available() -> bool where Self: Sized;
    fn create_phantom(&mut self, w: u32, h: u32, scale: f64) -> Result<OutputId, Error>;
    fn move_target(&mut self, out: &OutputId) -> Result<RestoreToken, Error>;
    fn capture(&self, out: &OutputId, path: &Path) -> Result<(), Error>;
    fn cleanup(&mut self, out: OutputId, restore: RestoreToken) -> Result<(), Error>;
}
```

`available()` should be cheap and non-destructive, normally just an environment
variable check. It runs before anything else and must not talk to the
compositor.

`create_phantom` gets dimensions already multiplied by the scale factor. Your
job is to produce an output of that size whose *scale* is also set, because the
scale is what makes toolkits re-render rather than just handing you a bigger
viewport. Getting the resolution right and the scale wrong yields a large blurry
image and is the easiest mistake to make here.

Position the phantom far outside the physical layout (`10000 0` works) so it
does not disturb the visible arrangement.

`move_target` returns a `RestoreToken` carrying whatever `cleanup` needs to undo
the move. Put the origin workspace or output in it. Do not stash that state in a
global.

`cleanup` runs from a `Drop` impl, including while a panic unwinds. That imposes
two rules: it must never panic, and it must be safe to call when
`create_phantom` only partly succeeded. Prefer best-effort restoration of each
step over bailing on the first error.

### 2. Cleanup safety

`PhantomGuard` in `src/backend/mod.rs` owns this. Once you have created a
phantom, wrap the backend in a guard and let it handle restoration:

```rust
let guard = PhantomGuard::new(backend.as_mut(), phantom.clone(), restore);
let result = guard.backend().capture(&phantom, &output_path);
guard.disarm()?;   // surfaces restore errors on the success path
result?;
```

`disarm()` returns the cleanup error instead of swallowing it. `Drop` logs it,
because there is nowhere to return it to. Tests in `backend/mod.rs` cover all
three paths, including the panicking one; if you change the guard, keep them
passing.

### 3. Wire it up

Add your `BackendKind` variant in `src/detect.rs`, its detection signal in
`candidates()`, and a match arm in `select_backend()` in `main.rs`. Mark it in
`is_implemented()` once it genuinely works. Add unit tests for detection: they
use a `FakeEnv` fixture, so they need no compositor.

### 4. Test it live

```sh
UTSUSHOT_LIVE_TESTS=1 cargo test --test live_compositor
```

These are opt-in because they move real windows around. Verify by hand too, and
in particular verify the ugly paths: kill utsushot mid-capture with SIGINT and
confirm your session comes back. A backend that works when everything succeeds
but strands the user on a failure is worse than no backend.

## Commits

Conventional Commits, single line:

```
feat(sway): implement phantom output via create_output
fix(niri): restore workspace when capture fails
docs: note portal VIRTUAL support on niri
```

Keep the subject under ~72 characters. Do not add tool-attribution trailers.

## PR checklist

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] New behaviour has tests; detection changes have `FakeEnv` fixtures
- [ ] Cleanup verified on the failure path, not only the happy path
- [ ] Backend docs updated under `docs/backends/`
- [ ] `CHANGELOG.md` entry under `[Unreleased]`

## Releasing

Maintainers only. Tag `v*` and push; `release.yml` builds static musl binaries,
attaches them to the GitHub release, and publishes to crates.io.

That last step needs a `CARGO_REGISTRY_TOKEN` repository secret. Create a token
at <https://crates.io/settings/tokens> scoped to `publish-update`, then add it
under Settings → Secrets and variables → Actions → New repository secret, named
exactly `CARGO_REGISTRY_TOKEN`.
