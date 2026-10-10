//! `World` is the wayland-thread state. Owned exclusively by the event
//! loop; never wrapped in an Arc/Mutex. The IPC side reaches it only
//! through [`crate::bridge`] envelopes, which the loop hands to
//! [`super::commands::receive`].

use std::collections::{HashMap, VecDeque};

use calloop::LoopHandle;
use wayland_client::backend::ObjectId;
use wayland_client::QueueHandle;

use crate::bridge::ReplySlot;
use crate::settings::Settings;

use super::bindings::Bindings;
use super::commands::Queued;
use super::globals::Globals;
use super::leases::Leases;
use super::modes::Modes;
use super::outputs::Outputs;
use super::render::TargetCache;
use super::rules::Rules;
use super::seats::Seats;
use super::sequence::Sequence;
use super::waits::Waits;
use super::windows::Windows;

/// Upper bound on the `pending_focus` queue. A normal session adds 0..3
/// entries per manage tick; this cap keeps a pathological case (lots of
/// windows opening while a launcher holds layer focus, so the queue is
/// not drained) from accumulating without bound. The newest IDs are kept,
/// since the drain walks newest-first anyway.
pub const PENDING_FOCUS_CAP: usize = 16;

pub struct World {
    pub settings: Settings,
    pub globals: Globals,
    pub qh: QueueHandle<World>,
    pub handle: LoopHandle<'static, World>,
    pub sequence: Sequence,
    pub windows: Windows,
    pub outputs: Outputs,
    pub seats: Seats,
    pub bindings: Bindings,
    pub modes: Modes,
    pub rules: Rules,
    pub leases: Leases,
    pub waits: Waits,
    /// `wl_output` globals the registry has advertised, keyed by global
    /// name → version. Consulted when a river output tells us its
    /// wl_output global name so we can bind it and learn the connector.
    pub wl_output_globals: HashMap<u32, u32>,
    pub targets: TargetCache,
    /// Commands waiting for the next manage sequence.
    pub queued: VecDeque<Queued>,
    /// Answers to send once the current manage sequence is finished, so
    /// "ok" means the server has the new state.
    pub replies: Vec<(ReplySlot, Result<String, String>)>,
    /// Windows that should receive keyboard focus at the next manage
    /// sequence (newly mapped, or clicked). Bounded by
    /// [`PENDING_FOCUS_CAP`].
    pub pending_focus: Vec<ObjectId>,
    /// Render order needs to be re-sent.
    pub stacking_dirty: bool,
    /// Monotonic counter for focus recency and raise order.
    serial: u64,
    /// The housekeeping timer (waits, process lifetimes) is scheduled.
    pub ticker_armed: bool,
    running: bool,
}

impl World {
    pub fn new(
        settings: Settings,
        globals: Globals,
        qh: QueueHandle<World>,
        handle: LoopHandle<'static, World>,
    ) -> Self {
        Self {
            settings,
            globals,
            qh,
            handle,
            sequence: Sequence::new(),
            windows: Windows::default(),
            outputs: Outputs::default(),
            seats: Seats::default(),
            bindings: Bindings::default(),
            modes: Modes::default(),
            rules: Rules::default(),
            leases: Leases::default(),
            waits: Waits::default(),
            wl_output_globals: HashMap::new(),
            targets: TargetCache::default(),
            queued: VecDeque::new(),
            replies: Vec::new(),
            pending_focus: Vec::new(),
            stacking_dirty: true,
            serial: 0,
            ticker_armed: false,
            running: true,
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn shutdown(&mut self) {
        self.running = false;
    }

    pub fn mark_layout_dirty(&mut self) {
        self.targets.mark_dirty();
    }

    /// Ask river for a manage sequence (state changed behind its back).
    pub fn request_manage(&self) {
        self.globals.manager.manage_dirty();
    }

    pub fn next_serial(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }

    /// Lift a window to the top of its stacking layer.
    pub fn raise(&mut self, id: &ObjectId) {
        let serial = self.next_serial();
        if let Some(w) = self.windows.get_mut(id) {
            w.raised = serial;
            self.stacking_dirty = true;
        }
    }

    /// Enqueue a window for keyboard focus at the next manage sequence.
    pub fn push_pending_focus(&mut self, id: ObjectId) {
        push_capped(&mut self.pending_focus, id, PENDING_FOCUS_CAP);
    }
}

fn push_capped<T>(buf: &mut Vec<T>, item: T, cap: usize) {
    buf.push(item);
    let extra = buf.len().saturating_sub(cap);
    if extra > 0 {
        buf.drain(..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_focus_cap_drops_oldest_when_saturated() {
        let mut buf: Vec<u32> = Vec::new();
        for n in 0..(PENDING_FOCUS_CAP as u32 * 2) {
            push_capped(&mut buf, n, PENDING_FOCUS_CAP);
        }
        assert_eq!(buf.len(), PENDING_FOCUS_CAP);
        assert_eq!(*buf.last().unwrap(), PENDING_FOCUS_CAP as u32 * 2 - 1);
        assert_eq!(*buf.first().unwrap(), PENDING_FOCUS_CAP as u32);
    }

    #[test]
    fn pending_focus_cap_is_a_no_op_below_threshold() {
        let mut buf: Vec<u32> = Vec::new();
        for n in 0..5 {
            push_capped(&mut buf, n, PENDING_FOCUS_CAP);
        }
        assert_eq!(buf, vec![0, 1, 2, 3, 4]);
    }
}
