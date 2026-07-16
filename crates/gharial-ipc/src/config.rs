//! Compile-time-checked configuration vocabulary for writing a gharial
//! config in Rust.
//!
//! This module sits on top of the raw [`Client`] / [`Action`] surface and
//! trades a little ceremony for a strong guarantee: **a value that would
//! make the daemon misbehave at runtime is rejected by the compiler.**
//!
//! The three macros do the work:
//!
//! - [`ratio!`](crate::ratio) — a `main-ratio` literal outside
//!   `0.05..=0.95` is a compile error instead of a silent clamp.
//! - [`tag!`](crate::tag) — a tag number outside `1..=32` is a compile
//!   error instead of a dropped request.
//! - [`chord!`](crate::chord) — a misspelled modifier or key
//!   (`"Supr+Q"`, `"Super+Quit"`) is a compile error instead of a
//!   binding that silently never fires.
//!
//! Everything is still available at the low level: [`Action`] has a typed
//! constructor for every verb, [`Client`] sends them, and
//! [`Client::raw`](crate::Client::raw) is the escape hatch for anything
//! newer than this module.
//!
//! ```no_run
//! use gharial_ipc::{chord, config::*, ratio, tag};
//! use std::time::Duration;
//!
//! # fn main() -> gharial_ipc::Result<()> {
//! let g = Client::new();
//! g.wait_until_ready(Duration::from_secs(2))?;
//!
//! Layout::new()
//!     .main_ratio(ratio!(0.55)) // ratio!(1.5) would not compile
//!     .gaps(8)
//!     .orientation(Orientation::Left)
//!     .border_color_focused(Color::rgb(0xC8, 0x32, 0x4B))
//!     .apply(&g)?;
//!
//! Bindings::new()
//!     .bind(chord!("Super+Q"), Action::Close)
//!     .bind(chord!("Super+F"), Action::ToggleFullscreen)
//!     .bind(chord!("Super+1"), tag!(1).focus()) // tag!(33) would not compile
//!     .bind(chord!("Super+L"), Action::focus(Direction::Next))
//!     .apply(&g)?;
//! # Ok(())
//! # }
//! ```

// Re-export the low-level vocabulary so `use gharial_ipc::config::*` is
// all a config needs. (`Result` is intentionally *not* glob-exported
// here: the crate's `Result<T>` alias is single-parameter and would
// shadow the two-parameter `std::result::Result` this module uses for
// `try_new`. Reach for it as `gharial_ipc::Result`.)
pub use crate::{
    Action, BindingSpec, BoolValue, Client, Color, Direction, Error, Orientation, OutputTarget,
};

/// Inclusive lower bound the daemon clamps `main-ratio` to.
pub const RATIO_MIN: f32 = 0.05;
/// Inclusive upper bound the daemon clamps `main-ratio` to.
pub const RATIO_MAX: f32 = 0.95;
/// Lowest tag number (tags are 1-indexed: 1..=32).
pub const TAG_MIN: u8 = 1;
/// Highest tag number.
pub const TAG_MAX: u8 = 32;

/// A value that was out of its valid range. Returned by the `try_new`
/// constructors, which are the runtime counterpart of the compile-time
/// macros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeError {
    /// Human-readable description of the bound that was violated.
    pub message: &'static str,
}

impl core::fmt::Display for RangeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for RangeError {}

// ── Ratio ────────────────────────────────────────────────────────────

/// A validated `main-ratio` — the fraction of the long axis the main area
/// gets. Always within `0.05..=0.95`.
///
/// Construct it with the [`ratio!`](crate::ratio) macro for a literal
/// (checked at compile time) or [`Ratio::try_new`] for a runtime value.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Ratio(f32);

impl Ratio {
    /// Const constructor used by [`ratio!`](crate::ratio). Panics — i.e.
    /// fails compilation when evaluated in a const context — if `v` is
    /// out of range.
    pub const fn new(v: f32) -> Ratio {
        assert!(
            v >= RATIO_MIN && v <= RATIO_MAX,
            "main-ratio must be within 0.05..=0.95"
        );
        Ratio(v)
    }

    /// Runtime-checked constructor for a dynamic value.
    pub fn try_new(v: f32) -> Result<Ratio, RangeError> {
        if (RATIO_MIN..=RATIO_MAX).contains(&v) {
            Ok(Ratio(v))
        } else {
            Err(RangeError {
                message: "main-ratio must be within 0.05..=0.95",
            })
        }
    }

