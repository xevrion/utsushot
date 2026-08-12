// SPDX-License-Identifier: GPL-3.0-or-later

//! Shelling out to the external tools v0.1 relies on.

use std::path::Path;
use std::process::Command;

use crate::error::Error;

/// Checks a program exists before we build a session state that depends on it.
///
/// Worth doing up front: discovering grim is missing *after* the workspace has
/// moved to the phantom output turns a clear error into a confusing one.
pub fn require(program: &'static str, install_hint: &str) -> Result<(), Error> {
    which(program)
        .then_some(())
        .ok_or_else(|| Error::MissingDependency(program, install_hint.to_string()))
}

fn which(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

/// Captures one output with grim.
///
/// TODO(#3): replace with native wlr-screencopy through wayland-client, which
/// drops the runtime dependency and lets us read the buffer without a temp file.
pub fn grim_capture(output: &str, path: &Path) -> Result<(), Error> {
    require(
        "grim",
        "install grim (e.g. `dnf install grim`, `apt install grim`)",
    )?;

    let status = Command::new("grim")
        .arg("-o")
        .arg(output)
        .arg(path)
        .status()
        .map_err(|e| Error::Capture(format!("could not run grim: {e}")))?;

    if !status.success() {
        return Err(Error::Capture(format!("grim exited with {status}")));
    }
    Ok(())
}

/// Copies a PNG to the Wayland clipboard.
pub fn copy_to_clipboard(path: &Path) -> Result<(), Error> {
    require("wl-copy", "install wl-clipboard")?;

    let file = std::fs::File::open(path)?;
    let status = Command::new("wl-copy")
        .arg("--type")
        .arg("image/png")
        .stdin(file)
        .status()
        .map_err(|e| Error::Capture(format!("could not run wl-copy: {e}")))?;

    if !status.success() {
        return Err(Error::Capture(format!("wl-copy exited with {status}")));
    }
    Ok(())
}

/// Best-effort desktop notification. A failed notification must not fail a
/// capture that already wrote a good file to disk.
pub fn notify(path: &Path) {
    if !which("notify-send") {
        tracing::debug!("notify-send not found, skipping notification");
        return;
    }
    let result = Command::new("notify-send")
        .arg("--app-name=utsushot")
        .arg("Screenshot captured")
        .arg(path.display().to_string())
        .status();

    if let Err(e) = result {
        tracing::warn!("notification failed: {e}");
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
    fn which_finds_a_program_that_certainly_exists() {
        assert!(which("sh"), "sh should be on PATH");
    }

    #[test]
    fn which_rejects_nonsense() {
        assert!(!which("utsushot-definitely-not-a-real-program"));
    }

    #[test]
    fn require_reports_the_program_and_hint() {
        let err = require("utsushot-nope", "install it somehow")
            .expect_err("missing program should be an error");
        let msg = err.to_string();
        assert!(msg.contains("utsushot-nope"));
        assert!(msg.contains("install it somehow"));
    }
}
