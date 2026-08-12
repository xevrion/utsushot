// SPDX-License-Identifier: GPL-3.0-or-later

//! niri backend, built on a nested niri instance.
//!
//! niri cannot create outputs at runtime: its IPC only reconfigures outputs
//! that already exist. A headless backend does exist in the source, but it is
//! documented as being "for tests", leaves `import_dmabuf` unimplemented, and
//! is selected by a parameter that `main.rs` hardcodes to `false`, so no flag
//! or environment variable reaches it. What niri can do is run
//! as a nested Wayland client, and that nested instance gets an output whose
//! size is not limited by the physical display. Sizing that instance's window
//! to 5120x2880 with a configured scale of 4 gives exactly the phantom output
//! this project is built around, on hardware a quarter of its size.
//!
//! `docs/backends/niri.md` records the approaches that did not work and why.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::backend::{Backend, OutputId, RestoreToken};
use crate::error::Error;

/// The nested compositor's only output is always called this; it is the winit
/// backend's fixed name, not something we choose.
const PHANTOM_OUTPUT_NAME: &str = "winit";

/// How long to wait for the nested compositor to announce its socket.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for a launched application to map a window.
///
/// Generous because the phantom can be enormous: allocating and painting a
/// 12800x7200 surface takes a client noticeably longer than a normal window.
const APP_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest phantom dimension worth attempting.
///
/// GPUs cap texture size, very commonly at 16384, and the compositor needs
/// headroom below that for its own buffers. Measured on this hardware:
/// 12800x7200 works and 15360x8640 does not. Rejecting the request up front
/// gives a clear reason instead of a client that silently never appears.
const MAX_PHANTOM_DIMENSION: u32 = 13000;

#[derive(Debug)]
pub struct NiriBackend {
    /// Command to run inside the phantom, and the settle delay before capture.
    app: Vec<String>,
    settle: Duration,
    /// Learned from the nested instance's log once it starts.
    nested: Option<NestedHandle>,
    config_path: Option<PathBuf>,
    /// The nested compositor between `create_phantom` and `move_target`, after
    /// which it belongs to the `RestoreToken` so the guard is responsible for
    /// reaping it.
    pending_child: Option<Child>,
}

#[derive(Debug, Clone)]
struct NestedHandle {
    /// Value for `WAYLAND_DISPLAY` when talking to the nested compositor.
    display: String,
    /// Value for `NIRI_SOCKET` when sending it IPC.
    socket: String,
}

impl NiriBackend {
    #[must_use]
    pub fn new(app: Vec<String>, settle: Duration) -> Self {
        Self {
            app,
            settle,
            nested: None,
            config_path: None,
            pending_child: None,
        }
    }

