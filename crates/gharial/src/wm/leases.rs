//! Temporary output changes tied to a process: `output view DP-2 9
//! until-exit PID` and `output presentation DP-2 async until-exit PID`.
//!
//! When the process exits, the previous value comes back — but only if
//! nobody changed it in the meantime, so a user who switched tags during
//! a benchmark keeps their choice. Leases on the same output and
//! property nest: if an older one ends first, its saved value passes to
//! the newer one, so the original state is what finally returns.

use gharial_ipc::Presentation;
use wayland_client::backend::ObjectId;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Tags(u32),
    Presentation(Presentation),
}

impl Value {
    fn same_kind(&self, other: &Value) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease<O = ObjectId> {
    pub pid: u32,
    pub output: O,
    /// Value to restore.
    pub previous: Value,
    /// Value the lease set; restore only if it is still in place.
    pub set: Value,
}

/// A value to put back on an output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restore<O = ObjectId> {
    pub output: O,
    pub expect: Value,
    pub value: Value,
}

/// Generic over the output id so the bookkeeping is testable without a
/// wayland connection.
pub struct Leases<O = ObjectId> {
    list: Vec<Lease<O>>,
}

impl<O> Default for Leases<O> {
    fn default() -> Self {
        Self { list: Vec::new() }
    }
}

impl<O: Clone + PartialEq> Leases<O> {
    pub fn add(&mut self, lease: Lease<O>) {
        self.list.push(lease);
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn watched_pids(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self.list.iter().map(|l| l.pid).collect();
        pids.sort_unstable();
        pids.dedup();
        pids
    }

    /// Drop leases on an output that no longer exists, returning the
    /// values they would have restored — the oldest lease per property
    /// holds the pre-lease original.
    pub fn forget_output(&mut self, output: &O) -> Vec<Value> {
        let mut originals: Vec<Value> = Vec::new();
        self.list.retain(|l| {
            if &l.output != output {
                return true;
            }
            if !originals.iter().any(|v| v.same_kind(&l.previous)) {
                originals.push(l.previous);
            }
            false
        });
        originals
    }

    /// End the leases of exited processes, oldest first, returning the
    /// restores to apply. The caller applies each only if the output
    /// still shows `expect`.
    pub fn expire(&mut self, alive: impl Fn(u32) -> bool) -> Vec<Restore<O>> {
        let mut restores = Vec::new();
        let mut i = 0;
        while i < self.list.len() {
            if alive(self.list[i].pid) {
                i += 1;
                continue;
            }
            let ended = self.list.remove(i);
            // A newer lease on the same property inherits the value to
            // restore; the original returns when the last one ends.
            let newer = self.list[i..]
                .iter_mut()
                .find(|l| l.output == ended.output && l.set.same_kind(&ended.set));
            match newer {
                Some(newer) => newer.previous = ended.previous,
                None => restores.push(Restore {
                    output: ended.output,
                    expect: ended.set,
                    value: ended.previous,
                }),
            }
        }
        restores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(pid: u32, output: &'static str, previous: u32, set: u32) -> Lease<&'static str> {
        Lease {
            pid,
            output,
            previous: Value::Tags(previous),
            set: Value::Tags(set),
        }
    }

    #[test]
    fn a_single_lease_restores_on_exit() {
        let mut leases = Leases::default();
        leases.add(lease(10, "DP-2", 0b1, 0x100));
        assert!(leases.expire(|_| true).is_empty());
        assert_eq!(
            leases.expire(|_| false),
            vec![Restore {
                output: "DP-2",
                expect: Value::Tags(0x100),
                value: Value::Tags(0b1),
            }]
        );
        assert!(leases.is_empty());
    }

    #[test]
    fn nested_leases_restore_the_original_whichever_ends_first() {
        // Older ends first: its saved value passes to the newer lease.
        let mut leases = Leases::default();
        leases.add(lease(1, "DP-2", 0b1, 0b10));
        leases.add(lease(2, "DP-2", 0b10, 0b100));
        assert!(leases.expire(|pid| pid != 1).is_empty());
        assert_eq!(leases.expire(|_| false)[0].value, Value::Tags(0b1));

        // Newer ends first: back to the older lease's view, then original.
        let mut leases = Leases::default();
        leases.add(lease(1, "DP-2", 0b1, 0b10));
        leases.add(lease(2, "DP-2", 0b10, 0b100));
        assert_eq!(leases.expire(|pid| pid != 2)[0].value, Value::Tags(0b10));
        assert_eq!(leases.expire(|_| false)[0].value, Value::Tags(0b1));
    }

    #[test]
    fn different_outputs_and_properties_do_not_chain() {
        let mut leases = Leases::default();
        leases.add(lease(1, "DP-2", 1, 2));
        leases.add(lease(2, "HDMI-A-1", 4, 8));
        leases.add(Lease {
            pid: 3,
            output: "DP-2",
            previous: Value::Presentation(Presentation::Vsync),
            set: Value::Presentation(Presentation::Async),
        });
        assert_eq!(leases.watched_pids(), vec![1, 2, 3]);
        let restores = leases.expire(|_| false);
        assert_eq!(restores.len(), 3);
        leases.add(lease(4, "DP-2", 1, 2));
        leases.add(lease(5, "DP-2", 2, 3));
        assert_eq!(leases.forget_output(&"DP-2"), vec![Value::Tags(1)]);
        assert!(leases.is_empty());
    }
}
