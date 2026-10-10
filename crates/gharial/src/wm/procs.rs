//! Process-tree queries over `/proc`, for `pid=` matchers and
//! `until-exit` lifetimes.
//!
//! River reports an *unreliable* pid per window (racy against pid reuse),
//! which is fine for placement policy: the worst case is a rule applying
//! to the wrong window, never anything security-relevant.

use std::fs;

/// The fields of `/proc/<pid>/stat` we use.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub state: char,
    pub ppid: u32,
    pub session: u32,
}

/// Parse a `/proc/<pid>/stat` line. The command name (field 2) may hold
/// spaces and parentheses, so fields are counted from the *last* `)`.
pub fn parse_stat(line: &str) -> Option<Stat> {
    let rest = &line[line.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let ppid = fields.next()?.parse().ok()?;
    let _pgrp = fields.next()?;
    let session = fields.next()?.parse().ok()?;
    Some(Stat {
        state,
        ppid,
        session,
    })
}

pub fn stat(pid: u32) -> Option<Stat> {
    parse_stat(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// `true` while `pid` exists and is not a zombie.
pub fn is_alive(pid: u32) -> bool {
    stat(pid).is_some_and(|s| !matches!(s.state, 'Z' | 'X'))
}

/// Ancestry walks stop after this many hops — real process trees are a
/// few dozen deep at most, and the cap guards against a loop if `/proc`
/// changes underneath the walk.
const MAX_DEPTH: usize = 128;

/// Whether `pid` is `ancestor`, one of its descendants, or a member of
/// the session `ancestor` leads. The session check catches processes
/// that double-forked away from the spawned tree (re-parented to init or
/// a subreaper) but stayed in the session gharial's `spawn` created.
pub fn is_related(pid: u32, ancestor: u32) -> bool {
    related_with(pid, ancestor, stat)
}

fn related_with(pid: u32, ancestor: u32, stat: impl Fn(u32) -> Option<Stat>) -> bool {
    if pid == ancestor {
        return true;
    }
    let mut current = pid;
    for _ in 0..MAX_DEPTH {
        let Some(s) = stat(current) else {
            return false;
        };
        if current == pid && s.session == ancestor {
            return true;
        }
        if s.ppid == ancestor {
            return true;
        }
        if s.ppid <= 1 {
            return false;
        }
        current = s.ppid;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parse_stat_handles_hostile_command_names() {
        let line = "1234 (my (weird) prog) S 1000 1234 1234 34816 1234 4194560 0 0";
        assert_eq!(
            parse_stat(line),
            Some(Stat {
                state: 'S',
                ppid: 1000,
                session: 1234,
            })
        );
        assert_eq!(parse_stat("garbage"), None);
        assert_eq!(parse_stat("1 (x) S"), None);
    }

    fn table(entries: &[(u32, u32, u32)]) -> impl Fn(u32) -> Option<Stat> {
        let map: HashMap<u32, Stat> = entries
            .iter()
            .map(|&(pid, ppid, session)| {
                (
                    pid,
                    Stat {
                        state: 'S',
                        ppid,
                        session,
                    },
                )
            })
            .collect();
        move |pid| map.get(&pid).copied()
    }

    #[test]
    fn descendants_and_session_members_are_related() {
        // 100 spawned (session leader) → 101 (nix develop) → 102 (game).
        // 200 double-forked away from 100 but kept its session.
        // 300 is unrelated.
        let t = table(&[
            (100, 50, 100),
            (101, 100, 100),
            (102, 101, 100),
            (200, 1, 100),
            (300, 1, 300),
        ]);
        assert!(related_with(102, 100, &t));
        assert!(related_with(101, 100, &t));
        assert!(related_with(100, 100, &t));
        assert!(related_with(200, 100, &t));
        assert!(!related_with(300, 100, &t));
        assert!(!related_with(100, 102, &t), "ancestry is one-way");
        assert!(!related_with(999, 100, &t), "unknown pids never match");
    }

    #[test]
    fn walk_terminates_on_cycles() {
        let t = table(&[(10, 11, 10), (11, 10, 10)]);
        assert!(!related_with(10, 99, &t));
    }

    #[test]
    fn this_process_is_alive_and_related_to_itself() {
        let me = std::process::id();
        assert!(is_alive(me));
        assert!(is_related(me, me));
        let parent = stat(me).unwrap().ppid;
        assert!(is_related(me, parent));
        assert!(!is_alive(u32::MAX - 1));
    }
}
