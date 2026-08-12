// SPDX-License-Identifier: GPL-3.0-or-later

//! Capturing the live desktop by switching the output to a larger mode.
//!
//! Unlike the nested backend, this photographs the session you are actually
//! looking at. The cost is that it reconfigures a real display: the screen
//! blanks, windows reflow, and everything has to be put back afterwards.
//!
//! Two findings from testing on niri 26.04 shape this module.
//!
//! Raising the *scale* does not help. `scale 2` on a 1920x1080 output gives a
//! 960x540 logical desktop inside the same 1920x1080 framebuffer, so the
//! capture is unchanged and the desktop merely gets coarser. Only a larger
//! *mode* produces more real pixels.
//!
//! A panel silently refuses modes it cannot drive. Asking a 1920x1080 laptop
//! display for 3840x2160 returns success and changes nothing, so the result
//! has to be read back rather than assumed.

use std::path::Path;
use std::process::Command;

use crate::error::Error;

/// A mode the compositor reported for an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    /// Millihertz, as niri reports it.
    pub refresh: u32,
}

impl Mode {
    /// Formats for `niri msg output <name> mode`.
    ///
    /// The refresh rate is included and printed to three decimals because niri
    /// matches modes exactly: asking for `2560x1440@144` when the mode is
    /// 144.001 Hz silently lands on the 60 Hz mode instead.
    #[must_use]
    pub fn as_arg(self) -> String {
        format!(
            "{}x{}@{:.3}",
            self.width,
            self.height,
            f64::from(self.refresh) / 1000.0
        )
    }

    #[must_use]
    pub fn pixels(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// Everything needed to put an output back exactly as it was.
#[derive(Debug, Clone)]
pub struct OutputState {
    pub name: String,
    pub mode: Mode,
    pub scale: f64,
}

/// Restores an output on drop, so a panic or an early return cannot leave the
/// user's display in a mode they did not choose.
#[derive(Debug)]
pub struct ModeGuard {
    original: OutputState,
    armed: bool,
}

impl ModeGuard {
    #[must_use]
    pub fn new(original: OutputState) -> Self {
        Self {
            original,
            armed: true,
        }
    }

    /// Restores now and reports failure, rather than logging it from `Drop`.
    pub fn restore(mut self) -> Result<(), Error> {
        self.armed = false;
        restore_output(&self.original)
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        if self.armed {
            if let Err(e) = restore_output(&self.original) {
                tracing::error!(
                    "could not restore {}: {e}. Run: niri msg output {} mode {}",
                    self.original.name,
                    self.original.name,
                    self.original.mode.as_arg()
                );
            }
        }
    }
}

fn restore_output(state: &OutputState) -> Result<(), Error> {
    // Both are attempted even if the first fails; a restored mode with a wrong
    // scale is still far better than leaving the display oversized.
    let mode = niri(&["output", &state.name, "mode", &state.mode.as_arg()]);
    let scale = niri(&["output", &state.name, "scale", &state.scale.to_string()]);
    mode.and(scale).map(|_| ())
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

fn outputs_json() -> Result<serde_json::Value, Error> {
    let raw = niri(&["-j", "outputs"])?;
    serde_json::from_str(&raw).map_err(|e| Error::Ipc(format!("could not parse outputs: {e}")))
}

/// Reads an output's current mode and scale.
pub fn current_state(name: &str) -> Result<OutputState, Error> {
    let outputs = outputs_json()?;
    let out = outputs
        .get(name)
        .ok_or_else(|| Error::Usage(format!("no output named '{name}'")))?;

    let idx = out
        .get("current_mode")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| Error::Ipc(format!("output '{name}' has no current mode")))?;

    let mode = out
        .get("modes")
        .and_then(|m| m.get(usize::try_from(idx).unwrap_or(0)))
        .and_then(parse_mode)
        .ok_or_else(|| Error::Ipc(format!("could not read the current mode of '{name}'")))?;

    let scale = out
        .get("logical")
        .and_then(|l| l.get("scale"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1.0);

    Ok(OutputState {
        name: name.to_string(),
        mode,
        scale,
    })
}

fn parse_mode(v: &serde_json::Value) -> Option<Mode> {
    let field = |k: &str| v.get(k).and_then(serde_json::Value::as_u64);
    Some(Mode {
        width: u32::try_from(field("width")?).ok()?,
        height: u32::try_from(field("height")?).ok()?,
        refresh: u32::try_from(field("refresh_rate")?).ok()?,
    })
}

/// The largest mode an output advertises, preferring higher refresh on ties.
///
/// Only modes the compositor already lists are considered. A custom mode beyond
/// what the panel reports is silently ignored by the hardware, so offering one
/// would promise resolution that never arrives.
pub fn best_mode(name: &str) -> Result<Mode, Error> {
    let outputs = outputs_json()?;
    let out = outputs
        .get(name)
        .ok_or_else(|| Error::Usage(format!("no output named '{name}'")))?;

    let current = current_state(name)?.mode;

    out.get("modes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_mode)
        // Only modes with the current aspect ratio, because the scale that
        // preserves the layout is a single number: a mode of a different shape
        // would stretch the desktop instead of sharpening it.
        .filter(|m| m.width * current.height == current.width * m.height)
        .max_by_key(|m| (m.pixels(), m.refresh))
        .ok_or_else(|| Error::Ipc(format!("output '{name}' advertises no usable modes")))
}

pub fn focused_output() -> Result<String, Error> {
    let raw = niri(&["-j", "focused-output"])?;
    let v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| Error::Ipc(format!("could not parse focused-output: {e}")))?;

    v.get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::Ipc("focused output has no name".into()))
}

/// Where a supersampling-capable compositor reads the requested factor from.
///
/// A file rather than an environment variable, because the compositor's
/// environment is fixed when the session starts and the factor has to be
/// chosen per capture.
fn supersample_request_path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|d| std::path::PathBuf::from(d).join("niri-screenshot-supersample"))
}