    /// The underlying fraction.
    pub const fn get(self) -> f32 {
        self.0
    }
}

// ── Tag ──────────────────────────────────────────────────────────────

/// A validated tag number, always within `1..=32`.
///
/// Construct it with the [`tag!`](crate::tag) macro for a literal
/// (checked at compile time) or [`Tag::try_new`] for a runtime value.
/// The action helpers ([`Tag::focus`] etc.) turn it into the matching
/// [`Action`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Tag(u8);

impl Tag {
    /// Const constructor used by [`tag!`](crate::tag). Fails compilation
    /// when evaluated in a const context if `n` is out of range.
    pub const fn new(n: u8) -> Tag {
        assert!(n >= TAG_MIN && n <= TAG_MAX, "tag must be within 1..=32");
        Tag(n)
    }

    /// Runtime-checked constructor for a dynamic value.
    pub fn try_new(n: u8) -> Result<Tag, RangeError> {
        if (TAG_MIN..=TAG_MAX).contains(&n) {
            Ok(Tag(n))
        } else {
            Err(RangeError {
                message: "tag must be within 1..=32",
            })
        }
    }

    /// The tag number, `1..=32`.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// View only this tag (replacing the active set).
    pub fn focus(self) -> Action {
        Action::FocusTag(self.0)
    }
    /// Add/remove this tag from the active set.
    pub fn toggle(self) -> Action {
        Action::ToggleTag(self.0)
    }
    /// Send the focused window to this tag.
    pub fn send_window(self) -> Action {
        Action::MoveToTag(self.0)
    }
    /// Add/remove this tag from the focused window's membership.
    pub fn toggle_window(self) -> Action {
        Action::ToggleWindowTag(self.0)
    }
}

// ── Layout builder ───────────────────────────────────────────────────

/// Fluent builder for the layout and border parameters. Each setter takes
/// a typed value, so the only way to reach the daemon with a bad
/// `main-ratio` is to bypass this builder entirely.
///
/// The setters queue [`Action`]s; [`Layout::apply`] sends them in order
/// and stops at the first daemon error.
#[derive(Default, Clone, Debug)]
pub struct Layout {
    actions: Vec<Action>,
}

impl Layout {
    /// A builder with no parameters set yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fraction of the long axis given to the main area.
    pub fn main_ratio(mut self, ratio: Ratio) -> Self {
        self.actions.push(Action::set_main_ratio(ratio.get()));
        self
    }
    /// Number of windows in the main area.
    pub fn main_count(mut self, count: u32) -> Self {
        self.actions.push(Action::set_main_count(count));
        self
    }
    /// Gap between adjacent windows, in pixels.
    pub fn gaps(mut self, px: u32) -> Self {
        self.actions.push(Action::set_gaps(px));
        self
    }
    /// Padding around the whole usable area, in pixels.
    pub fn outer_padding(mut self, px: u32) -> Self {
        self.actions.push(Action::set_outer_padding(px));
        self
    }
    /// Main-area placement.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.actions.push(Action::set_orientation(orientation));
        self
    }
    /// Drop gaps/padding when only one window is visible.
    pub fn smart_gaps(mut self, on: bool) -> Self {
        self.actions
            .push(Action::set_smart_gaps(BoolValue::from(on)));
        self
    }
    /// Border thickness, in pixels (`0` disables borders).
    pub fn border_width(mut self, px: u32) -> Self {
        self.actions.push(Action::set_border_width(px));
        self
    }
    /// Border colour of the focused window.
    pub fn border_color_focused(mut self, color: Color) -> Self {
        self.actions.push(Action::set_border_color_focused(color));
        self
    }
    /// Border colour of unfocused windows.
    pub fn border_color_unfocused(mut self, color: Color) -> Self {
        self.actions.push(Action::set_border_color_unfocused(color));
        self
    }

    /// The queued actions, in the order they'll be applied. Useful for
    /// composing into a larger [`Config`] or for testing.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// Send every queued parameter to the daemon, stopping at the first
    /// error.
    pub fn apply(&self, client: &Client) -> crate::Result<()> {
        for action in &self.actions {
            client.execute(action.clone())?;
        }
        Ok(())
    }
}

