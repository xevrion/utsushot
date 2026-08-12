// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests that need a real compositor session.
//!
//! These move windows around and enable outputs, so they are opt-in: they would
//! disrupt anyone running `cargo test` on a working desktop, and they cannot
//! pass in CI at all. Run them with:
//!
//! ```sh
//! UTSUSHOT_LIVE_TESTS=1 cargo test --test live_compositor
//! ```

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "panicking is how a test reports failure"
)]

use std::process::Command;

/// Skips the test unless the opt-in variable is set.
macro_rules! require_live {
    () => {
        if std::env::var_os("UTSUSHOT_LIVE_TESTS").is_none() {
            eprintln!("skipping: set UTSUSHOT_LIVE_TESTS=1 to run live compositor tests");
            return;
        }
    };
}

fn utsushot() -> Command {
    Command::new(env!("CARGO_BIN_EXE_utsushot"))
}

#[test]
fn list_backends_runs_without_a_compositor() {
    // The one case that is safe everywhere, including CI: --list-backends must
    // never touch the session.
    let out = utsushot()
        .arg("--list-backends")
        .output()
        .expect("binary should run");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for backend in ["niri", "sway", "hyprland"] {
        assert!(stdout.contains(backend), "missing {backend} in:\n{stdout}");
    }
}

#[test]
fn rejects_an_unknown_backend() {
    let out = utsushot()
        .args(["--backend", "kwin"])
        .output()
        .expect("binary should run");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown backend"));
}

#[test]
fn captures_at_the_requested_scale() {
    require_live!();
    // TODO(#5): once the niri backend lands, capture a known window and assert
    // the PNG is scale x the physical output size.
}

#[test]
fn session_is_restored_after_a_failed_capture() {
    require_live!();
    // TODO(#5): force a capture failure and assert via `niri msg outputs` that
    // the phantom output is disabled again and the workspace came back.
}
