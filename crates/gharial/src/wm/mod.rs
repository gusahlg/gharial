//! Window-management runtime built on `river-window-management-v1`,
//! `river-xkb-bindings-v1`, and `river-layer-shell-v1`. Public surface
//! is just [`run`]; the rest is the `World` state machine and the
//! per-interface dispatch tables.

mod actions;
mod bindings;
mod commands;
mod dispatch;
mod focus;
mod globals;
mod leases;
mod modes;
mod outputs;
mod placement;
mod procs;
mod query;
mod render;
mod rules;
mod seats;
mod sequence;
mod spatial;
mod tags;
mod ticker;
mod waits;
mod windows;
mod world;

use std::error::Error;

use calloop::channel::Event;
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

use crate::bridge::{Bridge, Envelope};
use crate::settings::Settings;

pub use world::World;

/// Connect to wayland, bind river globals, serve IPC, and run the
/// manage/render loop until river ends the session.
pub fn run(settings: Settings) -> Result<(), Box<dyn Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init::<World>(&conn)?;
    let qh = queue.handle();
    let bound = globals::bind_all(&globals, &qh)?;

    let mut event_loop: EventLoop<'static, World> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue).insert(handle.clone())?;

    // IPC → wayland thread: every request arrives here as an envelope.
    let (tx, rx) = calloop::channel::channel::<Envelope>();
    handle.insert_source(rx, |event, _, world: &mut World| {
        if let Event::Msg(envelope) = event {
            commands::receive(world, envelope);
        }
    })?;
    let ipc = crate::ipc::Server::start(Bridge::new(tx))?;
    eprintln!("gharial: ipc socket at {}", ipc.socket_path.display());

    let mut world = World::new(settings, bound, qh, handle);
    // Seed the wl_output global map from the initial registry burst;
    // hotplug arrivals flow through the registry dispatch afterwards.
    for global in globals.contents().clone_list() {
        if global.interface == "wl_output" {
            world.wl_output_globals.insert(global.name, global.version);
        }
    }

    while world.running() {
        event_loop.dispatch(None, &mut world)?;
    }
    drop(ipc);
    Ok(())
}
