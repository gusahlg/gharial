//! Active binding-mode tracking. Modes are just named buckets — a
//! binding's `mode` field decides which bucket it belongs to, and only
//! bindings in the active bucket receive `enable` from the compositor.
//!
//! The starting mode is `"default"`; that name is conventional and
//! treated as the fallback when a mode is "exited".
//!
//! `Modes` also tracks which named modes the user has any bindings for.
//! Entering an unknown mode is a soft-lock hazard — every key would be
//! disabled and there'd be no way out — so callers can consult
//! `Modes::is_known` to refuse the transition with a useful error
//! message before the wm becomes unresponsive.

use std::collections::HashSet;

pub const DEFAULT_MODE: &str = "default";

#[derive(Debug)]
pub struct Modes {
    pub active: String,
    /// Modes the user has at least one binding for. `default` is always
    /// present so `mode exit` is never refused.
    pub known: HashSet<String>,
}

impl Default for Modes {
    fn default() -> Self {
        let mut known = HashSet::new();
        known.insert(DEFAULT_MODE.to_string());
        Self {
            active: DEFAULT_MODE.into(),
            known,
        }
    }
}

impl Modes {
    /// Register `mode` as a mode the user can enter — called whenever a
    /// binding is installed. Idempotent.
    pub fn register(&mut self, mode: &str) {
        if !self.known.contains(mode) {
            self.known.insert(mode.to_string());
        }
    }

    /// `true` if the mode has at least one binding registered (or is
    /// the always-present `default`). The caller uses this to refuse
    /// `EnterMode("typo")` rather than soft-lock the keyboard.
    pub fn is_known(&self, mode: &str) -> bool {
        self.known.contains(mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_always_known() {
        let m = Modes::default();
        assert!(m.is_known(DEFAULT_MODE));
        assert!(!m.is_known("tile_ratio"));
    }

    #[test]
    fn register_makes_a_mode_known() {
        let mut m = Modes::default();
        m.register("tile_ratio");
        assert!(m.is_known("tile_ratio"));
    }

    #[test]
    fn register_is_idempotent() {
        let mut m = Modes::default();
        m.register("tile_ratio");
        m.register("tile_ratio");
        assert!(m.is_known("tile_ratio"));
        // Default was already there; double-register doesn't expand.
        assert_eq!(m.known.len(), 2);
    }

    #[test]
    fn typoed_mode_is_not_known() {
        let mut m = Modes::default();
        m.register("tile_ratio");
        assert!(!m.is_known("tile-ratio")); // dash vs underscore
        assert!(!m.is_known("TILE_RATIO")); // case-sensitive
    }
}
