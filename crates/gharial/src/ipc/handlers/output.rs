//! `output` IPC verb — multi-screen control.
//!
//! Grammar:
//!   `output focus <next|prev|left|right|up|down|NAME>`
//!                               switch the focused screen
//!   `output send <next|prev|left|right|up|down|NAME>`
//!                               move the focused window to a screen
//!   `output link A:EDGE B:EDGE` link two screen edges so the pointer
//!                               warps through them (both directions)
//!   `output unlink A:EDGE`      remove links touching that edge
//!   `output list`               describe outputs + configured links
//!
//! Outputs are addressed by connector name (`DP-1`) or 1-based index in
//! advertisement order; edges are `left|right|top|bottom`.

use gharial_ipc::Response;

use crate::action::Action;
use crate::state::Shared;

pub fn output(shared: &Shared, args: &[&str]) -> (Response, bool) {
    if args.first() == Some(&"list") {
        return (Response::ok(format_list(shared)), false);
    }
    // Reuse the same parser as bound `output ...` actions so the two
    // surfaces never drift.
    let action = match Action::parse(&prepend_output(args)) {
        Ok(a) => a,
        Err(e) => return (Response::err(e), false),
    };
    match shared.send_action(action) {
        Ok(()) => (Response::ok("output queued"), false),
        Err(e) => (Response::err(e), false),
    }
}

fn prepend_output<'a>(args: &'a [&'a str]) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(args.len() + 1);
    out.push("output");
    out.extend_from_slice(args);
    out
}

/// One line, semicolon-separated:
/// `DP-1 1920x1080+0+0 tags=0x00000001 focused; DP-2 ...; link DP-1:right<->DP-2:left`
fn format_list(shared: &Shared) -> String {
    let info = shared.outputs_info();
    if info.outputs.is_empty() {
        return "no outputs".into();
    }
    let mut parts: Vec<String> = info
        .outputs
        .iter()
        .map(|o| {
            format!(
                "{} {}x{}+{}+{} tags=0x{:08x}{}",
                o.name,
                o.dimensions.0,
                o.dimensions.1,
                o.position.0,
                o.position.1,
                o.active_tags,
                if o.focused { " focused" } else { "" },
            )
        })
        .collect();
    for (a, b) in &info.links {
        parts.push(format!("link {a}<->{b}"));
    }
    parts.join("; ")
}
