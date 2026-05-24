//! Unix-socket IPC server for gharialctl.
//!
//! Runs on a dedicated thread. Each connection is one request and one
//! response line. Connection handling delegates to [`dispatch`].

mod dispatch;
mod handlers;

#[cfg(test)]
mod tests;

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::state::Shared;

/// Callback invoked after a request applies a real state change. Used
/// by the wayland thread to wake itself and refresh the layout. The
/// IPC layer treats this as an opaque "something changed" notifier;
/// it deliberately does not know about calloop.
pub type Notifier = Arc<dyn Fn() + Send + Sync>;

pub struct Server {
    pub socket_path: PathBuf,
    _handle: JoinHandle<()>,
}

impl Server {
    /// Bind the socket at the default path and spawn the accept loop.
    /// `notifier` is invoked from the IPC thread whenever a request
    /// applies a real state change; the wayland thread uses that to
    /// schedule a layout refresh.
    pub fn start_with_notifier(shared: Shared, notifier: Notifier) -> io::Result<Self> {
        Self::start_at(gharial_ipc::socket_path(), shared, Some(notifier))
    }

    /// Bind the socket at an explicit path. Used for tests.
    pub fn start_at(
        path: PathBuf,
        shared: Shared,
        notifier: Option<Notifier>,
    ) -> io::Result<Self> {
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
            std::fs::remove_file(&path).ok();
        }

        let listener = UnixListener::bind(&path)?;
        let perm = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perm)?;

        let socket_path = path.clone();
        let handle = thread::Builder::new()
            .name("gharial-ipc".into())
            .spawn(move || run(listener, shared, notifier))
            .expect("spawn ipc thread");

        Ok(Self { socket_path, _handle: handle })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        std::fs::remove_file(&self.socket_path).ok();
    }
}

fn run(listener: UnixListener, shared: Shared, notifier: Option<Notifier>) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                if let Err(e) = dispatch::handle_client(stream, &shared, notifier.as_ref()) {
                    eprintln!("gharial: ipc client error: {e}");
                }
            }
            Err(e) => {
                eprintln!("gharial: ipc accept error: {e}");
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Try to connect to an existing socket to detect a live peer.
fn probe(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}
