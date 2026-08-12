// SPDX-License-Identifier: GPL-3.0-or-later

//! Hyprland backend (planned).
//!
//! Hyprland can create headless outputs at runtime with
//! `hyprctl output create headless`, so it should land shortly after sway.
//! See `docs/backends/hyprland.md`.

use std::path::Path;

use crate::backend::{Backend, OutputId, RestoreToken};
use crate::error::Error;

#[derive(Debug, Default)]
pub struct HyprlandBackend;

impl HyprlandBackend {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

fn planned() -> Error {
    Error::Unimplemented(
        "hyprland",
        "backend planned, contributions welcome. The path is \
         `hyprctl output create headless`; see docs/backends/hyprland.md and CONTRIBUTING.md."
            .into(),
    )
}

impl Backend for HyprlandBackend {
    fn name(&self) -> &'static str {
        "hyprland"
    }

    fn available() -> bool {
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|v| !v.is_empty())
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
        assert!(msg.contains("headless"));
    }
}
