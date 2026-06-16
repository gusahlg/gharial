//! Rust API for configuring a running gharial daemon.
//!
//! Same vocabulary as `gharialctl` (the shell-friendly CLI), expressed
//! as typed values and methods. Use this when you want to write your
//! river init as a Rust binary instead of a shell script.
//!
//! ```no_run
//! use gharial::{Action, Client, Color, Direction, Orientation};
//! use std::time::Duration;
//!
//! fn main() -> gharial::Result<()> {
//!     // Talk to the daemon at the default socket
//!     // ($GHARIAL_SOCKET, or the XDG path).
//!     let g = Client::new();
//!     g.wait_until_ready(Duration::from_secs(2))?;
//!
//!     // Layout
//!     g.set_gaps(8)?;
//!     g.set_main_ratio(0.55)?;
//!     g.set_orientation(Orientation::Left)?;
//!
//!     // Borders
//!     g.set_border_width(3)?;
//!     g.set_border_color_focused(Color::rgba(0xC8, 0x32, 0x4B, 0xFF))?;
//!     g.set_border_color_unfocused(Color::rgba(0x00, 0xC8, 0x96, 0xFF))?;
//!
//!     // Bindings (every Action variant + the chord vocabulary
//!     // BindingSpec::parse accepts)
//!     g.bind("Super+Q", Action::Close)?;
//!     g.bind("Super+L", Action::focus(Direction::Next))?;
//!     g.bind("Super+Return", Action::spawn("rio", [] as [&str; 0]))?;
//!     g.bind("Super+1", Action::FocusTag(1))?;
//!
//!     // Modes
//!     g.bind("Super+R", Action::enter_mode("resize"))?;
//!     g.bind_in_mode("resize", "Escape", Action::ExitMode)?;
//!     g.bind_in_mode("resize", "L", Action::adjust_main_ratio(0.05))?;
//!
//!     // Autostart
//!     g.spawn("waybar", &[] as &[&str])?;
//!     Ok(())
//! }
//! ```
//!
//! # Design
//!
//! - One handle: `Client`. Cheap to construct (just stores the socket
//!   path), every method opens a fresh connection. The IPC server
//!   serves one request per connection.
//! - [`Action`] is the same type the daemon dispatches internally —
//!   there is no wire-format drift between what you bind and what fires.
//! - Layout-param mutations are typed (`set_gaps(u32)`,
//!   `set_main_ratio(f32)`, `adjust_main_ratio(f32)`); falling back to
//!   [`Client::set`] and [`Client::raw`] is always available for
//!   forwards-compatibility.

// The control vocabulary (actions, colours, orientation, the keysym
// table, the typed `Client`) lives in the dependency-light `gharial-ipc`
// crate so configs can speak it without pulling in the Wayland stack.
// Re-export the modules here so daemon-internal call sites keep their
// `crate::action::*` / `crate::keysyms::*` paths and external users of
// the `gharial` crate see the same surface they always did.
pub use gharial_ipc::{action, client, color, keysyms, orientation, value};

pub use gharial_ipc::{
    Action, BindingSpec, BoolValue, Client, Color, Direction, Error, Orientation, Result,
};

/// Resolve the default daemon socket path — same precedence as
/// `gharialctl` and `gharial` itself.
pub use gharial_ipc::socket_path;
