//! Command — a whole IPC request, typed.
//!
//! [`Command::parse`] turns a wire [`Request`] into a `Command` and
//! [`Command::to_request`] goes back, so the daemon, `gharialctl`, and
//! the Rust [`Client`](crate::Client) share one grammar:
//!
//! ```text
//! ping | version | status | get KEY | set KEY VALUE | KEY VALUE
//! bind [--mode M] CHORD ACTION... | unbind [--mode M] CHORD
//! window list [SELECTOR] [--json] [--header]
//! output list [--json] [--header]
//! rule add [--until-exit PID] RULE... | rule list | rule del ID | rule clear
//! wait-window SELECTOR [--timeout DUR] [--new]
//! ACTION...                          (close, focus, spawn, window, output, …)
//! ```

use std::time::Duration;

use crate::action::{is_layout_key, Action, BindingSpec};
use crate::rule::Rule;
use crate::window::WindowSelector;
use crate::wire::Request;

/// Mode bindings go to when `bind` names none.
pub const DEFAULT_MODE: &str = "default";

/// How long `wait-window` waits when no `--timeout` is given.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(10);

/// Longest `wait-window` timeout accepted.
pub const MAX_WAIT: Duration = Duration::from_secs(3600);

/// Output style of the listing verbs.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ListFormat {
    /// Tab-separated lines; `header` prepends a line of column names.
    #[default]
    Tsv,
    TsvWithHeader,
    /// A JSON array of objects.
    Json,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Ping,
    Version,
    /// All settings as one `key=value;…` line.
    Status,
    /// One setting.
    Get(String),
    /// A state change (bindable action, layout setting, window command).
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
    ListWindows {
        selector: Option<WindowSelector>,
        format: ListFormat,
    },
    ListOutputs {
        format: ListFormat,
    },
    AddRule {
        rule: Rule,
        /// Drop the rule when this process exits.
        until_exit: Option<u32>,
    },
    ListRules,
    RemoveRule(u64),
    ClearRules,
    /// Block until a window matching `selector` has been placed and
    /// drawn at its final size; answers with its id.
    WaitWindow {
        selector: WindowSelector,
        timeout: Duration,
        /// Ignore windows already mapped when the wait began.
        new_only: bool,
    },
}

impl Command {
    pub fn parse(req: &Request) -> Result<Self, String> {
        let args: Vec<&str> = req.args.iter().map(String::as_str).collect();
        let cmd = req.command.as_str();
        match (cmd, args.as_slice()) {
            ("ping", _) => Ok(Self::Ping),
            ("version", _) => Ok(Self::Version),
            ("status", _) => Ok(Self::Status),
            ("get", [key]) => Ok(Self::Get((*key).to_string())),
            ("get", _) => Err("get: expected <key>".into()),
            ("set", [key, rest @ ..]) => {
                if !is_layout_key(key) {
                    return Err(format!("unknown key: {key}"));
                }
                if rest.is_empty() {
                    return Err(format!("{key}: missing value"));
                }
                Ok(Self::Action(Action::Layout {
                    key: (*key).to_string(),
                    args: rest.iter().map(|s| (*s).to_string()).collect(),
                }))
            }
            ("set", []) => Err("set: missing key".into()),
            ("bind", _) => parse_bind(&args),
            ("unbind", _) => parse_unbind(&args),
            ("window", ["list", rest @ ..]) => {
                let (format, rest) = split_format(rest)?;
                let selector = match rest.as_slice() {
                    [] => None,
                    sel => Some(WindowSelector::parse(sel)?),
                };
                Ok(Self::ListWindows { selector, format })
            }
            ("output", ["list", rest @ ..]) => {
                let (format, rest) = split_format(rest)?;
                match rest.as_slice() {
                    [] => Ok(Self::ListOutputs { format }),
                    [extra, ..] => Err(format!("output list: unexpected {extra}")),
                }
            }
            ("rule", rest) => parse_rule(rest),
            ("wait-window", rest) => parse_wait_window(rest),
            _ => {
                let mut tokens = Vec::with_capacity(args.len() + 1);
                tokens.push(cmd);
                tokens.extend_from_slice(&args);
                Action::parse(&tokens).map(Self::Action).map_err(|e| {
                    if e.starts_with("unknown action") {
                        format!("unknown command: {cmd}")
                    } else {
                        e
                    }
                })
            }
        }
    }

