//! IPC protocol and typed control vocabulary shared between the gharial
//! daemon, `gharialctl`, and Rust-based configs.
//!
//! Wire format ([`wire`]): a single newline-terminated request line, a
//! single newline-terminated response line. Tokens are
//! whitespace-separated, with double-quoted strings supporting `\"` and
//! `\\` escapes.
//!
//! On top of the raw wire types this crate owns the *vocabulary* every
//! client speaks — [`Action`], [`Placement`], [`Rule`],
//! [`WindowSelector`], [`TagSet`], [`Color`], the keysym table, and the
//! [`Client`] handle. The daemon re-exports these (so there is exactly
//! one definition of the grammar), and because this crate has no
//! dependencies at all a config binary can depend on it alone and still
//! build in well under a second.
//!
//! For writing a configuration in Rust with compile-time-checked values
//! (out-of-range ratios/tags and malformed chords become compile errors),
//! see the [`config`] module and the `ratio!` / `tag!` / `chord!` macros.

mod args;

pub mod action;
pub mod client;
pub mod color;
pub mod command;
pub mod config;
pub mod info;
pub mod keysyms;
pub mod matcher;
pub mod orientation;
pub mod output;
pub mod placement;
pub mod rule;
pub mod tags;
pub mod value;
pub mod window;
pub mod wire;

pub use action::{Action, BindingSpec, Direction};
pub use client::{Client, Error};
pub use color::Color;
pub use command::{Command, ListFormat};
pub use info::{Geometry, OutputInfo, WindowInfo};
pub use matcher::Matcher;
pub use orientation::Orientation;
pub use output::{OutputTarget, Presentation};
pub use placement::{FloatPos, Placement};
pub use rule::Rule;
pub use tags::TagSet;
pub use value::BoolValue;
pub use window::{WindowOp, WindowSelector};
pub use wire::{
    send_one, send_with_timeout, socket_path, ParseError, Request, Response, SOCKET_BASENAME,
    SOCKET_ENV,
};

/// Outcome of a [`Client`] method. Re-exported at the crate root so
/// `gharial_ipc::Result` reads naturally in a config `main`.
pub use client::Result;
