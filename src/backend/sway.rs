// SPDX-License-Identifier: GPL-3.0-or-later

//! sway backend (planned).
//!
//! Implementation path is known and short: sway supports headless outputs at
//! runtime via `swaymsg create_output`, which is exactly the primitive niri
//! lacks. See `docs/backends/sway.md`.

use std::path::Path;

use crate::backend::{Backend, OutputId, RestoreToken};
use crate::error::Error;

#[derive(Debug, Default)]
pub struct SwayBackend;

impl SwayBackend {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

fn planned() -> Error {
    Error::Unimplemented(
        "sway",
        "backend planned, contributions welcome. The path is swaymsg create_output; \
         see docs/backends/sway.md and CONTRIBUTING.md."
            .into(),
    )
}

impl Backend for SwayBackend {
    fn name(&self) -> &'static str {
        "sway"
    }

    fn available() -> bool {
        std::env::var_os("SWAYSOCK").is_some_and(|v| !v.is_empty())
    }

    fn create_phantom(&mut self, _w: u32, _h: u32, _scale: f64) -> Result<OutputId, Error> {
        Err(planned())
    }

    fn move_target(&mut self, _out: &OutputId) -> Result<RestoreToken, Error> {
        Err(planned())
    }

    fn capture(&self, _out: &OutputId, _path: &Path) -> Result<(), Error> {
        Err(planned())
    }

    fn cleanup(&mut self, _out: OutputId, _restore: RestoreToken) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_invites_contribution() {
        let msg = planned().to_string();
        assert!(msg.contains("contributions welcome"));
        assert!(msg.contains("create_output"));
    }
}