// ── Bindings builder ─────────────────────────────────────────────────

/// One keyboard binding: a chord, an action, and the mode it lives in.
#[derive(Clone, Debug)]
struct Binding {
    mode: Option<String>,
    chord: String,
    action: Action,
}

/// Fluent builder for keyboard bindings. Pair it with the
/// [`chord!`](crate::chord) macro so a malformed chord is a compile
/// error rather than a binding that silently never fires.
#[derive(Default, Clone, Debug)]
pub struct Bindings {
    entries: Vec<Binding>,
}

impl Bindings {
    /// An empty binding set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind `chord` to `action` in the default mode.
    pub fn bind(mut self, chord: &str, action: Action) -> Self {
        self.entries.push(Binding {
            mode: None,
            chord: chord.to_string(),
            action,
        });
        self
    }

    /// Bind `chord` to `action` in a named mode (auto-registered).
    pub fn bind_in_mode(mut self, mode: &str, chord: &str, action: Action) -> Self {
        self.entries.push(Binding {
            mode: Some(mode.to_string()),
            chord: chord.to_string(),
            action,
        });
        self
    }

    /// Install every binding, stopping at the first error.
    pub fn apply(&self, client: &Client) -> crate::Result<()> {
        for entry in &self.entries {
            match &entry.mode {
                Some(mode) => client.bind_in_mode(mode, &entry.chord, entry.action.clone())?,
                None => client.bind(&entry.chord, entry.action.clone())?,
            }
        }
        Ok(())
    }
}

// ── Top-level config ─────────────────────────────────────────────────

/// A whole config: layout/border parameters, output-focus behaviour,
/// bindings, and autostart programs, applied in that order by
/// [`Config::apply`]. Physical output layout belongs to the compositor's
/// output manager (for example kanshi), not to gharial.
///
/// This is sugar over applying a [`Layout`] and [`Bindings`] yourself —
/// use it when you want one `apply` call for the entire session.
#[derive(Default, Clone, Debug)]
pub struct Config {
    layout: Layout,
    bindings: Bindings,
    output_focus_warp: Option<bool>,
    autostart: Vec<Vec<String>>,
}

impl Config {
    /// An empty config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the layout/border parameters.
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Replace the binding set.
    pub fn bindings(mut self, bindings: Bindings) -> Self {
        self.bindings = bindings;
        self
    }

    /// Configure whether `output focus` warps the pointer to the newly
    /// focused output. The daemon defaults to enabled when this setting
    /// is omitted.
    pub fn warp_pointer_on_output_focus(mut self, enabled: bool) -> Self {
        self.output_focus_warp = Some(enabled);
        self
    }

    /// Queue an autostart program (argv form: command first, then args).
    pub fn spawn<I, S>(mut self, argv: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.autostart
            .push(argv.into_iter().map(Into::into).collect());
        self
    }

    /// Apply the layout, output-focus warp setting, and bindings in that
    /// order, then fire the autostart programs — stopping at the first
    /// daemon error.
    pub fn apply(&self, client: &Client) -> crate::Result<()> {
        self.layout.apply(client)?;
        if let Some(enabled) = self.output_focus_warp {
            client.set_output_focus_warp(enabled)?;
        }
        self.bindings.apply(client)?;
        for argv in &self.autostart {
            let Some((cmd, args)) = argv.split_first() else {
                continue;
            };
            client.spawn(cmd, args)?;
        }
        Ok(())
    }
}

// ── Macros ───────────────────────────────────────────────────────────

/// Build a [`Ratio`](crate::config::Ratio) from a literal, checked at
/// compile time. A value outside `0.05..=0.95` fails to compile.
///
/// ```
/// use gharial_ipc::{config::Ratio, ratio};
/// const R: Ratio = ratio!(0.55);
/// assert_eq!(R.get(), 0.55);
/// ```
#[macro_export]
macro_rules! ratio {
    ($v:expr $(,)?) => {{
        const R: $crate::config::Ratio = $crate::config::Ratio::new($v);
        R
    }};
}

/// Build a [`Tag`](crate::config::Tag) from a literal, checked at compile
/// time. A value outside `1..=32` fails to compile.
///
/// ```
/// use gharial_ipc::{config::Tag, tag};
/// const T: Tag = tag!(3);
/// assert_eq!(T.get(), 3);
/// ```
#[macro_export]
macro_rules! tag {
    ($n:expr $(,)?) => {{
        const T: $crate::config::Tag = $crate::config::Tag::new($n);
        T
    }};
}

