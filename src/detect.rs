// SPDX-License-Identifier: GPL-3.0-or-later

//! Works out which compositor we are talking to, in preference order.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BackendKind {
    Niri,
    Sway,
    Hyprland,
}

impl BackendKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Niri => "niri",
            Self::Sway => "sway",
            Self::Hyprland => "hyprland",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "niri" => Some(Self::Niri),
            "sway" => Some(Self::Sway),
            "hyprland" | "hypr" => Some(Self::Hyprland),
            _ => None,
        }
    }

    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Niri, Self::Sway, Self::Hyprland]
    }

    /// Whether this backend does anything beyond returning "not implemented".
    #[must_use]
    pub fn is_implemented(self) -> bool {
        matches!(self, Self::Niri)
    }
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a backend was suggested, so `--verbose` can explain itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub kind: BackendKind,
    pub reason: &'static str,
}

/// Read-only view of the environment, so tests do not mutate process globals.
pub trait Env {
    fn get(&self, key: &str) -> Option<String>;

    fn has(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| !v.is_empty())
    }
}

/// The real process environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemEnv;

impl Env for SystemEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Ordered candidate backends, most specific signal first.
///
/// Socket variables beat `XDG_CURRENT_DESKTOP` because a socket means the
/// compositor is actually reachable, whereas the desktop name is just a label a
/// session file set and is routinely wrong in nested or remote sessions.
pub fn candidates(env: &impl Env) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut push = |kind, reason| {
        if !out.iter().any(|c: &Candidate| c.kind == kind) {
            out.push(Candidate { kind, reason });
        }
    };

    if env.has("NIRI_SOCKET") {
        push(BackendKind::Niri, "NIRI_SOCKET is set");
    }
    if env.has("SWAYSOCK") {
        push(BackendKind::Sway, "SWAYSOCK is set");
    }
    if env.has("HYPRLAND_INSTANCE_SIGNATURE") {
        push(BackendKind::Hyprland, "HYPRLAND_INSTANCE_SIGNATURE is set");
    }

    // XDG_CURRENT_DESKTOP is colon-delimited ("niri:wlroots"), so match parts.
    if let Some(desktop) = env.get("XDG_CURRENT_DESKTOP") {
        for part in desktop.split(':') {
            if let Some(kind) = BackendKind::parse(part.trim()) {
                push(kind, "named in XDG_CURRENT_DESKTOP");
            }
        }
    }

    out
}

/// What kind of session we are in at all. Drives the error message when no
/// backend matches: X11 needs a different explanation than a bare TTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Wayland,
    X11,
    None,
}

pub fn session_kind(env: &impl Env) -> SessionKind {
    if env.has("WAYLAND_DISPLAY") {
        SessionKind::Wayland
    } else if env.has("DISPLAY") {
        SessionKind::X11
    } else {
        SessionKind::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeEnv(HashMap<String, String>);

    impl FakeEnv {
        fn with(pairs: &[(&str, &str)]) -> Self {
            Self(
                pairs
                    .iter()
                    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                    .collect(),
            )
        }
    }

    impl Env for FakeEnv {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    fn kinds(env: &FakeEnv) -> Vec<BackendKind> {
        candidates(env).into_iter().map(|c| c.kind).collect()
    }

    #[test]
    fn empty_env_yields_no_candidates() {
        assert!(candidates(&FakeEnv::default()).is_empty());
    }

    #[test]
    fn niri_socket_detected() {
        let env = FakeEnv::with(&[("NIRI_SOCKET", "/run/user/1000/niri.sock")]);
        assert_eq!(kinds(&env), vec![BackendKind::Niri]);
    }

    #[test]
    fn sway_and_hyprland_sockets_detected() {
        let env = FakeEnv::with(&[("SWAYSOCK", "/run/sway.sock")]);
        assert_eq!(kinds(&env), vec![BackendKind::Sway]);

        let env = FakeEnv::with(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc123")]);
        assert_eq!(kinds(&env), vec![BackendKind::Hyprland]);
    }

    #[test]
    fn empty_string_is_not_a_signal() {
        // Shells export empty vars readily; treating "" as present would make
        // us confidently pick a compositor that is not running.
        let env = FakeEnv::with(&[("NIRI_SOCKET", "")]);
        assert!(candidates(&env).is_empty());
    }

    #[test]
    fn socket_ranks_above_desktop_name() {
        let env = FakeEnv::with(&[
            ("XDG_CURRENT_DESKTOP", "Hyprland"),
            ("NIRI_SOCKET", "/run/user/1000/niri.sock"),
        ]);
        assert_eq!(kinds(&env), vec![BackendKind::Niri, BackendKind::Hyprland]);
    }

    #[test]
    fn colon_delimited_desktop_is_split() {
        let env = FakeEnv::with(&[("XDG_CURRENT_DESKTOP", "niri:wlroots")]);
        assert_eq!(kinds(&env), vec![BackendKind::Niri]);
    }

    #[test]
    fn desktop_name_matching_is_case_insensitive() {
        let env = FakeEnv::with(&[("XDG_CURRENT_DESKTOP", "Hyprland")]);
        assert_eq!(kinds(&env), vec![BackendKind::Hyprland]);
    }

    #[test]
    fn no_duplicate_when_socket_and_desktop_agree() {
        let env = FakeEnv::with(&[
            ("NIRI_SOCKET", "/run/niri.sock"),
            ("XDG_CURRENT_DESKTOP", "niri"),
        ]);
        assert_eq!(kinds(&env), vec![BackendKind::Niri]);
    }

    #[test]
    fn unknown_desktop_is_ignored() {
        let env = FakeEnv::with(&[("XDG_CURRENT_DESKTOP", "GNOME")]);
        assert!(candidates(&env).is_empty());
    }

    #[test]
    fn session_kinds() {
        assert_eq!(
            session_kind(&FakeEnv::with(&[("WAYLAND_DISPLAY", "wayland-1")])),
            SessionKind::Wayland
        );
        assert_eq!(
            session_kind(&FakeEnv::with(&[("DISPLAY", ":0")])),
            SessionKind::X11
        );
        assert_eq!(session_kind(&FakeEnv::default()), SessionKind::None);
    }

    #[test]
    fn wayland_wins_over_x11_when_both_set() {
        // XWayland sets DISPLAY inside a Wayland session; the Wayland socket is
        // the one that reflects what the compositor actually is.
        let env = FakeEnv::with(&[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")]);
        assert_eq!(session_kind(&env), SessionKind::Wayland);
    }

    #[test]
    fn parse_roundtrips_all_kinds() {
        for kind in BackendKind::all() {
            assert_eq!(BackendKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(BackendKind::parse("kwin"), None);
    }
}