    /// Encode for the wire. Round-trips with [`Command::parse`].
    pub fn to_request(&self) -> Request {
        let s = |v: &str| v.to_string();
        let (command, args): (String, Vec<String>) = match self {
            Self::Ping => (s("ping"), vec![]),
            Self::Version => (s("version"), vec![]),
            Self::Status => (s("status"), vec![]),
            Self::Get(key) => (s("get"), vec![key.clone()]),
            Self::Action(action) => {
                let mut tokens = action.to_tokens().into_iter();
                let command = tokens.next().unwrap_or_default();
                (command, tokens.collect())
            }
            Self::Bind { mode, spec, action } => {
                let mut args = vec![s("--mode"), mode.clone(), spec.to_chord()];
                args.extend(action.to_tokens());
                (s("bind"), args)
            }
            Self::Unbind { mode, spec } => (
                s("unbind"),
                vec![s("--mode"), mode.clone(), spec.to_chord()],
            ),
            Self::ListWindows { selector, format } => {
                let mut args = vec![s("list")];
                if let Some(sel) = selector {
                    args.extend(sel.to_tokens());
                }
                push_format(&mut args, *format);
                (s("window"), args)
            }
            Self::ListOutputs { format } => {
                let mut args = vec![s("list")];
                push_format(&mut args, *format);
                (s("output"), args)
            }
            Self::AddRule { rule, until_exit } => {
                let mut args = vec![s("add")];
                if let Some(pid) = until_exit {
                    args.push(s("--until-exit"));
                    args.push(pid.to_string());
                }
                args.extend(rule.to_tokens());
                (s("rule"), args)
            }
            Self::ListRules => (s("rule"), vec![s("list")]),
            Self::RemoveRule(id) => (s("rule"), vec![s("del"), id.to_string()]),
            Self::ClearRules => (s("rule"), vec![s("clear")]),
            Self::WaitWindow {
                selector,
                timeout,
                new_only,
            } => {
                let mut args = selector.to_tokens();
                args.push(s("--timeout"));
                args.push(format!("{}ms", timeout.as_millis()));
                if *new_only {
                    args.push(s("--new"));
                }
                (s("wait-window"), args)
            }
        };
        Request::new(command, args)
    }
}

impl BindingSpec {
    /// A chord string that parses back to this spec: modifier names plus
    /// the keysym in hex (always resolvable, never ambiguous).
    pub fn to_chord(&self) -> String {
        let mut out = String::new();
        for (name, bit) in crate::keysyms::MODIFIER_NAMES {
            if self.modifiers & bit != 0 {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(&format!("0x{:x}", self.keysym));
        out
    }
}

fn split_mode<'t, 'a>(args: &'t [&'a str]) -> Result<(String, &'t [&'a str]), String> {
    match args {
        ["--mode", mode, rest @ ..] => Ok(((*mode).to_string(), rest)),
        ["--mode"] => Err("--mode requires a name".into()),
        [first, rest @ ..] if first.starts_with("--mode=") => {
            Ok((first["--mode=".len()..].to_string(), rest))
        }
        _ => Ok((DEFAULT_MODE.to_string(), args)),
    }
}

fn parse_bind(args: &[&str]) -> Result<Command, String> {
    let (mode, rest) = split_mode(args)?;
    let (chord, action) = rest
        .split_first()
        .ok_or("bind: expected <chord> <action ...>")?;
    let spec = BindingSpec::parse(chord)?;
    let action = Action::parse(action)?;
    Ok(Command::Bind { mode, spec, action })
}

fn parse_unbind(args: &[&str]) -> Result<Command, String> {
    let (mode, rest) = split_mode(args)?;
    let [chord] = rest else {
        return Err("unbind: expected <chord>".into());
    };
    Ok(Command::Unbind {
        mode,
        spec: BindingSpec::parse(chord)?,
    })
}

fn split_format<'a>(args: &[&'a str]) -> Result<(ListFormat, Vec<&'a str>), String> {
    let mut format = ListFormat::Tsv;
    let mut rest = Vec::with_capacity(args.len());
    for &arg in args {
        match arg {
            "--json" => format = ListFormat::Json,
            "--header" if format != ListFormat::Json => format = ListFormat::TsvWithHeader,
            "--header" => {}
            other => rest.push(other),
        }
    }
    Ok((format, rest))
}

fn push_format(args: &mut Vec<String>, format: ListFormat) {
    match format {
        ListFormat::Tsv => {}
        ListFormat::TsvWithHeader => args.push("--header".into()),
        ListFormat::Json => args.push("--json".into()),
    }
}

