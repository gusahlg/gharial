//! Output-edge vocabulary for multi-screen support.
//!
//! Three small types shared by the daemon, gharialctl, and Rust
//! configs:
//!
//! - [`Edge`] — one side of a screen (`left`/`right`/`top`/`bottom`).
//! - [`EdgeRef`] — a specific side of a specific output, written
//!   `OUTPUT:EDGE` on the wire (e.g. `DP-1:right`). The output part is
//!   either a connector name (`DP-1`, `HDMI-A-1`) or a 1-based index in
//!   the order the compositor advertised the outputs.
//! - [`OutputTarget`] — how `output focus` / `output send` pick a
//!   screen: a [`Direction`] (`next`/`prev` cycle advertisement order,
//!   `left`/`right`/`up`/`down` pick the spatially nearest output) or a
//!   name/index.

use std::fmt;
use std::str::FromStr;

use crate::action::Direction;

/// One side of an output.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        s.parse()
            .map_err(|()| format!("invalid edge: {s} (left|right|top|bottom)"))
    }
}

impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Edge {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, ()> {
        match s {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            _ => Err(()),
        }
    }
}

/// A specific side of a specific output — `DP-1:right` on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeRef {
    /// Connector name (`DP-1`) or 1-based advertisement index (`1`).
    pub output: String,
    pub edge: Edge,
}

impl EdgeRef {
    pub fn new(output: impl Into<String>, edge: Edge) -> Self {
        Self {
            output: output.into(),
            edge,
        }
    }

    /// Parse the `OUTPUT:EDGE` wire form. The split is on the *last*
    /// colon so a hypothetical output name containing `:` still works.
    pub fn parse(s: &str) -> Result<Self, String> {
        let Some((output, edge)) = s.rsplit_once(':') else {
            return Err(format!(
                "invalid edge reference: {s} (expected OUTPUT:EDGE, e.g. DP-1:right)"
            ));
        };
        if output.is_empty() {
            return Err(format!("invalid edge reference: {s} (empty output)"));
        }
        Ok(Self {
            output: output.to_string(),
            edge: Edge::parse(edge)?,
        })
    }

    /// Canonical wire form. Round-trips with [`EdgeRef::parse`].
    pub fn to_token(&self) -> String {
        format!("{}:{}", self.output, self.edge.as_str())
    }
}

impl fmt::Display for EdgeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.output, self.edge.as_str())
    }
}

/// Which output an `output focus` / `output send` action selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputTarget {
    /// `next`/`prev` cycle the advertisement order; the cardinal
    /// directions pick the spatially nearest output.
    Direction(Direction),
    /// A connector name (`DP-1`) or 1-based advertisement index (`2`).
    Named(String),
}

impl OutputTarget {
    /// Direction tokens win; anything else is a name/index.
    pub fn parse(s: &str) -> Self {
        match Direction::parse(s) {
            Ok(dir) => Self::Direction(dir),
            Err(_) => Self::Named(s.to_string()),
        }
    }

    /// Canonical wire token. Round-trips with [`OutputTarget::parse`]
    /// for every value `parse` can produce (a `Named` that collides
    /// with a direction token is unreachable through `parse`).
    pub fn to_token(&self) -> String {
        match self {
            Self::Direction(dir) => dir.as_str().to_string(),
            Self::Named(name) => name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_round_trips_through_str() {
        for e in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            assert_eq!(Edge::parse(e.as_str()).unwrap(), e);
        }
    }

    #[test]
    fn edge_rejects_unknown() {
        assert!(Edge::parse("diagonal").is_err());
        assert!(Edge::parse("").is_err());
    }

    #[test]
    fn edge_ref_parses_name_and_edge() {
        let r = EdgeRef::parse("DP-1:right").unwrap();
        assert_eq!(r.output, "DP-1");
        assert_eq!(r.edge, Edge::Right);
        assert_eq!(r.to_token(), "DP-1:right");
    }

    #[test]
    fn edge_ref_splits_on_last_colon() {
        let r = EdgeRef::parse("weird:name:top").unwrap();
        assert_eq!(r.output, "weird:name");
        assert_eq!(r.edge, Edge::Top);
    }

    #[test]
    fn edge_ref_rejects_malformed() {
        assert!(EdgeRef::parse("DP-1").is_err());
        assert!(EdgeRef::parse(":left").is_err());
        assert!(EdgeRef::parse("DP-1:sideways").is_err());
    }

    #[test]
    fn output_target_prefers_directions() {
        assert_eq!(
            OutputTarget::parse("next"),
            OutputTarget::Direction(Direction::Next)
        );
        assert_eq!(
            OutputTarget::parse("left"),
            OutputTarget::Direction(Direction::Left)
        );
        assert_eq!(
            OutputTarget::parse("DP-2"),
            OutputTarget::Named("DP-2".into())
        );
        assert_eq!(OutputTarget::parse("2"), OutputTarget::Named("2".into()));
    }

    #[test]
    fn output_target_round_trips() {
        for token in ["next", "prev", "left", "right", "up", "down", "DP-1", "3"] {
            let t = OutputTarget::parse(token);
            assert_eq!(OutputTarget::parse(&t.to_token()), t);
        }
    }
}
