//! Small typed value wrappers used in the public Rust API.

use std::fmt;

/// Three-state boolean for fields like `smart-gaps`. The wire form
/// distinguishes "set true / set false / flip current" — modelling it
/// here keeps `Client::set_smart_gaps(BoolValue::Toggle)` legible
/// without overloading `bool`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BoolValue {
    On,
    Off,
    Toggle,
}

impl BoolValue {
    /// Parse the canonical boolean tokens accepted by action grammars.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            "toggle" => Ok(Self::Toggle),
            other => Err(format!("invalid boolean: {other} (expected on|off|toggle)")),
        }
    }

    /// Token form accepted by the daemon's grammar.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Toggle => "toggle",
        }
    }
}

impl fmt::Display for BoolValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<bool> for BoolValue {
    fn from(b: bool) -> Self {
        if b {
            Self::On
        } else {
            Self::Off
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_grammar() {
        assert_eq!(BoolValue::On.as_str(), "on");
        assert_eq!(BoolValue::Off.as_str(), "off");
        assert_eq!(BoolValue::Toggle.as_str(), "toggle");
    }

    #[test]
    fn from_bool_yields_on_off() {
        assert_eq!(BoolValue::from(true), BoolValue::On);
        assert_eq!(BoolValue::from(false), BoolValue::Off);
    }

    #[test]
    fn display_matches_as_str() {
        for b in [BoolValue::On, BoolValue::Off, BoolValue::Toggle] {
            assert_eq!(format!("{b}"), b.as_str());
        }
    }

    #[test]
    fn parse_accepts_exactly_the_canonical_tokens() {
        assert_eq!(BoolValue::parse("on"), Ok(BoolValue::On));
        assert_eq!(BoolValue::parse("off"), Ok(BoolValue::Off));
        assert_eq!(BoolValue::parse("toggle"), Ok(BoolValue::Toggle));

        for invalid in ["", "true", "false", "yes", "no", "maybe"] {
            let error = BoolValue::parse(invalid).unwrap_err();
            assert!(
                error.contains(invalid),
                "{error:?} should mention {invalid:?}"
            );
            assert!(error.contains("on|off|toggle"));
        }
    }
}
