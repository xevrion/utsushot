// SPDX-License-Identifier: GPL-3.0-or-later

//! Command-line surface.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

/// Capture supersampled screenshots by re-rendering onto a phantom output.
///
/// Exit codes:
///   0  success
///   2  usage error (bad flags)
///   3  no supported compositor detected
///   4  backend recognised but not implemented yet
///   5  a required external program is missing
///   6  capture failed
///   7  capture succeeded but restoring the session failed
#[derive(Debug, Parser)]
#[command(name = "utsushot", version, about, long_about = None, verbatim_doc_comment)]
// Command-line flags are booleans by nature, and grouping them into an enum to
// satisfy the lint would make the parsed struct harder to use, not easier.
#[allow(clippy::struct_excessive_bools, reason = "each field is a CLI flag")]
pub struct Cli {
    /// Supersampling factor; the phantom output is this many times the
    /// physical resolution, at a matching `HiDPI` scale.
    #[arg(short, long, default_value_t = 4.0, value_parser = parse_scale)]
    pub scale: f64,

    /// Where to write the PNG. Defaults to
    /// ~/Pictures/utsushot_<timestamp>.png
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Copy the result to the clipboard (needs wl-copy).
    #[arg(short, long)]
    pub copy: bool,

    /// Send a desktop notification when done (needs notify-send).
    #[arg(short, long)]
    pub notify: bool,

    /// Force a backend instead of autodetecting.
    #[arg(short, long)]
    pub backend: Option<String>,

    /// List known backends and whether they are usable here, then exit.
    #[arg(long)]
    pub list_backends: bool,

    /// Milliseconds to wait after the target appears, before capturing.
    ///
    /// Clients need a moment to finish painting at the phantom's scale;
    /// capturing too early catches a half-drawn frame.
    #[arg(long, default_value_t = 600)]
    pub settle: u64,

    /// Capture the live desktop instead of running an application.
    ///
    /// Temporarily switches the output to the largest mode it advertises,
    /// captures, then restores. Your screen visibly changes while this happens,
    /// and the ceiling is whatever the monitor supports: a display whose best
    /// mode equals its current one gains nothing.
    #[arg(long)]
    pub live: bool,

    /// Output to capture in `--live` mode. Defaults to the focused one.
    #[arg(long, value_name = "NAME")]
    pub output_name: Option<String>,

    /// Verbose logging; repeat for more (-v debug, -vv trace).
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Application to run inside the phantom output, after `--`.
    ///
    /// The niri backend captures a fresh instance of this program rather than
    /// an existing window, because a running window cannot be moved between
    /// compositors. See docs/backends/niri.md.
    #[arg(trailing_var_arg = true, num_args = 0.., value_name = "COMMAND")]
    pub app: Vec<String>,
}

/// Rejects scales that cannot produce a sane phantom output.
///
/// The upper bound is not arbitrary: at 16x a 4K display would need a
/// 61440x34560 buffer, which will fail somewhere less legibly than here.
fn parse_scale(s: &str) -> Result<f64, String> {
    let v: f64 = s.parse().map_err(|_| format!("'{s}' is not a number"))?;
    if !v.is_finite() {
        return Err("scale must be a finite number".into());
    }
    if v < 1.0 {
        return Err(format!("scale must be at least 1.0, got {v}"));
    }
    if v > 16.0 {
        return Err(format!("scale must be at most 16.0, got {v}"));
    }
    Ok(v)
}

impl Cli {
    /// Resolves the output path, defaulting to a timestamped file in Pictures.
    #[must_use]
    pub fn resolve_output(&self) -> PathBuf {
        if let Some(path) = &self.output {
            return path.clone();
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let name = format!("utsushot_{stamp}.png");

        directories::UserDirs::new()
            .and_then(|dirs| dirs.picture_dir().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(name)
    }

    #[must_use]
    pub fn settle_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.settle)
    }

    #[must_use]
    pub fn log_level(&self) -> tracing::Level {
        match self.verbose {
            0 => tracing::Level::INFO,
            1 => tracing::Level::DEBUG,
            _ => tracing::Level::TRACE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("utsushot").chain(args.iter().copied()))
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn defaults() {
        let cli = parse(&[]);
        assert!((cli.scale - 4.0).abs() < f64::EPSILON);
        assert!(!cli.copy && !cli.notify && !cli.list_backends);
        assert_eq!(cli.verbose, 0);
        assert!(cli.output.is_none());
    }

    #[test]
    fn scale_accepts_floats() {
        assert!((parse(&["--scale", "2.5"]).scale - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn scale_rejects_out_of_range() {
        for bad in ["0.5", "0", "-3", "17", "nan", "inf", "abc"] {
            assert!(
                Cli::try_parse_from(["utsushot", "--scale", bad]).is_err(),
                "scale {bad} should be rejected"
            );
        }
    }

    #[test]
    fn scale_accepts_boundaries() {
        assert!((parse(&["--scale", "1"]).scale - 1.0).abs() < f64::EPSILON);
        assert!((parse(&["--scale", "16"]).scale - 16.0).abs() < f64::EPSILON);
    }

    #[test]
    fn explicit_output_is_used_verbatim() {
        let cli = parse(&["--output", "/tmp/shot.png"]);
        assert_eq!(cli.resolve_output(), PathBuf::from("/tmp/shot.png"));
    }

    #[test]
    fn default_output_is_a_timestamped_png() {
        let path = parse(&[]).resolve_output();
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("png"));
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        assert!(name.starts_with("utsushot_"), "unexpected name: {name}");
    }

    #[test]
    fn verbose_counts_up() {
        assert_eq!(parse(&["-v"]).log_level(), tracing::Level::DEBUG);
        assert_eq!(parse(&["-vv"]).log_level(), tracing::Level::TRACE);
        assert_eq!(parse(&[]).log_level(), tracing::Level::INFO);
    }

    #[test]
    fn short_flags_work() {
        let cli = parse(&["-c", "-n", "-b", "niri"]);
        assert!(cli.copy && cli.notify);
        assert_eq!(cli.backend.as_deref(), Some("niri"));
    }
}
