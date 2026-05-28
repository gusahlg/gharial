//! Active binding-mode tracking. Modes are just named buckets — a
//! binding's `mode` field decides which bucket it belongs to, and only
//! bindings in the active bucket receive `enable` from the compositor.
//!
//! The starting mode is `"default"`; that name is conventional and
//! treated as the fallback when a mode is "exited".

pub const DEFAULT_MODE: &str = "default";

#[derive(Debug)]
pub struct Modes {
    pub active: String,
}

impl Default for Modes {
    fn default() -> Self {
        Self {
            active: DEFAULT_MODE.into(),
        }
    }
}
