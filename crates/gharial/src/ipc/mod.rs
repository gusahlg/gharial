//! Unix-socket IPC server for gharialctl and Rust clients.
//!
//! The accept loop runs on its own thread and hands every connection to
//! a short-lived worker thread, so a blocking `wait-window` never holds
//! up other clients. Each connection is one request line and one
//! response line; [`dispatch`] parses the request and forwards it across
//! the [`Bridge`] to the wayland thread.

mod dispatch;

#[cfg(test)]
mod tests;

use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use gharial_ipc::Response;

use crate::bridge::Bridge;

/// Connections served at once. Beyond this a client gets an immediate
/// `err busy` instead of a thread — enough headroom for scripts that
/// fire many requests in parallel, small enough to bound resource use
/// if something hammers the socket.
const MAX_CONNECTIONS: usize = 64;

pub struct Server {
    pub socket_path: PathBuf,
}

impl Server {
    /// Bind the socket at the default path and spawn the accept loop.
    pub fn start(bridge: Bridge) -> io::Result<Self> {
        Self::start_at(gharial_ipc::socket_path(), bridge)
    }

    /// Bind the socket at an explicit path.
    pub fn start_at(path: PathBuf, bridge: Bridge) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if path.exists() {
            if probe(&path) {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("another gharial daemon is listening on {}", path.display()),
                ));
            }
            // A stale socket from a crashed daemon.
            std::fs::remove_file(&path).ok();
        }

        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;

        thread::Builder::new()
            .name("gharial-ipc".into())
            .spawn(move || accept_loop(listener, bridge))?;

        Ok(Self { socket_path: path })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        std::fs::remove_file(&self.socket_path).ok();
    }
}

fn accept_loop(listener: UnixListener, bridge: Bridge) {
    let active = Arc::new(AtomicUsize::new(0));
    for incoming in listener.incoming() {
        let stream = match incoming {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("gharial: ipc accept error: {e}");
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::AcqRel);
            refuse(stream, "busy: too many concurrent requests, retry");
            continue;
        }
        let bridge = bridge.clone();
        let guard = ActiveGuard(active.clone());
        let spawned = thread::Builder::new()
            .name("gharial-ipc-conn".into())
            .spawn(move || {
                let _guard = guard;
                if let Err(e) = dispatch::handle_client(stream, &bridge) {
                    eprintln!("gharial: ipc client error: {e}");
                }
            });
        if let Err(e) = spawned {
            eprintln!("gharial: cannot spawn ipc worker: {e}");
        }
    }
}

/// Decrements the active-connection count when a worker finishes (or
/// when its closure is dropped because the spawn failed).
struct ActiveGuard(Arc<AtomicUsize>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn refuse(mut stream: UnixStream, message: &str) {
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
    let _ = stream.write_all(Response::err(message).encode().as_bytes());
}

/// Try to connect to an existing socket to detect a live peer.
fn probe(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}
