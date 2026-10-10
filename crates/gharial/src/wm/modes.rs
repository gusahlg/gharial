//! Active binding-mode tracking. Modes are just named buckets — a
//! binding's mode decides which bucket it belongs to, and only bindings
//! in the active bucket are enabled.
//!
//! The starting mode is `"default"`; `mode exit` returns there.

pub use gharial_ipc::command::DEFAULT_MODE;

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
