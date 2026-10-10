//! Listing records: what `window list` / `output list` report.
//!
//! The daemon renders these and clients parse them, so the column
//! definitions live in exactly one place. Tab-separated form (one record
//! per line, `-` for unknown, `1`/`0` for booleans):
//!
//! ```text
//! window: id app_id title pid output tags visible floating fullscreen sticky locked focused x y width height
//! output: name x y width height tags focused presentation usable_x usable_y usable_width usable_height
//! ```
//!
//! Window geometry is the content rectangle in global compositor
//! coordinates — feed it straight to `grim -g "X,Y WxH"`.

use std::fmt::Write as _;

use crate::output::Presentation;
use crate::tags::TagSet;

pub const WINDOW_COLUMNS: [&str; 16] = [
    "id",
    "app_id",
    "title",
    "pid",
    "output",
    "tags",
    "visible",
    "floating",
    "fullscreen",
    "sticky",
    "locked",
    "focused",
    "x",
    "y",
    "width",
    "height",
];

pub const OUTPUT_COLUMNS: [&str; 12] = [
    "name",
    "x",
    "y",
    "width",
    "height",
    "tags",
    "focused",
    "presentation",
    "usable_x",
    "usable_y",
    "usable_width",
    "usable_height",
];

/// A rectangle in global compositor coordinates.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub pid: Option<u32>,
    pub output: Option<String>,
    pub tags: TagSet,
    pub visible: bool,
    pub floating: bool,
    pub fullscreen: bool,
    pub sticky: bool,
    pub locked: bool,
    pub focused: bool,
    /// Content rectangle; `None` until the window has been sized and placed.
    pub geometry: Option<Geometry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputInfo {
    /// Connector name (`DP-1`), or the 1-based index when unnamed.
    pub name: String,
    pub geometry: Geometry,
    pub tags: TagSet,
    pub focused: bool,
    pub presentation: Presentation,
    /// The area left for windows after panels' exclusive zones.
    pub usable: Geometry,
}

// ── TSV ──────────────────────────────────────────────────────────────

/// Make a free-text field safe for one TSV cell.
fn cell(s: &str) -> String {
    if s.is_empty() {
        return "-".into();
    }
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn opt_cell(s: Option<&str>) -> String {
    s.map_or_else(|| "-".into(), cell)
}

fn flag(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

fn parse_flag(s: &str) -> Result<bool, String> {
    match s {
        "1" => Ok(true),
        "0" => Ok(false),
        other => Err(format!("invalid flag {other:?}")),
    }
}

fn parse_opt<T: std::str::FromStr>(s: &str, what: &str) -> Result<Option<T>, String> {
    if s == "-" {
        return Ok(None);
    }
    s.parse()
        .map(Some)
        .map_err(|_| format!("invalid {what} {s:?}"))
}

fn parse_num<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("invalid {what} {s:?}"))
}

fn text(s: &str) -> Option<String> {
    (s != "-").then(|| s.to_string())
}

impl WindowInfo {
    pub fn to_tsv(&self) -> String {
        let (x, y, w, h) = match self.geometry {
            Some(g) => (
                g.x.to_string(),
                g.y.to_string(),
                g.width.to_string(),
                g.height.to_string(),
            ),
            None => ("-".into(), "-".into(), "-".into(), "-".into()),
        };
        [
            self.id.to_string(),
            opt_cell(self.app_id.as_deref()),
            opt_cell(self.title.as_deref()),
            self.pid.map_or_else(|| "-".into(), |p| p.to_string()),
            opt_cell(self.output.as_deref()),
            self.tags.to_token(),
            flag(self.visible).into(),
            flag(self.floating).into(),
            flag(self.fullscreen).into(),
            flag(self.sticky).into(),
            flag(self.locked).into(),
            flag(self.focused).into(),
            x,
            y,
            w,
            h,
        ]
        .join("\t")
    }

    pub fn from_tsv(line: &str) -> Result<Self, String> {
        let f: Vec<&str> = line.split('\t').collect();
        let [id, app_id, title, pid, output, tags, visible, floating, fullscreen, sticky, locked, focused, x, y, w, h] =
            f.as_slice()
        else {
            return Err(format!(
                "window record has {} fields, expected {}",
                f.len(),
                WINDOW_COLUMNS.len()
            ));
        };
        let geometry = match (
            parse_opt(x, "x")?,
            parse_opt(y, "y")?,
            parse_opt(w, "width")?,
            parse_opt(h, "height")?,
        ) {
            (Some(x), Some(y), Some(width), Some(height)) => Some(Geometry {
                x,
                y,
                width,
                height,
            }),
            _ => None,
        };
        Ok(Self {
            id: parse_num(id, "id")?,
            app_id: text(app_id),
            title: text(title),
            pid: parse_opt(pid, "pid")?,
            output: text(output),
            tags: TagSet::parse(tags)?,
            visible: parse_flag(visible)?,
            floating: parse_flag(floating)?,
            fullscreen: parse_flag(fullscreen)?,
            sticky: parse_flag(sticky)?,
            locked: parse_flag(locked)?,
            focused: parse_flag(focused)?,
            geometry,
        })
    }

