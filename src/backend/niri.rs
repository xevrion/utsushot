// SPDX-License-Identifier: GPL-3.0-or-later

//! niri backend.
//!
//! niri's IPC (as of 26.04) cannot create an output at runtime: `Request::Output`
//! only reconfigures outputs the compositor already knows about. So the phantom
//! output has to exist in the user's config as a disabled entry, which utsushot
//! then resizes, rescales, and enables around the capture. See
//! `docs/backends/niri.md` for the full findings.

use std::path::Path;

use crate::backend::{Backend, OutputId, RestoreToken};
use crate::error::Error;

/// Name of the output the user is expected to have declared as `off` in their
/// niri config. Matching on a fixed name keeps the config snippet copy-pasteable.
pub const PHANTOM_OUTPUT_NAME: &str = "utsushot-phantom";

#[derive(Debug, Default)]
pub struct NiriBackend {
    /// Mode the phantom output had before we touched it, so cleanup can put it
    /// back rather than assuming a default.
    previous: Option<PhantomState>,
}

#[derive(Debug, Clone)]
#[allow(
    dead_code,
    reason = "read by cleanup once the phantom is actually enabled (#1)"
)]
struct PhantomState {
    was_enabled: bool,
}

impl NiriBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Backend for NiriBackend {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn available() -> bool {
        std::env::var_os("NIRI_SOCKET").is_some_and(|v| !v.is_empty())
    }

    fn create_phantom(&mut self, _w: u32, _h: u32, _scale: f64) -> Result<OutputId, Error> {
        // TODO(#1): enumerate outputs via niri_ipc::Request::Outputs, find
        // PHANTOM_OUTPUT_NAME, then apply custom-mode + scale + on.
        let _ = &mut self.previous;
        Err(Error::Unimplemented(
            "niri",
            format!(
                "phantom output setup is not wired up yet. niri 26.04 cannot create \
                 outputs over IPC, so utsushot will require a disabled '{PHANTOM_OUTPUT_NAME}' \
                 output in your niri config. See docs/backends/niri.md and issue #1."
            ),
        ))
    }

    fn move_target(&mut self, _out: &OutputId) -> Result<RestoreToken, Error> {
        // TODO(#2): Action::MoveWindowToMonitor / MoveWorkspaceToMonitor, and
        // record the origin workspace in the returned token.
        Err(Error::Unimplemented(
            "niri",
            "moving the target is not implemented yet (issue #2)".into(),
        ))
    }

    fn capture(&self, out: &OutputId, path: &Path) -> Result<(), Error> {
        // TODO(#3): replace with native wlr-screencopy via wayland-client so the
        // grim runtime dependency goes away.
        crate::capture::grim_capture(out.as_str(), path)
    }

    fn cleanup(&mut self, _out: OutputId, _restore: RestoreToken) -> Result<(), Error> {
        // TODO(#1): disable the phantom output and move the target back. Must
        // stay best-effort: a failure here is logged, never propagated as a panic.
        self.previous = None;
        Ok(())
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

    #[test]
    fn name_is_stable() {
        assert_eq!(NiriBackend::new().name(), "niri");
    }

    #[test]
    fn unimplemented_error_points_at_the_docs() {
        let err = NiriBackend::new()
            .create_phantom(5120, 2880, 4.0)
            .expect_err("phantom creation is not implemented yet");
        let msg = err.to_string();
        assert!(
            msg.contains(PHANTOM_OUTPUT_NAME),
            "should name the output: {msg}"
        );
        assert!(
            msg.contains("docs/backends/niri.md"),
            "should point at docs: {msg}"
        );
    }

    #[test]
    fn cleanup_is_infallible_on_a_fresh_backend() {
        let mut backend = NiriBackend::new();
        assert!(backend
            .cleanup(
                OutputId(PHANTOM_OUTPUT_NAME.into()),
                RestoreToken::default()
            )
            .is_ok());
    }
}
