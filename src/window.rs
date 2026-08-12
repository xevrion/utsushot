// SPDX-License-Identifier: GPL-3.0-or-later

//! True supersampled captures of a single live window, on stock niri.
//!
//! The recipe was suggested by niri's maintainer in discussion #4436: output
//! *scale* changes need no modeset (so no black flash), floating windows keep
//! their logical size across scale changes and may exceed the output, and
//! `screenshot-window` renders the window at its full buffer density. So:
//! float the window, pin its logical size, raise the output scale, let the
//! client re-render, capture the window, restore everything.
//!
//! Two facts learned the hard way. The size pin must happen *before* the scale
//! boost: resizing during it fights the reconfigure and gets clamped. And the
//! visible desktop does zoom and reflow for the settle duration, because the
//! output's logical size shrinks; there is no black flash, but this mode is
//! not invisible. The capture itself shows the window at its normal logical
//! size, only denser.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::error::Error;

/// Everything needed to put the window and output back.
#[derive(Debug, Clone)]
struct WindowState {
    id: u64,
    was_floating: bool,
    width: u32,
    height: u32,
    output: String,
    output_scale: f64,
}

fn niri(args: &[&str]) -> Result<String, Error> {
    let out = Command::new("niri")
        .arg("msg")
        .args(args)
        .output()
        .map_err(|e| Error::Ipc(format!("could not run niri msg: {e}")))?;
    if !out.status.success() {
        return Err(Error::Ipc(format!(
            "niri msg {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn focused_window() -> Result<serde_json::Value, Error> {
    let raw = niri(&["-j", "focused-window"])?;
    serde_json::from_str(&raw).map_err(|e| Error::Ipc(format!("parsing focused-window: {e}")))
}

fn window_by_id(id: u64) -> Result<serde_json::Value, Error> {
    let raw = niri(&["-j", "windows"])?;
    let windows: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| Error::Ipc(format!("parsing windows: {e}")))?;
    windows
        .as_array()
        .into_iter()
        .flatten()
        .find(|w| w.get("id").and_then(serde_json::Value::as_u64) == Some(id))
        .cloned()
        .ok_or_else(|| Error::Usage(format!("no window with id {id}")))
}

fn read_state(win: &serde_json::Value) -> Result<WindowState, Error> {
    let id = win
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| Error::Ipc("window has no id".into()))?;
    let was_floating = win
        .get("is_floating")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let size = win
        .get("layout")
        .and_then(|l| l.get("window_size"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::Ipc("window has no size".into()))?;
    let dim = |i: usize| {
        size.get(i)
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| Error::Ipc("window size malformed".into()))
    };

    // The window's workspace tells us which output it is on.
    let ws_id = win.get("workspace_id").and_then(serde_json::Value::as_u64);
    let raw = niri(&["-j", "workspaces"])?;
    let workspaces: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| Error::Ipc(format!("parsing workspaces: {e}")))?;
    let output = workspaces
        .as_array()
        .into_iter()
        .flatten()
        .find(|w| w.get("id").and_then(serde_json::Value::as_u64) == ws_id)
        .and_then(|w| w.get("output").and_then(serde_json::Value::as_str))
        .ok_or_else(|| Error::Ipc("could not determine the window's output".into()))?
        .to_string();

    let output_scale = crate::live::current_state(&output)?.scale;

    Ok(WindowState {
        id,
        was_floating,
        width: dim(0)?,
        height: dim(1)?,
        output,
        output_scale,
    })
}

/// Restores scale and tiling on drop, so a failed capture cannot leave the
/// desktop zoomed with a stray floating window.
struct WindowGuard {
    state: WindowState,
    armed: bool,
}

impl WindowGuard {
    fn restore_inner(state: &WindowState) -> Result<(), Error> {
        // Scale first: the visible zoom is the worst part to leave behind.
        let scale = niri(&[
            "output",
            &state.output,
            "scale",
            &state.output_scale.to_string(),
        ]);
        let tile = if state.was_floating {
            Ok(String::new())
        } else {
            niri(&[
                "action",
                "move-window-to-tiling",
                "--id",
                &state.id.to_string(),
            ])
        };
        scale.and(tile).map(|_| ())
    }

    fn restore(mut self) -> Result<(), Error> {
        self.armed = false;
        Self::restore_inner(&self.state)
    }
}

impl Drop for WindowGuard {
    fn drop(&mut self) {
        if self.armed {
            if let Err(e) = Self::restore_inner(&self.state) {
                tracing::error!(
                    "could not restore after window capture: {e}. Run: niri msg output {} scale {}",
                    self.state.output,
                    self.state.output_scale
                );
            }
        }
    }
}

/// Captures one live window at `factor` times its density.
pub fn capture(
    id: Option<u64>,
    path: &Path,
    factor: f64,
    settle: Duration,
) -> Result<(u32, u32), Error> {
    let win = match id {
        Some(id) => window_by_id(id)?,
        None => focused_window()?,
    };
    let state = read_state(&win)?;
    let id_arg = state.id.to_string();

    if !state.was_floating {
        // Floating is what lets the window keep its logical size when the
        // output's logical size shrinks under the boosted scale.
        niri(&["action", "move-window-to-floating", "--id", &id_arg])?;
    }
    // Pin the size before touching the scale; doing it after fights the
    // client's reconfigure and loses.
    niri(&[
        "action",
        "set-window-width",
        "--id",
        &id_arg,
        &state.width.to_string(),
    ])?;
    niri(&[
        "action",
        "set-window-height",
        "--id",
        &id_arg,
        &state.height.to_string(),
    ])?;

    let boosted = state.output_scale * factor;
    tracing::info!(
        "boosting {} to scale {boosted} for a {factor}x capture of window {}",
        state.output,
        state.id
    );
    niri(&["output", &state.output, "scale", &boosted.to_string()])?;
    let guard = WindowGuard {
        state: state.clone(),
        armed: true,
    };

    // The client needs real time to re-render at the new density.
    std::thread::sleep(settle);

    let capture = niri(&[
        "action",
        "screenshot-window",
        "--id",
        &id_arg,
        "--write-to-disk",
        "true",
        "--path",
        &path.display().to_string(),
    ]);

    // niri writes the file asynchronously (issue #2664 upstream).
    let mut size = None;
    if capture.is_ok() {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if let Some(s) = crate::live::png_size(path) {
                size = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    guard.restore()?;
    capture?;
    size.ok_or_else(|| {
        Error::Capture("the compositor accepted the capture but never wrote the file".into())
    })
}