    fn write_json(&self, out: &mut String) {
        out.push('{');
        let _ = write!(out, "\"id\":{}", self.id);
        push_json_field(out, "app_id", self.app_id.as_deref());
        push_json_field(out, "title", self.title.as_deref());
        match self.pid {
            Some(pid) => {
                let _ = write!(out, ",\"pid\":{pid}");
            }
            None => out.push_str(",\"pid\":null"),
        }
        push_json_field(out, "output", self.output.as_deref());
        let tags: Vec<String> = self.tags.iter().map(|t| t.to_string()).collect();
        let _ = write!(out, ",\"tags\":[{}]", tags.join(","));
        for (key, value) in [
            ("visible", self.visible),
            ("floating", self.floating),
            ("fullscreen", self.fullscreen),
            ("sticky", self.sticky),
            ("locked", self.locked),
            ("focused", self.focused),
        ] {
            let _ = write!(out, ",\"{key}\":{value}");
        }
        out.push_str(",\"geometry\":");
        match self.geometry {
            Some(g) => write_geometry(out, g),
            None => out.push_str("null"),
        }
        out.push('}');
    }
}

impl OutputInfo {
    pub fn to_tsv(&self) -> String {
        let g = self.geometry;
        let u = self.usable;
        [
            cell(&self.name),
            g.x.to_string(),
            g.y.to_string(),
            g.width.to_string(),
            g.height.to_string(),
            self.tags.to_token(),
            flag(self.focused).into(),
            self.presentation.as_str().into(),
            u.x.to_string(),
            u.y.to_string(),
            u.width.to_string(),
            u.height.to_string(),
        ]
        .join("\t")
    }

    pub fn from_tsv(line: &str) -> Result<Self, String> {
        let f: Vec<&str> = line.split('\t').collect();
        let [name, x, y, w, h, tags, focused, presentation, ux, uy, uw, uh] = f.as_slice() else {
            return Err(format!(
                "output record has {} fields, expected {}",
                f.len(),
                OUTPUT_COLUMNS.len()
            ));
        };
        Ok(Self {
            name: (*name).to_string(),
            geometry: Geometry {
                x: parse_num(x, "x")?,
                y: parse_num(y, "y")?,
                width: parse_num(w, "width")?,
                height: parse_num(h, "height")?,
            },
            tags: TagSet::parse(tags)?,
            focused: parse_flag(focused)?,
            presentation: Presentation::parse(presentation)?,
            usable: Geometry {
                x: parse_num(ux, "usable_x")?,
                y: parse_num(uy, "usable_y")?,
                width: parse_num(uw, "usable_width")?,
                height: parse_num(uh, "usable_height")?,
            },
        })
    }

    fn write_json(&self, out: &mut String) {
        out.push('{');
        out.push_str("\"name\":");
        push_json_string(out, &self.name);
        out.push_str(",\"geometry\":");
        write_geometry(out, self.geometry);
        let tags: Vec<String> = self.tags.iter().map(|t| t.to_string()).collect();
        let _ = write!(
            out,
            ",\"tags\":[{}],\"focused\":{},\"presentation\":\"{}\",\"usable\":",
            tags.join(","),
            self.focused,
            self.presentation.as_str()
        );
        write_geometry(out, self.usable);
        out.push('}');
    }
}

/// Render records as TSV lines (optionally under a header) or JSON.
pub fn render_windows(windows: &[WindowInfo], format: crate::ListFormat) -> String {
    render(
        windows,
        format,
        &WINDOW_COLUMNS,
        WindowInfo::to_tsv,
        WindowInfo::write_json,
    )
}

pub fn render_outputs(outputs: &[OutputInfo], format: crate::ListFormat) -> String {
    render(
        outputs,
        format,
        &OUTPUT_COLUMNS,
        OutputInfo::to_tsv,
        OutputInfo::write_json,
    )
}

fn render<T>(
    items: &[T],
    format: crate::ListFormat,
    columns: &[&str],
    tsv: fn(&T) -> String,
    json: fn(&T, &mut String),
) -> String {
    use crate::ListFormat;
    match format {
        ListFormat::Json => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json(item, &mut out);
            }
            out.push(']');
            out
        }
        ListFormat::Tsv | ListFormat::TsvWithHeader => {
            let mut lines = Vec::with_capacity(items.len() + 1);
            if format == ListFormat::TsvWithHeader {
                lines.push(columns.join("\t"));
            }
            lines.extend(items.iter().map(tsv));
            lines.join("\n")
        }
    }
}