fn parse_pid(raw: &str) -> Result<u32, String> {
    match raw.parse::<u32>() {
        Ok(pid) if pid > 0 => Ok(pid),
        _ => Err(format!("invalid pid {raw:?}")),
    }
}

fn parse_rule(args: &[&str]) -> Result<Command, String> {
    match args {
        ["add", rest @ ..] => {
            let mut until_exit = None;
            let mut tokens = Vec::with_capacity(rest.len());
            let mut iter = rest.iter();
            while let Some(&tok) = iter.next() {
                match tok {
                    "--until-exit" | "until-exit" => {
                        let pid = iter.next().ok_or("rule add: until-exit needs a pid")?;
                        until_exit = Some(parse_pid(pid)?);
                    }
                    t if t.starts_with("--until-exit=") || t.starts_with("until-exit=") => {
                        let (_, pid) = t.split_once('=').unwrap_or_default();
                        until_exit = Some(parse_pid(pid)?);
                    }
                    t => tokens.push(t),
                }
            }
            Ok(Command::AddRule {
                rule: Rule::parse(&tokens)?,
                until_exit,
            })
        }
        ["list"] => Ok(Command::ListRules),
        ["del" | "delete" | "remove" | "rm", id] => id
            .parse()
            .map(Command::RemoveRule)
            .map_err(|_| format!("rule del: invalid id {id:?}")),
        ["clear"] => Ok(Command::ClearRules),
        _ => Err("rule: expected add RULE… | list | del ID | clear".into()),
    }
}

fn parse_wait_window(args: &[&str]) -> Result<Command, String> {
    let mut timeout = DEFAULT_WAIT;
    let mut new_only = false;
    let mut selector_tokens = Vec::new();
    let mut iter = args.iter();
    while let Some(&tok) = iter.next() {
        match tok {
            "--timeout" | "-t" => {
                let raw = iter.next().ok_or("wait-window: --timeout needs a value")?;
                timeout = parse_duration(raw)?;
            }
            t if t.starts_with("--timeout=") => timeout = parse_duration(&t["--timeout=".len()..])?,
            "--new" => new_only = true,
            t => selector_tokens.push(t),
        }
    }
    if timeout > MAX_WAIT {
        return Err(format!(
            "wait-window: timeout above {}s",
            MAX_WAIT.as_secs()
        ));
    }
    Ok(Command::WaitWindow {
        selector: WindowSelector::parse(&selector_tokens)?,
        timeout,
        new_only,
    })
}

