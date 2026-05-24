//! `World` is the wayland-thread state. Owned exclusively by the event
//! loop; never wrapped in an Arc/Mutex. Subsystems (windows, outputs,
//! seats, bindings, modes, tags) live as fields here.

use std::collections::VecDeque;

use wayland_client::backend::ObjectId;
use wayland_client::QueueHandle;

use crate::action::Action;
use crate::state::Shared;

use super::bindings::Bindings;
use super::globals::Globals;
use super::modes::Modes;
use super::outputs::Outputs;
use super::seats::Seats;
use super::sequence::Sequence;
use super::tags::Tags;
use super::windows::Windows;

pub struct World {
    pub shared: Shared,
    pub globals: Globals,
    pub qh: QueueHandle<World>,
    pub sequence: Sequence,
    pub windows: Windows,
    pub outputs: Outputs,
    pub seats: Seats,
    pub bindings: Bindings,
    pub modes: Modes,
    pub tags: Tags,
    /// Actions sent from the IPC thread that need to be applied during
    /// the next manage sequence. Drained at the top of `manage_start`.
    pub pending_actions: VecDeque<Action>,
    /// Window IDs that arrived since the last manage sequence and need
    /// initial keyboard focus. Drained in `manage_start`.
    pub pending_focus: Vec<ObjectId>,
    running: bool,
}

impl World {
    pub fn new(shared: Shared, globals: Globals, qh: QueueHandle<World>) -> Self {
        Self {
            shared,
            globals,
            qh,
            sequence: Sequence::new(),
            windows: Windows::default(),
            outputs: Outputs::default(),
            seats: Seats::default(),
            bindings: Bindings::default(),
            modes: Modes::default(),
            tags: Tags::default(),
            pending_actions: VecDeque::new(),
            pending_focus: Vec::new(),
            running: true,
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn shutdown(&mut self) {
        self.running = false;
    }
}
