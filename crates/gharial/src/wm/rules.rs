//! Installed window rules and their lifetimes.
//!
//! Rules are consulted once per window, when it is first placed. Every
//! matching rule contributes its placement in insertion order (later
//! fields win); `oneshot` rules retire after their first match. A rule
//! tied to a process — through `until-exit PID`, a `pid=` matcher, or a
//! placed `spawn` — is dropped when that process exits, so a crashed
//! test run leaves nothing behind.

use gharial_ipc::{Placement, Rule};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleEntry {
    pub id: u64,
    pub rule: Rule,
    /// Drop the rule once this process is gone.
    pub until_exit: Option<u32>,
}

impl RuleEntry {
    /// Pids whose exit ends this rule.
    fn lifetime_pids(&self) -> impl Iterator<Item = u32> + '_ {
        self.until_exit.into_iter().chain(self.rule.matcher.pid)
    }
}

/// What a window looks like to the matcher.
pub struct Subject<'a> {
    pub app_id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub pid: Option<u32>,
}

#[derive(Default)]
pub struct Rules {
    entries: Vec<RuleEntry>,
    next_id: u64,
}

impl Rules {
    pub fn add(&mut self, rule: Rule, until_exit: Option<u32>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.entries.push(RuleEntry {
            id,
            rule,
            until_exit,
        });
        id
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    pub fn clear(&mut self) -> usize {
        std::mem::take(&mut self.entries).len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &RuleEntry> {
        self.entries.iter()
    }

    /// Fold every rule matching `subject` into one placement (`None`
    /// when nothing matched), retiring oneshot rules that fired — they
    /// are returned so the window can keep them for a re-placement.
    /// `related(window_pid, rule_pid)` decides process ancestry
    /// (injected so tests need no `/proc`).
    pub fn resolve(
        &mut self,
        subject: &Subject<'_>,
        related: impl Fn(u32, u32) -> bool,
    ) -> (Option<Placement>, Vec<Rule>) {
        let mut merged: Option<Placement> = None;
        let mut consumed = Vec::new();
        self.entries.retain(|entry| {
            if !matches(&entry.rule, subject, &related) {
                return true;
            }
            merged
                .get_or_insert_with(Placement::default)
                .merge(&entry.rule.placement);
            if entry.rule.oneshot {
                consumed.push(entry.rule.clone());
            }
            !entry.rule.oneshot
        });
        (merged, consumed)
    }

    /// Distinct pids some rule's lifetime depends on.
    pub fn watched_pids(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self
            .entries
            .iter()
            .flat_map(RuleEntry::lifetime_pids)
            .collect();
        pids.sort_unstable();
        pids.dedup();
        pids
    }

    /// Drop rules bound to a process that has exited. Returns how many.
    pub fn expire(&mut self, alive: impl Fn(u32) -> bool) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.lifetime_pids().all(&alive));
        before - self.entries.len()
    }

    /// `ID<TAB>tokens…` per rule, plus `until-exit` when set.
    pub fn listing(&self) -> String {
        self.entries
            .iter()
            .map(|e| {
                let mut line = format!("{}\t{}", e.id, e.rule.to_tokens().join(" "));
                if let Some(pid) = e.until_exit {
                    line.push_str(&format!("\tuntil-exit={pid}"));
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Whether `rule` applies to `subject`.
pub fn matches(rule: &Rule, subject: &Subject<'_>, related: impl Fn(u32, u32) -> bool) -> bool {
    let m = &rule.matcher;
    let pid_ok = match m.pid {
        None => true,
        Some(want) => subject.pid.is_some_and(|have| related(have, want)),
    };
    pid_ok && m.matches_text(subject.app_id, subject.title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gharial_ipc::{BoolValue, Matcher};

    fn rule(tokens: &[&str]) -> Rule {
        Rule::parse(tokens).unwrap()
    }

    fn subject<'a>(app_id: &'a str, title: &'a str, pid: Option<u32>) -> Subject<'a> {
        Subject {
            app_id: Some(app_id),
            title: Some(title),
            pid,
        }
    }

    fn same(a: u32, b: u32) -> bool {
        a == b
    }

    #[test]
    fn later_rules_override_earlier_ones() {
        let mut rules = Rules::default();
        rules.add(rule(&["app-id=foo*", "float", "output", "DP-1"]), None);
        rules.add(rule(&["app-id=foobar", "output", "DP-2"]), None);
        let p = rules
            .resolve(&subject("foobar", "t", None), same)
            .0
            .expect("both match");
        assert_eq!(p.floating, Some(BoolValue::On));
        assert_eq!(p.output.unwrap().to_token(), "DP-2");
        assert!(rules.resolve(&subject("baz", "t", None), same).0.is_none());
    }

    #[test]
    fn oneshot_rules_retire_after_first_match() {
        let mut rules = Rules::default();
        rules.add(rule(&["title=bench", "tags", "9", "oneshot"]), None);
        let (placement, consumed) = rules.resolve(&subject("a", "bench", None), same);
        assert!(placement.is_some());
        assert_eq!(consumed.len(), 1, "the window keeps the rule it consumed");
        assert!(rules
            .resolve(&subject("a", "bench", None), same)
            .0
            .is_none());
        assert!(rules.is_empty());
    }

    #[test]
    fn pid_matchers_use_the_relation() {
        let mut rules = Rules::default();
        rules.add(
            Rule::new(Matcher::pid(100), Placement::new().sticky(true)),
            None,
        );
        let child_of_100 = |pid: u32, ancestor: u32| pid == 101 && ancestor == 100;
        let mut placed = |pid| rules.resolve(&subject("x", "y", pid), child_of_100).0;
        assert!(placed(Some(101)).is_some());
        assert!(placed(Some(555)).is_none());
        assert!(
            placed(None).is_none(),
            "an unknown window pid never satisfies a pid matcher"
        );
    }

    #[test]
    fn rules_expire_with_their_processes() {
        let mut rules = Rules::default();
        let keep = rules.add(rule(&["app-id=a", "float"]), None);
        rules.add(rule(&["app-id=b", "float"]), Some(7));
        rules.add(rule(&["pid=8", "float"]), None);
        assert_eq!(rules.watched_pids(), vec![7, 8]);
        assert_eq!(rules.expire(|pid| pid != 7 && pid != 8), 2);
        assert_eq!(rules.iter().map(|e| e.id).collect::<Vec<_>>(), vec![keep]);
    }

    #[test]
    fn listing_and_removal() {
        let mut rules = Rules::default();
        let a = rules.add(rule(&["app-id=a", "float"]), Some(42));
        let b = rules.add(rule(&["title=b", "sticky"]), None);
        assert_eq!(
            rules.listing(),
            format!("{a}\tapp-id=a float\tuntil-exit=42\n{b}\ttitle=b sticky")
        );
        assert!(rules.remove(a));
        assert!(!rules.remove(a));
        assert_eq!(rules.clear(), 1);
        assert_eq!(rules.listing(), "");
    }
}