/// Validate a chord string at compile time and evaluate to that same
/// string, ready to hand to [`Client::bind`](crate::Client::bind). An
/// unknown modifier or key, or an empty chord, fails to compile.
///
/// ```
/// use gharial_ipc::chord;
/// let c: &str = chord!("Super+Shift+Q");
/// assert_eq!(c, "Super+Shift+Q");
/// ```
#[macro_export]
macro_rules! chord {
    ($s:literal) => {{
        // Force compile-time validation; the spec itself is discarded
        // because the daemon re-parses the string off the wire.
        const _CHECK: $crate::BindingSpec = $crate::BindingSpec::const_checked($s);
        $s
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_const_and_runtime_agree() {
        const R: Ratio = Ratio::new(0.55);
        assert_eq!(R.get(), 0.55);
        assert_eq!(Ratio::try_new(0.55).unwrap().get(), 0.55);
        assert!(Ratio::try_new(1.5).is_err());
        assert!(Ratio::try_new(0.0).is_err());
    }

    #[test]
    fn tag_const_and_runtime_agree() {
        const T: Tag = Tag::new(32);
        assert_eq!(T.get(), 32);
        assert!(Tag::try_new(0).is_err());
        assert!(Tag::try_new(33).is_err());
        assert_eq!(Tag::try_new(1).unwrap().get(), 1);
    }

    #[test]
    fn tag_action_helpers_map_to_the_right_variants() {
        assert!(matches!(Tag::new(3).focus(), Action::FocusTag(3)));
        assert!(matches!(Tag::new(3).toggle(), Action::ToggleTag(3)));
        assert!(matches!(Tag::new(3).send_window(), Action::MoveToTag(3)));
        assert!(matches!(
            Tag::new(3).toggle_window(),
            Action::ToggleWindowTag(3)
        ));
    }

    #[test]
    fn ratio_macro_evaluates_at_compile_time() {
        const R: Ratio = ratio!(0.7);
        assert_eq!(R.get(), 0.7);
    }

    #[test]
    fn tag_macro_evaluates_at_compile_time() {
        const T: Tag = tag!(9);
        assert_eq!(T.get(), 9);
    }

    #[test]
    fn chord_macro_validates_and_yields_the_string() {
        let c: &str = chord!("Super+Shift+Q");
        assert_eq!(c, "Super+Shift+Q");
        // And what it validated matches the runtime parser.
        let spec = BindingSpec::parse(c).unwrap();
        assert_eq!(spec, BindingSpec::parse_const(c).unwrap());
    }

    #[test]
    fn output_focus_warp_builder_is_optional_and_last_value_wins() {
        assert_eq!(Config::new().output_focus_warp, None);
        assert_eq!(
            Config::new()
                .warp_pointer_on_output_focus(false)
                .output_focus_warp,
            Some(false)
        );
        assert_eq!(
            Config::new()
                .warp_pointer_on_output_focus(false)
                .warp_pointer_on_output_focus(true)
                .output_focus_warp,
            Some(true)
        );
    }

    #[test]
    fn layout_builder_queues_actions_in_order() {
        let layout = Layout::new()
            .main_ratio(Ratio::new(0.6))
            .gaps(8)
            .orientation(Orientation::Left);
        let tokens: Vec<Vec<String>> = layout.actions().iter().map(Action::to_tokens).collect();
        assert_eq!(tokens[0], vec!["main-ratio".to_string(), "0.6000".into()]);
        assert_eq!(tokens[1], vec!["gaps".to_string(), "8".into()]);
        assert_eq!(tokens[2], vec!["orientation".to_string(), "left".into()]);
    }

    #[test]
    fn const_checked_rejects_bad_chords_at_runtime_too() {
        // The const path returns Err rather than panicking when called
        // outside a const context.
        assert!(BindingSpec::parse_const("Supr+Q").is_err());
        assert!(BindingSpec::parse_const("Super+Quit").is_err());
        assert!(BindingSpec::parse_const("").is_err());
        assert!(BindingSpec::parse_const("Super+Q").is_ok());
    }
}
