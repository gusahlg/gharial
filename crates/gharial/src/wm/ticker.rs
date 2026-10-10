//! Housekeeping timer: `wait-window` deadlines and process lifetimes
//! (`until-exit`, `pid=` rules, placed spawns).
//!
//! It only runs while something needs watching, so an idle desktop never
//! wakes gharial up.

use std::collections::HashSet;
use std::time::Duration;

use calloop::timer::{TimeoutAction, Timer};

use super::commands::{enqueue, Op};
use super::world::World;
use super::{procs, waits};

const TICK: Duration = Duration::from_millis(200);

/// Make sure the timer is running.
pub fn arm(world: &mut World) {
    if world.ticker_armed {
        return;
    }
    let inserted = world
        .handle
        .insert_source(Timer::from_duration(TICK), |_, _, world: &mut World| {
            tick(world)
        });
    match inserted {
        Ok(_) => world.ticker_armed = true,
        Err(e) => eprintln!("gharial: cannot start housekeeping timer: {e}"),
    }
}

fn tick(world: &mut World) -> TimeoutAction {
    waits::check(world);

    let mut pids = world.rules.watched_pids();
    pids.extend(world.leases.watched_pids());
    let dead: HashSet<u32> = pids.into_iter().filter(|&p| !procs::is_alive(p)).collect();
    if !dead.is_empty() {
        let alive = |pid: u32| !dead.contains(&pid);
        world.rules.expire(alive);
        for restore in world.leases.expire(alive) {
            enqueue(world, Op::Restore(restore), None);
        }
    }

    let idle =
        world.waits.is_empty() && world.leases.is_empty() && world.rules.watched_pids().is_empty();
    if idle {
        world.ticker_armed = false;
        TimeoutAction::Drop
    } else {
        TimeoutAction::ToDuration(TICK)
    }
}
