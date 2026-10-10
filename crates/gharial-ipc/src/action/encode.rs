//! [`Action`] → tokens: the inverse of [`super::parse`], so the Rust API
//! ships exactly the vocabulary gharialctl speaks.

use super::Action;

impl Action {
    /// Encode this action as the token list [`Action::parse`] accepts.
    /// Round-trips: parsing the result recovers `self`.
    pub fn to_tokens(&self) -> Vec<String> {
        let words = |ws: &[&str]| ws.iter().map(|w| (*w).to_string()).collect::<Vec<_>>();
        match self {
            Self::Close => words(&["close"]),
            Self::ToggleFloat => words(&["toggle-float"]),
            Self::ToggleFullscreen => words(&["toggle-fullscreen"]),
            Self::ToggleSticky => words(&["toggle-sticky"]),
            Self::FocusDirection(dir) => words(&["focus", dir.as_str()]),
            Self::SwapDirection(dir) => words(&["swap", dir.as_str()]),
            Self::Spawn {
                cmd,
                args,
                placement,
            } => {
                let mut out = vec!["spawn".to_string()];
                if !placement.is_empty() || cmd.starts_with('-') {
                    out.extend(placement.to_flags());
                    out.push("--".into());
                }
                out.push(cmd.clone());
                out.extend(args.iter().cloned());
                out
            }
            Self::Layout { key, args } => {
                let mut out = vec![key.clone()];
                out.extend(args.iter().cloned());
                out
            }
            Self::EnterMode(name) => vec!["mode".into(), name.clone()],
            Self::ExitMode => words(&["mode", "exit"]),
            Self::FocusTag(n) => vec!["tag".into(), "focus".into(), n.to_string()],
            Self::ToggleTag(n) => vec!["tag".into(), "toggle".into(), n.to_string()],
            Self::MoveToTag(n) => vec!["tag".into(), "move".into(), n.to_string()],
            Self::ToggleWindowTag(n) => {
                vec!["tag".into(), "window-toggle".into(), n.to_string()]
            }
            Self::FocusOutput(target) => {
                vec!["output".into(), "focus".into(), target.to_token()]
            }
            Self::SendToOutput(target) => {
                vec!["output".into(), "send".into(), target.to_token()]
            }
            Self::SetOutputFocusWarp(value) => words(&["output", "focus-warp", value.as_str()]),
            Self::ViewOutputTags {
                output,
                tags,
                until_exit,
            } => {
                let mut out = vec![
                    "output".into(),
                    "view".into(),
                    output.to_token(),
                    tags.to_token(),
                ];
                push_until_exit(&mut out, *until_exit);
                out
            }
            Self::SetPresentation {
                output,
                mode,
                until_exit,
            } => {
                let mut out = vec![
                    "output".into(),
                    "presentation".into(),
                    output.to_token(),
                    mode.as_str().into(),
                ];
                push_until_exit(&mut out, *until_exit);
                out
            }
            Self::Window { selector, op } => {
                let mut out = vec!["window".to_string()];
                out.extend(selector.to_tokens());
                out.extend(op.to_tokens());
                out
            }
        }
    }
}

fn push_until_exit(out: &mut Vec<String>, pid: Option<u32>) {
    if let Some(pid) = pid {
        out.push("until-exit".into());
        out.push(pid.to_string());
    }
}

/// `%.4f`-equivalent — matches the daemon's own summary formatting so a
/// value round-trips through Action → parse → apply unchanged.
pub(super) fn format_f32(v: f32) -> String {
    format!("{v:.4}")
}

/// Signed form; the daemon's parser treats a leading `+`/`-` as a
/// relative adjustment.
pub(super) fn format_delta_f32(delta: f32) -> String {
    format!("{delta:+.4}")
}
