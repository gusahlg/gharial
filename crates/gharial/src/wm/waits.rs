//! Pending `wait-window` requests.
//!
//! A wait is answered with the window's id as soon as a matching window
//! is settled (placed, sized, positioned, drawn), or with an error at
//! its deadline. Checked after every manage and render sequence and on
//! the housekeeping tick.

use std::time::{Duration, Instant};

use gharial_ipc::WindowSelector;

use crate::bridge::ReplySlot;

use super::actions::window::select;
use super::ticker;
use super::world::World;

pub struct Wait {
    pub selector: WindowSelector,
    /// `--new`: windows already mapped (ever settled) when the wait
    /// began, which must not answer it. Keyed on "mapped", not
    /// "created", so a window that appeared just before the wait
    /// registered still counts.
    pub ignore: Vec<u64>,
    pub deadline: Instant,
    pub reply: ReplySlot,
}

#[derive(Default)]
pub struct Waits {
    pending: Vec<Wait>,
}

impl Waits {
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

pub fn register(
    world: &mut World,
    selector: WindowSelector,
    timeout: Duration,
    new_only: bool,
    reply: ReplySlot,
) {
    let ignore = if new_only {
        select(world, &selector)
            .iter()
            .filter_map(|id| world.windows.get(id))
            .filter(|w| w.ever_settled)
            .map(|w| w.uid)
            .collect()
    } else {
        Vec::new()
    };
    if let Some(&uid) = settled(world, &selector, &ignore).first() {
        reply.ok(uid.to_string());
        return;
    }
    world.waits.pending.push(Wait {
        selector,
        ignore,
        deadline: Instant::now() + timeout,
        reply,
    });
    ticker::arm(world);
}

/// Answer every wait that is satisfied or out of time.
pub fn check(world: &mut World) {
    if world.waits.is_empty() {
        return;
    }
    let now = Instant::now();
    for wait in std::mem::take(&mut world.waits.pending) {
        if let Some(&uid) = settled(world, &wait.selector, &wait.ignore).first() {
            wait.reply.ok(uid.to_string());
        } else if now >= wait.deadline {
            let candidates = select(world, &wait.selector).len();
            wait.reply.err(format!(
                "timed out waiting for a window matching {} ({candidates} matching, none newly mapped at final size)",
                wait.selector
            ));
        } else {
            world.waits.pending.push(wait);
        }
    }
}

/// Uids of settled windows matching `selector`, minus `ignore`.
fn settled(world: &World, selector: &WindowSelector, ignore: &[u64]) -> Vec<u64> {
    select(world, selector)
        .iter()
        .filter_map(|id| world.windows.get(id))
        .filter(|w| w.is_settled() && !ignore.contains(&w.uid))
        .map(|w| w.uid)
        .collect()
}
