//! Bootstraps the wayland globals we depend on. River advertises
//! `river_xkb_bindings_v1` and `river_layer_shell_v1` only when it also
//! advertises `river_window_manager_v1`, so we bind all three at
//! startup; if any are missing, the compositor isn't a viable target.

use std::fmt;

use wayland_client::globals::{BindError, GlobalList};
use wayland_client::QueueHandle;

use crate::wayland_proto::{RiverLayerShellV1, RiverWindowManagerV1, RiverXkbBindingsV1};

use super::world::World;

/// Versions we'll bind. Clamped down to what we actually exercise so we
/// keep the protocol contract tight.
const WM_VERSION: std::ops::RangeInclusive<u32> = 1..=4;
const XKB_VERSION: std::ops::RangeInclusive<u32> = 1..=3;
const LAYER_SHELL_VERSION: std::ops::RangeInclusive<u32> = 1..=1;

pub struct Globals {
    pub manager: RiverWindowManagerV1,
    pub xkb: RiverXkbBindingsV1,
    /// Binding this global is what tells river "the WM supports layer
    /// shell, please don't auto-close layer surfaces". Required for
    /// waybar / tofi / launchers / panels.
    pub layer_shell: RiverLayerShellV1,
}

#[derive(Debug)]
pub enum GlobalError {
    Manager(BindError),
    Xkb(BindError),
    LayerShell(BindError),
}

impl fmt::Display for GlobalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manager(e) => {
                write!(f, "compositor does not expose river_window_manager_v1: {e}")
            }
            Self::Xkb(e) => {
                write!(f, "compositor does not expose river_xkb_bindings_v1: {e}")
            }
            Self::LayerShell(e) => {
                write!(f, "compositor does not expose river_layer_shell_v1: {e}")
            }
        }
    }
}

impl std::error::Error for GlobalError {}

pub fn bind_all(globals: &GlobalList, qh: &QueueHandle<World>) -> Result<Globals, GlobalError> {
    let manager: RiverWindowManagerV1 = globals
        .bind(qh, WM_VERSION, ())
        .map_err(GlobalError::Manager)?;
    let xkb: RiverXkbBindingsV1 = globals
        .bind(qh, XKB_VERSION, ())
        .map_err(GlobalError::Xkb)?;
    let layer_shell: RiverLayerShellV1 = globals
        .bind(qh, LAYER_SHELL_VERSION, ())
        .map_err(GlobalError::LayerShell)?;
    Ok(Globals { manager, xkb, layer_shell })
}
