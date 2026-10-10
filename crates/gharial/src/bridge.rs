//! The bridge from IPC connection threads to the wayland thread.
//!
//! The wayland thread owns all state. A connection thread parses its
//! request into a [`Command`], ships it across a calloop channel inside
//! an [`Envelope`], and blocks on the envelope's reply slot. The wayland
//! thread answers queries immediately and state changes once they have
//! been applied in a manage sequence — so an `ok` means "done", not
//! "queued".

use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::time::Duration;

use calloop::channel::Sender;
use gharial_ipc::{Command, Response};

/// Where the wayland thread sends a command's answer. Sending into a
/// slot whose requester gave up is a silent no-op.
pub struct ReplySlot(SyncSender<Response>);

impl ReplySlot {
    pub fn send(self, response: Response) {
        let _ = self.0.try_send(response);
    }

    pub fn ok(self, body: impl Into<String>) {
        self.send(Response::ok(body));
    }

    pub fn err(self, message: impl Into<String>) {
        self.send(Response::err(message));
    }

    pub fn result(self, result: Result<String, String>) {
        self.send(match result {
            Ok(body) => Response::Ok(body),
            Err(message) => Response::Err(message),
        });
    }
}

/// A command plus the slot its answer goes to.
pub struct Envelope {
    pub command: Command,
    pub reply: ReplySlot,
}

/// Cheap, cloneable handle the IPC side holds.
#[derive(Clone)]
pub struct Bridge {
    tx: Sender<Envelope>,
}

impl Bridge {
    pub fn new(tx: Sender<Envelope>) -> Self {
        Self { tx }
    }

    /// Send `command` and wait up to `timeout` for the answer.
    pub fn request(&self, command: Command, timeout: Duration) -> Response {
        let (reply, answer) = slot();
        if self.tx.send(Envelope { command, reply }).is_err() {
            return Response::err("window manager is shutting down");
        }
        match answer.recv_timeout(timeout) {
            Ok(response) => response,
            Err(RecvTimeoutError::Timeout) => Response::err(format!(
                "window manager did not answer within {timeout:?} \
                 (is the compositor stalled?)"
            )),
            Err(RecvTimeoutError::Disconnected) => {
                Response::err("window manager dropped the request")
            }
        }
    }
}

/// A fresh reply slot and the receiver that waits on it.
pub fn slot() -> (ReplySlot, Receiver<Response>) {
    let (tx, rx) = sync_channel(1);
    (ReplySlot(tx), rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_reaches_the_waiting_side() {
        let (slot, rx) = slot();
        slot.ok("done");
        assert_eq!(rx.recv().unwrap(), Response::ok("done"));
    }

    #[test]
    fn replying_after_the_requester_gave_up_is_harmless() {
        let (slot, rx) = slot();
        drop(rx);
        slot.err("too late");
    }

    #[test]
    fn request_times_out_when_nobody_answers() {
        let (tx, _rx) = calloop::channel::channel();
        let bridge = Bridge::new(tx);
        let resp = bridge.request(Command::Status, Duration::from_millis(20));
        assert!(!resp.is_ok());
        assert!(resp.body().contains("did not answer"));
    }

    #[test]
    fn request_fails_fast_when_the_wayland_side_is_gone() {
        let (tx, rx) = calloop::channel::channel::<Envelope>();
        drop(rx);
        let resp = Bridge::new(tx).request(Command::Status, Duration::from_secs(5));
        assert!(resp.body().contains("shutting down"));
    }
}