/// Asks the compositor to render the screenshot itself, supersampled.
///
/// This is the approach that does what utsushot actually wants. niri's
/// `Niri::screenshot` calls `render_to_vec(renderer, size, scale, ..)`, so
/// multiplying both arguments renders the live desktop into an offscreen
/// texture N times larger. The display is never reconfigured, so there is no
/// modeset, no blanking, and no ceiling imposed by what the panel can show.
///
/// Stock niri 26.04 ignores the request file and produces an output-sized
/// screenshot; the caller detects that from the resulting image and falls back
/// to switching modes. The 16-line compositor patch that enables this lives in
/// `docs/niri-supersample.patch`.
fn try_compositor_supersample(
    path: &Path,
    factor: f64,
    settle: std::time::Duration,
) -> Result<(), Error> {
    let request = supersample_request_path()
        .ok_or_else(|| Error::Ipc("XDG_RUNTIME_DIR is not set".into()))?;

    // "<factor> <settle_ms>": the settle is how long the compositor gives
    // clients to re-render at the boosted scale before capturing.
    std::fs::write(&request, format!("{factor} {}", settle.as_millis()))?;
    // Removed however this returns, so a stale factor cannot silently apply to
    // somebody else's screenshot later.
    let _cleanup = RequestFileGuard(request);

    let out = Command::new("niri")
        .args(["msg", "action", "screenshot-screen"])
        .args(["--write-to-disk", "true"])
        .arg("--path")
        .arg(path)
        .output()
        .map_err(|e| Error::Ipc(format!("could not run niri msg: {e}")))?;

    if !out.status.success() {
        return Err(Error::Capture(format!(
            "niri screenshot failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    // The action returns as soon as the compositor accepts it; the capture
    // itself happens after the settle delay and the PNG is encoded on another
    // thread, so the file appears well after the command exits.
    let deadline = std::time::Instant::now() + settle + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if png_size(path).is_some() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(Error::Capture(
        "the compositor accepted the screenshot but never wrote the file".into(),
    ))
}

/// Reads the dimensions out of a PNG header.
///
/// Used to tell whether the compositor actually honoured a supersample
/// request: a stock build writes an output-sized image and reports success, so
/// the returned file is the only evidence of what really happened.
pub fn png_size(path: &Path) -> Option<(u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    // 8-byte signature, then the IHDR chunk whose width and height are the
    // first two big-endian u32s of its data at offsets 16 and 20.
    if bytes.len() < 24 || &bytes[1..4] != b"PNG" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

struct RequestFileGuard(std::path::PathBuf);

impl Drop for RequestFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Captures the live desktop, supersampled if the compositor can do it.
///
/// Two strategies, best first. If the compositor renders screenshots at a
/// requested factor, nothing about the display changes and any factor works.
/// Otherwise the output is switched to its largest mode for the capture, which
/// blanks the screen briefly and is limited to what the panel advertises.
pub fn capture(
    name: &str,
    path: &Path,
    settle: std::time::Duration,
    supersample: f64,
) -> Result<Mode, Error> {
    let original = current_state(name)?;

    // `screenshot-screen` always captures the focused output, so this path can
    // only serve a request for that same output. Asking for another one has to
    // fall through, or we would return a picture of the wrong screen.
    let is_focused = focused_output().is_ok_and(|f| f == name);

    if supersample > 1.0 && is_focused {
        match try_compositor_supersample(path, supersample, settle) {
            Ok(()) => match png_size(path) {
                // Honoured only if the image is larger than the output itself;
                // an equal-sized one means the request was ignored.
                Some((w, h)) if u64::from(w) * u64::from(h) > original.mode.pixels() => {
                    tracing::info!("compositor rendered {w}x{h} without touching the display");
                    return Ok(Mode {
                        width: w,
                        height: h,
                        refresh: original.mode.refresh,
                    });
                }
                Some((w, h)) => {
                    // Stock niri wrote an output-sized image. Remove it, or the
                    // fallback below could be skipped and this plausible but
                    // un-supersampled file would be reported as success.
                    tracing::debug!(
                        "compositor ignored the request and wrote {w}x{h}; falling back"
                    );
                    let _ = std::fs::remove_file(path);
                }
                None => tracing::debug!("could not read the captured image; falling back"),
            },
            Err(e) => tracing::debug!("compositor screenshot unavailable ({e}); falling back"),
        }
    }

    let best = best_mode(name)?;

    tracing::debug!(
        "{name}: current {}x{}, best available {}x{}",
        original.mode.width,
        original.mode.height,
        best.width,
        best.height
    );

    if best.pixels() <= original.mode.pixels() {
        // Nothing to gain, so do not disturb the display at all.
        tracing::info!(
            "{name} is already at its highest resolution ({}x{}); capturing as-is",
            original.mode.width,
            original.mode.height
        );
        crate::capture::grim_capture(name, path)?;
        return Ok(original.mode);
    }

    // The scale has to rise with the mode, or the desktop simply grows: windows
    // keep their pixel sizes and end up occupying a smaller fraction of a
    // larger canvas, which is a differently-laid-out desktop rather than a
    // sharper one. Holding logical size fixed is what makes this supersampling.
    let factor = f64::from(best.width) / f64::from(original.mode.width);
    let scale = original.scale * factor;

    tracing::info!(
        "switching {name} to {}x{} @ scale {scale} (logical stays {}x{})",
        best.width,
        best.height,
        original.mode.width,
        original.mode.height
    );

    niri(&["output", name, "mode", &best.as_arg()])?;
    // Armed immediately: from here on, every exit path restores the display.
    let guard = ModeGuard::new(original.clone());
    niri(&["output", name, "scale", &scale.to_string()])?;

    // The compositor needs a moment to apply the mode, and clients need longer
    // to reflow into it. Capturing early catches a half-drawn desktop.
    std::thread::sleep(settle);

    let applied = current_state(name)?.mode;
    if applied.pixels() < best.pixels() {
        // The panel refused. niri reports success either way, so this is the
        // only way to find out.
        tracing::warn!(
            "{name} refused {}x{} and stayed at {}x{}",
            best.width,
            best.height,
            applied.width,
            applied.height
        );
    }

    let result = crate::capture::grim_capture(name, path);
    guard.restore()?;
    result?;

    Ok(applied)
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
    fn mode_arg_keeps_three_decimals() {
        // niri matches exactly: "2560x1440@144" lands on the 60Hz mode when the
        // real rate is 144.001, which is a bug this format prevents.
        let m = Mode {
            width: 2560,
            height: 1440,
            refresh: 144_001,
        };
        assert_eq!(m.as_arg(), "2560x1440@144.001");
    }

    #[test]
    fn mode_arg_handles_whole_rates() {
        let m = Mode {
            width: 1920,
            height: 1080,
            refresh: 60_000,
        };
        assert_eq!(m.as_arg(), "1920x1080@60.000");
    }

    #[test]
    fn pixels_multiplies_out() {
        assert_eq!(
            Mode {
                width: 3840,
                height: 2160,
                refresh: 60_000
            }
            .pixels(),
            8_294_400
        );
    }

    #[test]
    fn parse_mode_reads_niri_json() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"width":2560,"height":1440,"refresh_rate":144001}"#).unwrap();
        assert_eq!(
            parse_mode(&v),
            Some(Mode {
                width: 2560,
                height: 1440,
                refresh: 144_001
            })
        );
    }

    #[test]
    fn parse_mode_rejects_incomplete_json() {
        let v: serde_json::Value = serde_json::from_str(r#"{"width":2560}"#).unwrap();
        assert_eq!(parse_mode(&v), None);
    }

    #[test]
    fn best_mode_prefers_area_then_refresh() {
        let modes: Vec<Mode> = serde_json::from_str::<serde_json::Value>(
            r#"[{"width":1920,"height":1080,"refresh_rate":144000},
                {"width":3840,"height":2160,"refresh_rate":30000},
                {"width":2560,"height":1440,"refresh_rate":144001}]"#,
        )
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(parse_mode)
        .collect();

        let best = modes
            .iter()
            .copied()
            .max_by_key(|m| (m.pixels(), m.refresh))
            .unwrap();
        // 4K at 30Hz beats 1440p at 144Hz: pixels are what this tool is for.
        assert_eq!(best.width, 3840);
    }

    #[test]
    fn best_mode_breaks_ties_on_refresh() {
        let a = Mode {
            width: 2560,
            height: 1440,
            refresh: 60_000,
        };
        let b = Mode {
            width: 2560,
            height: 1440,
            refresh: 144_001,
        };
        assert_eq!(
            [a, b].into_iter().max_by_key(|m| (m.pixels(), m.refresh)),
            Some(b)
        );
    }
}
