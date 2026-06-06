//! `bind` and `unbind` IPC verbs.
//!
//! Grammar:
//!   `bind   [--mode MODE] <chord> <action ...>`
//!   `unbind [--mode MODE] <chord>`
//!
//! Both validate eagerly so a typo in `~/.config/gharial/init` fails at
//! the IPC reply rather than from a daemon log line at fire-time. The
//! `--mode MODE` flag defaults to `default`.

use gharial_ipc::Response;

use crate::action::{Action, BindingSpec};
use crate::state::Shared;

use super::queue;

const DEFAULT_MODE: &str = "default";

pub fn bind(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let (mode, rest) = match split_mode(args) {
        Ok(p) => p,
        Err(e) => return (Response::err(e), false),
    };

    let (chord, action_tokens) = match rest.split_first() {
        Some(pair) => pair,
        None => return (Response::err("bind: expected <chord> <action ...>"), false),
    };

    let spec = match BindingSpec::parse(chord) {
        Ok(s) => s,
        Err(e) => return (Response::err(e), false),
    };
    let action = match Action::parse(action_tokens) {
        Ok(a) => a,
        Err(e) => return (Response::err(e), false),
    };

    queue(
        shared,
        Action::Bind {
            spec,
            action: Box::new(action),
            mode: mode.to_string(),
        },
        "bind queued",
    )
}

pub fn unbind(shared: &Shared, args: &[&str]) -> (Response, bool) {
    let (mode, rest) = match split_mode(args) {
        Ok(p) => p,
        Err(e) => return (Response::err(e), false),
    };
    let chord = match rest.first() {
        Some(&c) => c,
        None => return (Response::err("unbind: expected <chord>"), false),
    };
    let spec = match BindingSpec::parse(chord) {
        Ok(s) => s,
        Err(e) => return (Response::err(e), false),
    };
    queue(
        shared,
        Action::Unbind {
            spec,
            mode: mode.to_string(),
        },
        "unbind queued",
    )
}

fn split_mode<'a>(args: &'a [&'a str]) -> Result<(&'a str, &'a [&'a str]), String> {
    match args.split_first() {
        Some((&"--mode", rest)) => {
            let (&mode, after) = rest
                .split_first()
                .ok_or_else(|| "--mode requires a name".to_string())?;
            Ok((mode, after))
        }
        _ => Ok((DEFAULT_MODE, args)),
    }
}
