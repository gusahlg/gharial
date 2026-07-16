//! Output-selection vocabulary shared by the daemon, CLI, and Rust configs.

use crate::action::Direction;

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
            let target = OutputTarget::parse(token);
            assert_eq!(OutputTarget::parse(&target.to_token()), target);
        }
    }
}