/// Parse a TSV listing (header lines starting with a column name are
/// skipped).
pub fn parse_windows(body: &str) -> Result<Vec<WindowInfo>, String> {
    body.lines()
        .filter(|l| !l.is_empty() && !l.starts_with("id\t"))
        .map(WindowInfo::from_tsv)
        .collect()
}

pub fn parse_outputs(body: &str) -> Result<Vec<OutputInfo>, String> {
    body.lines()
        .filter(|l| !l.is_empty() && !l.starts_with("name\t"))
        .map(OutputInfo::from_tsv)
        .collect()
}

// ── JSON helpers ─────────────────────────────────────────────────────

fn write_geometry(out: &mut String, g: Geometry) {
    let _ = write!(
        out,
        "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
        g.x, g.y, g.width, g.height
    );
}

fn push_json_field(out: &mut String, key: &str, value: Option<&str>) {
    let _ = write!(out, ",\"{key}\":");
    match value {
        Some(v) => push_json_string(out, v),
        None => out.push_str("null"),
    }
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ListFormat;

    fn window() -> WindowInfo {
        WindowInfo {
            id: 7,
            app_id: Some("wattbench-42".into()),
            title: Some("tab\there \"quoted\"".into()),
            pid: Some(1234),
            output: Some("DP-2".into()),
            tags: TagSet::single(9).unwrap(),
            visible: true,
            floating: true,
            fullscreen: false,
            sticky: false,
            locked: true,
            focused: false,
            geometry: Some(Geometry {
                x: 100,
                y: -20,
                width: 1280,
                height: 720,
            }),
        }
    }

    #[test]
    fn window_tsv_round_trips_modulo_sanitising() {
        let w = window();
        let line = w.to_tsv();
        assert_eq!(line.split('\t').count(), WINDOW_COLUMNS.len());
        let back = WindowInfo::from_tsv(&line).unwrap();
        assert_eq!(back.title.as_deref(), Some("tab here \"quoted\""));
        assert_eq!(
            WindowInfo {
                title: w.title.clone(),
                ..back
            },
            w
        );

        let bare = WindowInfo {
            app_id: None,
            title: None,
            pid: None,
            output: None,
            geometry: None,
            ..window()
        };
        assert_eq!(WindowInfo::from_tsv(&bare.to_tsv()).unwrap(), bare);
    }

    #[test]
    fn output_tsv_round_trips() {
        let o = OutputInfo {
            name: "DP-2".into(),
            geometry: Geometry {
                x: 0,
                y: 0,
                width: 3440,
                height: 1440,
            },
            tags: TagSet::parse("1,9").unwrap(),
            focused: true,
            presentation: Presentation::Async,
            usable: Geometry {
                x: 0,
                y: 30,
                width: 3440,
                height: 1410,
            },
        };
        assert_eq!(OutputInfo::from_tsv(&o.to_tsv()).unwrap(), o);
        let listing = render_outputs(std::slice::from_ref(&o), ListFormat::TsvWithHeader);
        assert_eq!(parse_outputs(&listing).unwrap(), vec![o]);
    }

    #[test]
    fn listings_render_headers_and_json() {
        let ws = vec![window()];
        let tsv = render_windows(&ws, ListFormat::TsvWithHeader);
        assert!(tsv.starts_with("id\tapp_id\t"));
        assert_eq!(parse_windows(&tsv).unwrap().len(), 1);
        assert_eq!(render_windows(&[], ListFormat::Tsv), "");
        assert_eq!(render_windows(&[], ListFormat::Json), "[]");

        let json = render_windows(&ws, ListFormat::Json);
        assert!(json.starts_with("[{\"id\":7,\"app_id\":\"wattbench-42\""));
        assert!(json.contains("\"title\":\"tab\\there \\\"quoted\\\"\""));
        assert!(json.contains("\"tags\":[9]"));
        assert!(json.contains("\"geometry\":{\"x\":100,\"y\":-20,\"width\":1280,\"height\":720}"));
    }

    #[test]
    fn malformed_records_error() {
        assert!(WindowInfo::from_tsv("1\t2").is_err());
        assert!(OutputInfo::from_tsv("").is_err());
        let mut fields: Vec<String> = window().to_tsv().split('\t').map(String::from).collect();
        fields[6] = "yes".into();
        assert!(WindowInfo::from_tsv(&fields.join("\t")).is_err());
    }
}
