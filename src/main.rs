// SPDX-License-Identifier: GPL-3.0-or-later

//! utsushot: supersampled screenshots via a temporary phantom output.

mod backend;
mod capture;
mod cli;
mod detect;
mod error;

use clap::Parser;

use backend::{Backend, PhantomGuard};
use cli::Cli;
use detect::{BackendKind, SessionKind, SystemEnv};
use error::Error;

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(cli.log_level())
        .with_target(false)
        .without_time()
        .init();

    match run(&cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            // Exit codes are all small positives; u8::try_from documents that
            // rather than asserting it with a cast.
            std::process::ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(1))
        }
    }
}

fn run(cli: &Cli) -> Result<(), Error> {
    let env = SystemEnv;

    if cli.list_backends {
        list_backends(&env);
        return Ok(());
    }

    let kind = select_backend(cli, &env)?;

    // Autodetection already proved reachability; an explicit --backend has not,
    // so check it here rather than failing later with an opaque IPC error.
    let (mut backend, reachable): (Box<dyn Backend>, bool) = match kind {
        BackendKind::Niri => (
            Box::new(backend::niri::NiriBackend::new()),
            backend::niri::NiriBackend::available(),
        ),
        BackendKind::Sway => (
            Box::new(backend::sway::SwayBackend::new()),
            backend::sway::SwayBackend::available(),
        ),
        BackendKind::Hyprland => (
            Box::new(backend::hyprland::HyprlandBackend::new()),
            backend::hyprland::HyprlandBackend::available(),
        ),
    };

    if !reachable {
        return Err(Error::Usage(format!(
            "backend '{}' was requested but its compositor is not running here",
            backend.name()
        )));
    }
    tracing::info!("using backend: {}", backend.name());

    // Fail before touching the session, not after the workspace has moved.
    capture::require(
        "grim",
        "install grim (e.g. `dnf install grim`, `apt install grim`)",
    )?;

    let output_path = cli.resolve_output();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // TODO(#4): read the physical output geometry from the compositor instead of
    // assuming; the phantom dimensions must derive from the real screen.
    let (width, height) = phantom_size(1920, 1080, cli.scale);
    tracing::debug!("phantom output: {width}x{height} @ scale {}", cli.scale);

    let phantom = backend.create_phantom(width, height, cli.scale)?;
    let restore = backend.move_target(&phantom)?;

    // From here on the session is modified, so everything runs under the guard.
    let guard = PhantomGuard::new(backend.as_mut(), phantom.clone(), restore);
    let result = guard.backend().capture(&phantom, &output_path);
    guard.disarm().map_err(|e| Error::Restore(e.to_string()))?;
    result?;

    tracing::info!("wrote {}", output_path.display());

    if cli.copy {
        capture::copy_to_clipboard(&output_path)?;
    }
    if cli.notify {
        capture::notify(&output_path);
    }

    Ok(())
}

/// Phantom dimensions for a physical screen at a given supersampling factor.
fn phantom_size(w: u32, h: u32, scale: f64) -> (u32, u32) {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let scaled = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (scaled(w), scaled(h))
}

fn select_backend(cli: &Cli, env: &impl detect::Env) -> Result<BackendKind, Error> {
    if let Some(name) = &cli.backend {
        return BackendKind::parse(name).ok_or_else(|| {
            Error::Usage(format!(
                "unknown backend '{name}'; try one of: niri, sway, hyprland"
            ))
        });
    }

    let candidates = detect::candidates(env);
    for candidate in &candidates {
        tracing::debug!("candidate {}: {}", candidate.kind, candidate.reason);
    }

    candidates
        .first()
        .map(|c| c.kind)
        .ok_or_else(|| match detect::session_kind(env) {
            SessionKind::X11 => Error::NoSession(
                "this looks like an X11 session. utsushot needs a Wayland compositor that can \
                 re-render at a higher scale; X11 has no per-output scaling to exploit."
                    .into(),
            ),
            SessionKind::None => Error::NoSession(
                "no graphical session detected (neither WAYLAND_DISPLAY nor DISPLAY is set)".into(),
            ),
            SessionKind::Wayland => Error::NoBackend,
        })
}

fn list_backends(env: &impl detect::Env) {
    let detected: Vec<BackendKind> = detect::candidates(env)
        .into_iter()
        .map(|c| c.kind)
        .collect();

    println!("{:<12} {:<14} DETECTED HERE", "BACKEND", "STATUS");
    for kind in BackendKind::all() {
        let status = if kind.is_implemented() {
            "in progress"
        } else {
            "planned"
        };
        let here = if detected.contains(&kind) {
            "yes"
        } else {
            "no"
        };
        println!("{:<12} {status:<14} {here}", kind.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phantom_size_multiplies_both_axes() {
        assert_eq!(phantom_size(1280, 720, 4.0), (5120, 2880));
        assert_eq!(phantom_size(1920, 1080, 2.0), (3840, 2160));
    }

    #[test]
    fn phantom_size_at_unit_scale_is_identity() {
        assert_eq!(phantom_size(1920, 1080, 1.0), (1920, 1080));
    }

    #[test]
    fn phantom_size_rounds_fractional_scales() {
        assert_eq!(phantom_size(1280, 720, 1.5), (1920, 1080));
        // 1281 * 2.5 = 3202.5, rounds to 3203 rather than truncating to 3202.
        assert_eq!(phantom_size(1281, 100, 2.5), (3203, 250));
    }

    #[test]
    fn phantom_size_never_returns_zero() {
        // A zero-width output would be rejected by the compositor with a far
        // less helpful message than anything we could print.
        assert_eq!(phantom_size(0, 0, 4.0), (1, 1));
    }
}
