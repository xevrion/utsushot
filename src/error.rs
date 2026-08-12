// SPDX-License-Identifier: GPL-3.0-or-later

//! Error type and the exit codes it maps to.

use thiserror::Error;

/// Process exit codes. Documented in README and `--help`; treat them as API.
pub mod exit {
    // Part of the documented contract, but emitted by std rather than by our
    // own error mapping.
    #[allow(
        dead_code,
        reason = "documented contract; std::process::ExitCode emits this"
    )]
    pub const SUCCESS: i32 = 0;
    /// Also what clap exits with on bad flags.
    pub const USAGE: i32 = 2;
    pub const NO_BACKEND: i32 = 3;
    pub const BACKEND_UNIMPLEMENTED: i32 = 4;
    pub const MISSING_DEPENDENCY: i32 = 5;
    pub const CAPTURE_FAILED: i32 = 6;
    pub const RESTORE_FAILED: i32 = 7;
}

#[derive(Debug, Error)]
pub enum Error {
    /// Bad input that clap could not catch, such as a backend name that parses
    /// as a string but names no backend we have.
    #[error("{0}")]
    Usage(String),

    #[error("no supported compositor detected (checked niri, sway, hyprland)")]
    NoBackend,

    /// A graphical session utsushot cannot work with at all, rather than a
    /// Wayland session whose compositor we merely lack a backend for.
    #[error("{0}")]
    NoSession(String),

    #[error("backend '{0}' is not implemented yet: {1}")]
    Unimplemented(&'static str, String),

    #[error("required program '{0}' not found in PATH: {1}")]
    MissingDependency(&'static str, String),

    #[error("no phantom output configured: {0}")]
    #[allow(
        dead_code,
        reason = "constructed once niri config detection lands (#1)"
    )]
    NoPhantomOutput(String),

    #[error("compositor IPC failed: {0}")]
    #[allow(
        dead_code,
        reason = "constructed once a backend actually talks to a compositor"
    )]
    Ipc(String),

    #[error("capture failed: {0}")]
    Capture(String),

    #[error("failed to restore session: {0}")]
    Restore(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => exit::USAGE,
            Self::NoBackend | Self::NoSession(_) => exit::NO_BACKEND,
            Self::Unimplemented(..) => exit::BACKEND_UNIMPLEMENTED,
            Self::MissingDependency(..) => exit::MISSING_DEPENDENCY,
            Self::Restore(_) => exit::RESTORE_FAILED,
            _ => exit::CAPTURE_FAILED,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The codes are documented in README and `--help`, so changing one is a
    /// breaking change for anyone scripting around utsushot.
    #[test]
    fn exit_codes_match_the_documented_contract() {
        let cases = [
            (Error::Usage(String::new()), 2),
            (Error::NoBackend, 3),
            (Error::NoSession(String::new()), 3),
            (Error::Unimplemented("niri", String::new()), 4),
            (Error::MissingDependency("grim", String::new()), 5),
            (Error::Capture(String::new()), 6),
            (Error::Ipc(String::new()), 6),
            (Error::Restore(String::new()), 7),
        ];
        for (err, code) in cases {
            assert_eq!(err.exit_code(), code, "wrong exit code for {err:?}");
        }
    }

    #[test]
    fn every_code_fits_in_a_u8() {
        // main() converts with u8::try_from; a code above 255 would silently
        // become 1 instead of what the docs promise.
        for code in [
            exit::SUCCESS,
            exit::USAGE,
            exit::NO_BACKEND,
            exit::BACKEND_UNIMPLEMENTED,
            exit::MISSING_DEPENDENCY,
            exit::CAPTURE_FAILED,
            exit::RESTORE_FAILED,
        ] {
            assert!(
                u8::try_from(code).is_ok(),
                "exit code {code} does not fit in u8"
            );
        }
    }
}
