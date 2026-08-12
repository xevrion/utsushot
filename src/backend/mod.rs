// SPDX-License-Identifier: GPL-3.0-or-later

//! Compositor backends and the guard that keeps the user's session intact.

use std::path::Path;

use crate::error::Error;

pub mod hyprland;
pub mod niri;
pub mod sway;

/// Identifies a phantom output for the lifetime of one capture.
///
/// The name is what the capture tool addresses. `display` is set when the
/// phantom lives on a different Wayland display than the session we were
/// launched from, as with niri's nested instance, where every command has to be
/// aimed at that display rather than the user's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputId {
    pub name: String,
    pub display: Option<String>,
}

impl OutputId {
    #[must_use]
    #[allow(
        dead_code,
        reason = "for backends whose phantom is on the session display (#7, #8)"
    )]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display: None,
        }
    }

    /// An output on a nested compositor reachable at `display`.
    #[must_use]
    pub fn on_display(name: impl Into<String>, display: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display: Some(display.into()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

/// Whatever a backend needs to put the session back how it found it.
///
/// Deliberately opaque, because the backends do genuinely different things. The
/// niri backend spawns processes and needs to reap them; a backend that moves a
/// window on the live session needs to remember where it came from instead.
#[derive(Debug, Default)]
pub struct RestoreToken {
    /// Processes to terminate, most recently spawned first.
    pub children: Vec<std::process::Child>,
    /// Workspace the target lived on before it was moved to the phantom.
    #[allow(dead_code, reason = "used by backends that move live windows (#7, #8)")]
    pub origin_workspace: Option<u64>,
    /// Window the capture targeted, if a single window was moved.
    #[allow(dead_code, reason = "used by backends that move live windows (#7, #8)")]
    pub window_id: Option<u64>,
}

pub trait Backend {
    fn name(&self) -> &'static str;

    /// Whether this backend can drive the session we are currently in.
    fn available() -> bool
    where
        Self: Sized;

    fn create_phantom(&mut self, w: u32, h: u32, scale: f64) -> Result<OutputId, Error>;

    fn move_target(&mut self, out: &OutputId) -> Result<RestoreToken, Error>;

    fn capture(&self, out: &OutputId, path: &Path) -> Result<(), Error>;

    fn cleanup(&mut self, out: OutputId, restore: RestoreToken) -> Result<(), Error>;
}

/// Runs `cleanup` on drop, including while a panic unwinds.
///
/// The whole point of the type: a capture that fails partway must not leave the
/// user looking at an empty screen with their windows parked on a 5120x2880
/// output they cannot see. Everything between `create_phantom` and the end of
/// the capture happens with one of these alive.
pub struct PhantomGuard<'a, B: Backend + ?Sized> {
    backend: &'a mut B,
    /// Both are taken together by whichever of `disarm` or `drop` runs first,
    /// so cleanup happens exactly once.
    output: Option<OutputId>,
    restore: Option<RestoreToken>,
}

impl<'a, B: Backend + ?Sized> PhantomGuard<'a, B> {
    pub fn new(backend: &'a mut B, output: OutputId, restore: RestoreToken) -> Self {
        Self {
            backend,
            output: Some(output),
            restore: Some(restore),
        }
    }

    #[must_use]
    #[allow(
        dead_code,
        reason = "part of the guard API; used once capture takes the guard"
    )]
    pub fn output(&self) -> Option<&OutputId> {
        self.output.as_ref()
    }

    #[must_use]
    pub fn backend(&self) -> &B {
        self.backend
    }

    /// Cleans up and surfaces the error instead of swallowing it.
    ///
    /// Prefer this on the success path; the `Drop` impl exists for the paths
    /// that never reach it.
    pub fn disarm(mut self) -> Result<(), Error> {
        match (self.output.take(), self.restore.take()) {
            (Some(out), Some(restore)) => self.backend.cleanup(out, restore),
            _ => Ok(()),
        }
    }
}

impl<B: Backend + ?Sized> Drop for PhantomGuard<'_, B> {
    fn drop(&mut self) {
        if let (Some(out), Some(restore)) = (self.output.take(), self.restore.take()) {
            if let Err(e) = self.backend.cleanup(out, restore) {
                // A panic here would abort the process mid-unwind and guarantee
                // the stranded state we are trying to avoid. Log and move on.
                tracing::error!("failed to restore session after capture: {e}");
            }
        }
    }
}

impl<B: Backend + ?Sized> std::fmt::Debug for PhantomGuard<'_, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhantomGuard")
            .field("output", &self.output)
            .field("restore", &self.restore)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "panicking is how a test reports failure"
)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Spy<'c> {
        cleaned: &'c Cell<bool>,
    }

    impl Backend for Spy<'_> {
        fn name(&self) -> &'static str {
            "spy"
        }
        fn available() -> bool {
            true
        }
        fn create_phantom(&mut self, _w: u32, _h: u32, _scale: f64) -> Result<OutputId, Error> {
            Ok(OutputId::new("spy-0"))
        }
        fn move_target(&mut self, _out: &OutputId) -> Result<RestoreToken, Error> {
            Ok(RestoreToken::default())
        }
        fn capture(&self, _out: &OutputId, _path: &Path) -> Result<(), Error> {
            Ok(())
        }
        fn cleanup(&mut self, _out: OutputId, _restore: RestoreToken) -> Result<(), Error> {
            self.cleaned.set(true);
            Ok(())
        }
    }

    #[test]
    fn guard_cleans_up_when_dropped() {
        let cleaned = Cell::new(false);
        let mut spy = Spy { cleaned: &cleaned };
        drop(PhantomGuard::new(
            &mut spy,
            OutputId::new("spy-0"),
            RestoreToken::default(),
        ));
        assert!(cleaned.get(), "drop must restore the session");
    }

    #[test]
    fn guard_cleans_up_exactly_once_when_disarmed() {
        let cleaned = Cell::new(false);
        let mut spy = Spy { cleaned: &cleaned };
        let guard = PhantomGuard::new(&mut spy, OutputId::new("spy-0"), RestoreToken::default());
        guard.disarm().expect("cleanup should succeed");
        assert!(cleaned.get());
    }

    #[test]
    fn guard_cleans_up_while_unwinding() {
        let cleaned = Cell::new(false);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut spy = Spy { cleaned: &cleaned };
            let _guard =
                PhantomGuard::new(&mut spy, OutputId::new("spy-0"), RestoreToken::default());
            panic!("capture blew up");
        }));
        assert!(result.is_err());
        assert!(
            cleaned.get(),
            "a panicking capture must still restore the session"
        );
    }
}
