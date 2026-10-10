//! IPC commands arriving on the wayland thread.
//!
//! Queries (`status`, `window list`, `rule list`, …) are answered on the
//! spot from current state. State changes are queued for the next manage
//! sequence — the only time their protocol requests are legal — and
//! answered after `manage_finish`, so a caller's `ok` means the change
//! has been handed to the compositor. `spawn` and rule edits touch no
//! protocol state and complete immediately.

use gharial_ipc::{Action, BindingSpec, Command};

use crate::bridge::{Envelope, ReplySlot};

use super::actions::{self, tag::apply_tag_change, Outcome};
use super::leases::{Restore, Value};
use super::world::World;
use super::{query, ticker, waits};

/// Work for the next manage sequence.
pub enum Op {
    Action(Action),
    Bind {
        mode: String,
        spec: BindingSpec,
        action: Action,
    },
    Unbind {
        mode: String,
        spec: BindingSpec,
    },
    /// A lease ended; put the old value back if still applicable.
    Restore(Restore),
}

pub struct Queued {
    pub op: Op,
    /// `None` for key bindings and internal work.
    pub reply: Option<ReplySlot>,
}

pub fn receive(world: &mut World, envelope: Envelope) {
    let Envelope { command, reply } = envelope;
    match command {
        Command::Ping => reply.ok("pong"),
        Command::Version => reply.ok(env!("CARGO_PKG_VERSION")),
        Command::Status => reply.ok(world.settings.status_line()),
        Command::Get(key) => reply.result(world.settings.get(&key)),
        Command::ListWindows { selector, format } => {
            reply.ok(query::windows(world, selector.as_ref(), format))
        }
        Command::ListOutputs { format } => reply.ok(query::outputs(world, format)),
        Command::ListRules => reply.ok(world.rules.listing()),
        Command::AddRule { rule, until_exit } => {
            let pid_bound = until_exit.is_some() || rule.matcher.pid.is_some();
            let id = world.rules.add(rule, until_exit);
            if pid_bound {
                ticker::arm(world);
            }
            reply.ok(id.to_string());
        }
        Command::RemoveRule(id) => {
            if world.rules.remove(id) {
                reply.ok("")
            } else {
                reply.err(format!("no rule {id}"))
            }
        }
        Command::ClearRules => reply.ok(world.rules.clear().to_string()),
        Command::WaitWindow {
            selector,
            timeout,
            new_only,
        } => waits::register(world, selector, timeout, new_only, reply),
        // No protocol requests involved: do it now.
        Command::Action(action @ (Action::Spawn { .. } | Action::SetOutputFocusWarp(_))) => {
            reply.result(actions::execute(world, action))
        }
        Command::Action(action) => enqueue(world, Op::Action(action), Some(reply)),
        Command::Bind { mode, spec, action } => {
            enqueue(world, Op::Bind { mode, spec, action }, Some(reply))
        }
        Command::Unbind { mode, spec } => enqueue(world, Op::Unbind { mode, spec }, Some(reply)),
    }
}

/// Queue `op` for the next manage sequence and ask river for one.
pub fn enqueue(world: &mut World, op: Op, reply: Option<ReplySlot>) {
    world.queued.push_back(Queued { op, reply });
    world.request_manage();
}

/// Run everything queued. Inside a manage sequence only.
pub fn drain(world: &mut World) {
    while let Some(Queued { op, reply }) = world.queued.pop_front() {
        let label = op_label(&op);
        let outcome = run(world, op);
        match reply {
            Some(slot) => world.replies.push((slot, outcome)),
            None => {
                if let Err(e) = outcome {
                    eprintln!("gharial: {label}: {e}");
                }
            }
        }
    }
}

fn run(world: &mut World, op: Op) -> Outcome {
    match op {
        Op::Action(action) => actions::execute(world, action),
        Op::Bind { mode, spec, action } => actions::bind(world, mode, spec, action),
        Op::Unbind { mode, spec } => actions::unbind(world, &mode, &spec),
        Op::Restore(restore) => {
            restore_lease(world, restore);
            Ok(String::new())
        }
    }
}

fn op_label(op: &Op) -> String {
    match op {
        Op::Action(action) => action.to_tokens().join(" "),
        Op::Bind { .. } => "bind".into(),
        Op::Unbind { .. } => "unbind".into(),
        Op::Restore(_) => "restore".into(),
    }
}

/// Send the answers collected during the manage sequence. Call right
/// after `manage_finish`.
pub fn flush_replies(world: &mut World) {
    for (slot, outcome) in world.replies.drain(..) {
        slot.result(outcome);
    }
}

fn restore_lease(world: &mut World, restore: Restore) {
    let Some(output) = world.outputs.get_mut(&restore.output) else {
        return;
    };
    match (restore.expect, restore.value) {
        (Value::Tags(expect), Value::Tags(value)) if output.active_tags == expect => {
            output.active_tags = value;
            apply_tag_change(world);
        }
        (Value::Presentation(expect), Value::Presentation(value))
            if output.presentation == expect =>
        {
            output.presentation = value;
        }
        _ => {} // changed meanwhile: the user's choice wins
    }
}