    /// Removes the generated config directory, ignoring failures.
    fn discard_config(&mut self) {
        if let Some(path) = self.config_path.take() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    /// Runs a `niri msg` command against the host session.
    fn host_msg(args: &[&str]) -> Result<String, Error> {
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

    /// Finds the host window belonging to our nested instance.
    ///
    /// Matched by pid rather than by `app_id`, because every niri window shares
    /// the `app_id` "niri" and grabbing the wrong one would resize the user's
    /// real compositor window.
    ///
    /// Polls, because niri announces its Wayland socket before its window has
    /// been mapped in the host session; querying immediately finds nothing.
    fn find_window_by_pid(pid: u32) -> Result<u64, Error> {
        let deadline = Instant::now() + STARTUP_TIMEOUT;

        loop {
            let json = Self::host_msg(&["-j", "windows"])?;
            let windows: serde_json::Value = serde_json::from_str(&json)
                .map_err(|e| Error::Ipc(format!("could not parse niri windows: {e}")))?;

            let found = windows
                .as_array()
                .into_iter()
                .flatten()
                .find(|w| w.get("pid").and_then(serde_json::Value::as_u64) == Some(u64::from(pid)))
                .and_then(|w| w.get("id").and_then(serde_json::Value::as_u64));

            if let Some(id) = found {
                return Ok(id);
            }
            if Instant::now() > deadline {
                return Err(Error::Ipc(format!(
                    "the nested niri window (pid {pid}) never appeared in the host session"
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Waits for the nested compositor to report its display and IPC socket.
    ///
    /// Both are printed to its log at startup. Parsing them is more reliable
    /// than guessing the next free `wayland-N`, which races with anything else
    /// starting a compositor at the same time.
    ///
    /// The reading happens on a thread that keeps draining stderr for the rest
    /// of the run. niri logs continuously, so if nobody drains the pipe it
    /// fills, and the compositor blocks on its own logging and never maps a
    /// window.
    fn await_startup(child: &mut Child) -> Result<NestedHandle, Error> {
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Ipc("nested niri produced no stderr".into()))?;

        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let (mut display, mut socket) = (None, None);
            let mut tx = Some(tx);

            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                tracing::trace!(target: "nested-niri", "{line}");

                if let Some(rest) = line.split("listening on Wayland socket: ").nth(1) {
                    display = Some(rest.trim().to_string());
                }
                if let Some(rest) = line.split("IPC listening on: ").nth(1) {
                    socket = Some(rest.trim().to_string());
                }
                if let (Some(d), Some(s)) = (&display, &socket) {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(NestedHandle {
                            display: d.clone(),
                            socket: s.clone(),
                        });
                    }
                }
                // Keep draining after reporting, so the pipe never fills.
            }
        });

        rx.recv_timeout(STARTUP_TIMEOUT).map_err(|_| {
            Error::Ipc(
                "nested niri did not announce its socket in time; run with -vv to see its output"
                    .into(),
            )
        })
    }

    /// Waits until the nested compositor reports at least one window.
    fn await_window_on(handle: &NestedHandle, app: &[String]) -> Result<(), Error> {
        let deadline = Instant::now() + APP_TIMEOUT;

        while Instant::now() < deadline {
            let out = Command::new("niri")
                .env("NIRI_SOCKET", &handle.socket)
                .args(["msg", "-j", "windows"])
                .output()
                .map_err(|e| Error::Ipc(format!("could not query nested niri: {e}")))?;

            if out.status.success() {
                let parsed: serde_json::Value = serde_json::from_slice(&out.stdout)
                    .map_err(|e| Error::Ipc(format!("could not parse nested windows: {e}")))?;
                if parsed.as_array().is_some_and(|w| !w.is_empty()) {
                    tracing::debug!("target window mapped");
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }

        Err(Error::Capture(format!(
            "'{}' did not open a window within {}s",
            app.join(" "),
            APP_TIMEOUT.as_secs()
        )))
    }
}

impl Backend for NiriBackend {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn available() -> bool {
        std::env::var_os("NIRI_SOCKET").is_some_and(|v| !v.is_empty())
    }

    fn create_phantom(&mut self, w: u32, h: u32, scale: f64) -> Result<OutputId, Error> {
        if w > MAX_PHANTOM_DIMENSION || h > MAX_PHANTOM_DIMENSION {
            return Err(Error::Usage(format!(
                "a {w}x{h} phantom exceeds what GPUs can allocate (limit here is about \
                 {MAX_PHANTOM_DIMENSION} per side); try a smaller --scale"
            )));
        }
        if self.app.is_empty() {
            return Err(Error::Usage(
                "this backend runs an application on a phantom output, so it needs a command: \
                 utsushot -- <command>. Run utsushot with no command to capture the screen \
                 instead."
                    .into(),
            ));
        }

        // The winit backend ignores the configured mode and takes its size from
        // its host window instead, so `mode` here is only a hint. The scale is
        // the part that matters: it is what makes clients re-render at N times
        // the density rather than simply filling a larger buffer.
        let config = format!(
            "output \"{PHANTOM_OUTPUT_NAME}\" {{\n    \
                 mode \"{w}x{h}\"\n    \
                 scale {scale}\n\
             }}\n\
             hotkey-overlay {{ skip-at-startup; }}\n\
             // Written by utsushot; safe to delete.\n"
        );

        let dir = std::env::temp_dir().join(format!("utsushot-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let config_path = dir.join("phantom.kdl");
        std::fs::write(&config_path, config)?;
        self.config_path = Some(config_path.clone());

        tracing::debug!("starting nested niri with {}", config_path.display());
        let mut child = Command::new("niri")
            .arg("-c")
            .arg(&config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Ipc(format!("could not start nested niri: {e}")))?;

        // Everything up to the end of create_phantom runs before the guard
        // exists, so a failure here has to reap the child itself or leave an
        // invisible compositor running and a config directory behind.
        let result = Self::await_startup(&mut child).and_then(|handle| {
            tracing::debug!("nested niri on {} ({})", handle.display, handle.socket);

            // Size the nested output by resizing its window in the host
            // session; the winit backend takes its output size from there.
            let id = Self::find_window_by_pid(child.id())?.to_string();
            Self::host_msg(&["action", "move-window-to-floating", "--id", &id])?;
            Self::host_msg(&["action", "set-window-width", "--id", &id, &w.to_string()])?;
            Self::host_msg(&["action", "set-window-height", "--id", &id, &h.to_string()])?;
            Ok(handle)
        });

        let handle = match result {
            Ok(handle) => handle,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                self.discard_config();
                return Err(e);
            }
        };

        self.nested = Some(handle.clone());
        // The child is handed to the guard so it is reaped even on a panic.
        self.pending_child = Some(child);

        Ok(OutputId::on_display(PHANTOM_OUTPUT_NAME, handle.display))
    }

    fn move_target(&mut self, out: &OutputId) -> Result<RestoreToken, Error> {
        let handle = self
            .nested
            .clone()
            .ok_or_else(|| Error::Ipc("phantom output was not created".into()))?;

        // The nested compositor moves into the token immediately, so from here
        // on every exit path carries it. A failure below returns the token by
        // way of `cleanup`, which is what stops a failed launch from orphaning
        // an invisible compositor.
        let mut restore = RestoreToken::default();
        if let Some(nested) = self.pending_child.take() {
            restore.children.push(nested);
        }

        let mut attempt = || -> Result<(), Error> {
            let (program, args) = self.app.split_first().ok_or_else(|| {
                Error::Usage("no application given to run inside the phantom".into())
            })?;

            tracing::info!("launching {} inside the phantom output", self.app.join(" "));
            let app = Command::new(program)
                .args(args)
                .env(
                    "WAYLAND_DISPLAY",
                    out.display.as_deref().unwrap_or(&handle.display),
                )
                // A nested compositor is a Wayland-only environment; leaving the
                // host DISPLAY set makes toolkits pick X11 and render unscaled.
                .env_remove("DISPLAY")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| Error::Capture(format!("could not run '{program}': {e}")))?;
            restore.children.push(app);

            Self::await_window_on(&handle, &self.app)?;

            // Even once mapped, clients need a moment to finish painting at the
            // new scale. Capturing immediately yields a half-drawn frame (#6).
            tracing::debug!(
                "waiting {}ms for the target to settle",
                self.settle.as_millis()
            );
            std::thread::sleep(self.settle);
            Ok(())
        };

        match attempt() {
            Ok(()) => Ok(restore),
            Err(e) => {
                // Reap what we started rather than leaking it back to the caller.
                self.cleanup(out.clone(), restore)?;
                Err(e)
            }
        }
    }

    fn capture(&self, out: &OutputId, path: &Path) -> Result<(), Error> {
        let display = out
            .display
            .as_deref()
            .ok_or_else(|| Error::Capture("phantom output has no Wayland display".into()))?;

        crate::capture::grim_capture_on(display, out.as_str(), path)
    }

    fn cleanup(&mut self, _out: OutputId, mut restore: RestoreToken) -> Result<(), Error> {
        // Best effort throughout: this runs from a Drop impl, possibly while a
        // panic unwinds, so every step must be attempted even if an earlier one
        // failed. Children are reaped in reverse order so the application goes
        // before the compositor hosting it.
        for mut child in restore.children.drain(..).rev() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Reached when create_phantom succeeded but move_target never ran, so
        // the nested compositor was never handed over to the token.
        if let Some(mut child) = self.pending_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        self.discard_config();
        self.nested = None;

        tracing::debug!("phantom torn down");
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

    fn backend() -> NiriBackend {
        NiriBackend::new(vec!["true".into()], Duration::from_millis(0))
    }

    #[test]
    fn name_is_stable() {
        assert_eq!(backend().name(), "niri");
    }

    #[test]
    fn an_empty_command_is_a_usage_error() {
        let err = NiriBackend::new(Vec::new(), Duration::ZERO)
            .create_phantom(5120, 2880, 4.0)
            .expect_err("no application should be rejected");
        assert_eq!(err.exit_code(), crate::error::exit::USAGE);
    }

    #[test]
    fn cleanup_succeeds_with_nothing_to_clean() {
        // cleanup runs from Drop even when create_phantom failed early, so it
        // has to tolerate a backend that never got started.
        let mut b = backend();
        assert!(b
            .cleanup(OutputId::new(PHANTOM_OUTPUT_NAME), RestoreToken::default())
            .is_ok());
    }

    #[test]
    fn capture_requires_a_display() {
        // An OutputId with no display means the phantom was never really set
        // up; grim would otherwise silently target the user's real screen.
        let err = backend()
            .capture(&OutputId::new(PHANTOM_OUTPUT_NAME), Path::new("/dev/null"))
            .expect_err("a display-less output should not be captured");
        assert!(err.to_string().contains("no Wayland display"));
    }

    #[test]
    fn startup_lines_are_parsed_from_the_log() {
        // Guards against a niri log format change silently breaking discovery.
        let line = "INFO niri: listening on Wayland socket: wayland-2";
        assert_eq!(
            line.split("listening on Wayland socket: ").nth(1),
            Some("wayland-2")
        );

        let line = "INFO niri: IPC listening on: /run/user/1000/niri.wayland-2.172687.sock";
        assert_eq!(
            line.split("IPC listening on: ").nth(1),
            Some("/run/user/1000/niri.wayland-2.172687.sock")
        );
    }
}
