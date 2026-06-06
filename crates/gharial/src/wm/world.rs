//! `World` is the wayland-thread state. Owned exclusively by the event
//! loop; never wrapped in an Arc/Mutex. Subsystems (windows, outputs,
//! seats, bindings, modes, tags) live as fields here.

use std::collections::VecDeque;

use wayland_client::backend::ObjectId;
use wayland_client::QueueHandle;

use crate::action::Action;
use crate::state::Shared;

use super::bindings::Bindings;
use super::focus::FocusMemory;
use super::globals::Globals;
use super::modes::Modes;
use super::outputs::Outputs;
use super::render::TargetCache;
use super::seats::Seats;
use super::sequence::Sequence;
use super::tags::Tags;
use super::windows::Windows;

/// Upper bound on the `pending_focus` queue. A normal session adds 0..3
/// entries per manage tick; this cap keeps a pathological case (lots of
/// windows opening while a launcher holds layer focus, so the queue is
/// not drained) from accumulating without bound. We still keep the most
/// recent IDs, since `drain_pending_focus` walks newest-first anyway.
pub const PENDING_FOCUS_CAP: usize = 16;

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
    pub focus: FocusMemory<ObjectId>,
    pub target_cache: TargetCache,
    /// Actions sent from the IPC thread that need to be applied during
    /// the next manage sequence. Drained at the top of `manage_start`.
    pub pending_actions: VecDeque<Action>,
    /// Window IDs that arrived since the last manage sequence and need
    /// initial keyboard focus. Drained in `manage_start`. Bounded by
    /// `PENDING_FOCUS_CAP` to avoid unbounded growth while a layer
    /// surface holds focus and the drain is deferred.
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
            focus: FocusMemory::default(),
            target_cache: TargetCache::default(),
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

    pub fn mark_layout_dirty(&mut self) {
        self.target_cache.mark_dirty();
    }

    /// Enqueue a freshly-announced window for initial focus. Capped so a
    /// runaway open-loop while a layer surface is focused can't grow the
    /// queue without bound; we drop the *oldest* pending IDs first since
    /// the drain prefers the newest (most likely the one in front).
    pub fn push_pending_focus(&mut self, id: ObjectId) {
        self.pending_focus.push(id);
        let extra = self.pending_focus.len().saturating_sub(PENDING_FOCUS_CAP);
        if extra > 0 {
            self.pending_focus.drain(..extra);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Geometry-free tests for the small `World` helpers that don't need
    //! a wayland connection.
    use wayland_client::backend::ObjectId;

    /// We can't construct real `ObjectId`s in unit tests (they live in
    /// wayland-backend internals), so the cap behavior is exercised by
    /// the same algorithm on a stand-in element type.
    fn push_capped<T>(buf: &mut Vec<T>, item: T, cap: usize) {
        buf.push(item);
        let extra = buf.len().saturating_sub(cap);
        if extra > 0 {
            buf.drain(..extra);
        }
    }

    #[test]
    fn pending_focus_cap_drops_oldest_when_saturated() {
        let mut buf: Vec<u32> = Vec::new();
        for n in 0..(super::PENDING_FOCUS_CAP as u32 * 2) {
            push_capped(&mut buf, n, super::PENDING_FOCUS_CAP);
        }
        assert_eq!(buf.len(), super::PENDING_FOCUS_CAP);
        // The newest IDs survive; the oldest are evicted first.
        assert_eq!(*buf.last().unwrap(), super::PENDING_FOCUS_CAP as u32 * 2 - 1);
        assert_eq!(*buf.first().unwrap(), super::PENDING_FOCUS_CAP as u32);
    }

    #[test]
    fn pending_focus_cap_is_a_no_op_below_threshold() {
        let mut buf: Vec<u32> = Vec::new();
        for n in 0..5 {
            push_capped(&mut buf, n, super::PENDING_FOCUS_CAP);
        }
        assert_eq!(buf, vec![0, 1, 2, 3, 4]);
    }

    // Compile-time assertion: ObjectId is the type the queue actually
    // holds. Catches accidental drift if someone retypes pending_focus.
    #[allow(dead_code)]
    fn _typed_signature(world: &mut super::World, id: ObjectId) {
        world.push_pending_focus(id);
    }
}
