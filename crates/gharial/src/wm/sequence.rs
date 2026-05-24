//! Phase state machine for the river-window-management-v1 manage/render
//! loop. The protocol kills the WM with `protocol_error::sequence_order`
//! if we touch manage-state outside a manage sequence (or render-state
//! outside a manage/render sequence), so the wm dispatch impls go
//! through this type to keep the rule provable from one place.
//!
//! v0.2 only tracks the current phase and bumps a generation counter
//! per manage sequence; bucket queueing for cross-phase requests is
//! deferred to v0.3 where new actions need it.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Managing { generation: u64 },
    Rendering { generation: u64 },
}

#[derive(Debug)]
pub struct Sequence {
    phase: Phase,
    generation: u64,
}

impl Sequence {
    pub fn new() -> Self {
        Self { phase: Phase::Idle, generation: 0 }
    }

    /// Exposed for tests only — production code never needs to peek at
    /// the phase from outside the dispatch impls.
    #[cfg(test)]
    pub(super) fn phase(&self) -> Phase {
        self.phase
    }

    pub fn enter_manage(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.phase = Phase::Managing { generation: self.generation };
    }

    pub fn exit_manage(&mut self) {
        self.phase = Phase::Idle;
    }

    pub fn enter_render(&mut self) {
        // Render keeps the same generation as the manage that preceded
        // it; meaningful once per-generation reconciliation lands.
        self.phase = Phase::Rendering { generation: self.generation };
    }

    pub fn exit_render(&mut self) {
        self.phase = Phase::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_idle() {
        let s = Sequence::new();
        assert_eq!(s.phase(), Phase::Idle);
    }

    #[test]
    fn manage_then_render_cycle() {
        let mut s = Sequence::new();
        s.enter_manage();
        assert!(matches!(s.phase(), Phase::Managing { .. }));
        s.exit_manage();
        assert_eq!(s.phase(), Phase::Idle);
        s.enter_render();
        assert!(matches!(s.phase(), Phase::Rendering { .. }));
        s.exit_render();
        assert_eq!(s.phase(), Phase::Idle);
    }

    #[test]
    fn generation_increments_per_manage() {
        let mut s = Sequence::new();
        s.enter_manage();
        let g1 = match s.phase() {
            Phase::Managing { generation } => generation,
            _ => unreachable!(),
        };
        s.exit_manage();
        s.enter_manage();
        let g2 = match s.phase() {
            Phase::Managing { generation } => generation,
            _ => unreachable!(),
        };
        assert!(g2 > g1);
    }

    #[test]
    fn render_uses_preceding_manage_generation() {
        let mut s = Sequence::new();
        s.enter_manage();
        let g_manage = match s.phase() {
            Phase::Managing { generation } => generation,
            _ => unreachable!(),
        };
        s.exit_manage();
        s.enter_render();
        let g_render = match s.phase() {
            Phase::Rendering { generation } => generation,
            _ => unreachable!(),
        };
        assert_eq!(g_render, g_manage);
    }
}