/// `1.5s`, `250ms`, `2m`, or a bare number of seconds.
pub fn parse_duration(raw: &str) -> Result<Duration, String> {
    let bad = || format!("invalid duration {raw:?} (e.g. 10s, 500ms, 2m)");
    let split = raw
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(raw.len());
    let (num, unit) = raw.split_at(split);
    let value: f64 = num.parse().map_err(|_| bad())?;
    let secs = match unit {
        "" | "s" => value,
        "ms" => value / 1000.0,
        "m" | "min" => value * 60.0,
        _ => return Err(bad()),
    };
    Duration::try_from_secs_f64(secs).map_err(|_| bad())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::Matcher;
    use crate::placement::Placement;
    use crate::value::BoolValue;

    fn parse(line: &str) -> Result<Command, String> {
        Command::parse(&Request::parse(line).unwrap())
    }

    fn round_trip(cmd: Command) {
        let req = cmd.to_request();
        assert_eq!(Command::parse(&req).as_ref(), Ok(&cmd), "{req:?}");
        // And through the actual wire encoding.
        let wire = Request::parse(req.encode().trim_end()).unwrap();
        assert_eq!(Command::parse(&wire), Ok(cmd));
    }

    #[test]
    fn basic_verbs() {
        assert_eq!(parse("ping"), Ok(Command::Ping));
        assert_eq!(parse("get gaps"), Ok(Command::Get("gaps".into())));
        assert_eq!(
            parse("set gaps 8"),
            Ok(Command::Action(Action::set_gaps(8)))
        );
        assert_eq!(parse("gaps 8"), Ok(Command::Action(Action::set_gaps(8))));
        assert!(parse("set nope 1").is_err());
        assert!(parse("set gaps").is_err());
        assert!(parse("get").is_err());
        let err = parse("frobnicate").unwrap_err();
        assert!(err.contains("unknown command"), "{err}");
        // A known verb keeps its specific error.
        assert!(parse("focus").unwrap_err().contains("next|prev"));
    }

    #[test]
    fn bind_and_unbind() {
        let cmd = parse("bind --mode resize Super+H main-ratio -0.05").unwrap();
        assert_eq!(
            cmd,
            Command::Bind {
                mode: "resize".into(),
                spec: BindingSpec::parse("Super+H").unwrap(),
                action: Action::Layout {
                    key: "main-ratio".into(),
                    args: vec!["-0.05".into()],
                },
            }
        );
        round_trip(cmd);
        assert!(parse("bind Hyper+Q close").is_err());
        assert!(parse("bind Super+Q fly").is_err());
        assert!(parse("bind").is_err());
        assert!(parse("bind --mode").is_err());
        round_trip(parse("unbind Super+Shift+Q").unwrap());
        assert!(parse("unbind").is_err());
    }

    #[test]
    fn chords_round_trip_through_hex() {
        for chord in [
            "Super+Q",
            "Super+Shift+Return",
            "XF86AudioMute",
            "Ctrl+Alt+F1",
            "Mod5+a",
        ] {
            let spec = BindingSpec::parse(chord).unwrap();
            assert_eq!(BindingSpec::parse(&spec.to_chord()), Ok(spec), "{chord}");
        }
    }

    #[test]
    fn listings() {
        assert_eq!(
            parse("window list"),
            Ok(Command::ListWindows {
                selector: None,
                format: ListFormat::Tsv
            })
        );
        assert_eq!(
            parse("window list app-id=foo --json"),
            Ok(Command::ListWindows {
                selector: Some(WindowSelector::Match(Matcher::app_id("foo"))),
                format: ListFormat::Json
            })
        );
        round_trip(parse("window list focused --header").unwrap());
        round_trip(parse("output list --json").unwrap());
        assert!(parse("output list extra").is_err());
    }

    #[test]
    fn rules() {
        let cmd = parse("rule add --until-exit 42 app-id=wattbench-* output DP-2 float size 1280x720 nofocus lock oneshot").unwrap();
        match &cmd {
            Command::AddRule { rule, until_exit } => {
                assert_eq!(*until_exit, Some(42));
                assert!(rule.oneshot);
                assert_eq!(rule.placement.focus, Some(BoolValue::Off));
            }
            other => panic!("{other:?}"),
        }
        round_trip(cmd);
        round_trip(parse("rule list").unwrap());
        round_trip(parse("rule del 3").unwrap());
        round_trip(parse("rule clear").unwrap());
        assert!(parse("rule add float").is_err());
        assert!(parse("rule del x").is_err());
        assert!(parse("rule add until-exit 0 app-id=a float").is_err());
        assert!(parse("rule").is_err());
    }

    #[test]
    fn wait_window() {
        assert_eq!(
            parse("wait-window app-id=foo --timeout 5s --new"),
            Ok(Command::WaitWindow {
                selector: WindowSelector::Match(Matcher::app_id("foo")),
                timeout: Duration::from_secs(5),
                new_only: true,
            })
        );
        assert_eq!(
            parse("wait-window pid=7"),
            Ok(Command::WaitWindow {
                selector: WindowSelector::Match(Matcher::pid(7)),
                timeout: DEFAULT_WAIT,
                new_only: false,
            })
        );
        round_trip(parse("wait-window title=x --timeout=250ms").unwrap());
        assert!(parse("wait-window").is_err());
        assert!(parse("wait-window app-id=x --timeout").is_err());
        assert!(parse("wait-window app-id=x --timeout 2h").is_err());
    }

    #[test]
    fn actions_round_trip_as_commands() {
        for action in [
            Action::Close,
            Action::set_gaps(4),
            Action::spawn_placed(Placement::new().on_output("DP-2"), "game", ["-x"]),
            Action::spawn("rio", [] as [&str; 0]),
            Action::FocusTag(3),
        ] {
            round_trip(Command::Action(action));
        }
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("10"), Ok(Duration::from_secs(10)));
        assert_eq!(parse_duration("1.5s"), Ok(Duration::from_millis(1500)));
        assert_eq!(parse_duration("250ms"), Ok(Duration::from_millis(250)));
        assert_eq!(parse_duration("2m"), Ok(Duration::from_secs(120)));
        for bad in ["", "s", "1h", "-1", "1.2.3"] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
    }
}
