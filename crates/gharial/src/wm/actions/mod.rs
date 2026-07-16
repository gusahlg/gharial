//! Action execution. Called from inside a manage sequence (for state-
//! changing actions) or directly (for `Spawn`, which has no protocol
//! effect). Each action mutates `World` and may issue protocol
//! requests; everything goes through this module so adding a new
//! action is one match arm in [`execute`].
//!
//! Sub-modules group by concern, matching the user-facing action
//! vocabulary:
//!
//! | sub-module | actions                                            |
//! |------------|----------------------------------------------------|
//! | `focus`    | `FocusDirection`, internal `set_focus`/`ensure_*`  |
//! | `window`   | `Close`, `ToggleFloat`, `SwapDirection`            |
//! | `tag`      | `FocusTag` / `ToggleTag` / `MoveToTag` / window-tag |
//! | `output`   | output focus/send and focus-warp policy             |
//! | `mode`     | `EnterMode` / `ExitMode` / `Bind` / `Unbind`       |
//! | `layout`   | `Layout { key, args }`                             |
//! | `spawn`    | `Spawn { cmd, args }`                              |
//!
//! Splitting per concern lets each piece be navigated and tested in
//! isolation; this `mod.rs` only owns the top-level dispatch.

mod focus;
mod layout;
mod mode;
mod output;
mod spawn;
mod tag;
mod window;

use crate::action::Action;

use super::modes::DEFAULT_MODE;
use super::world::World;

// Re-export the symbols the wayland dispatch layer still pokes at by
// name. Everything else stays module-private.
pub(super) use focus::{ensure_focus_invariant, forget_window, set_focus};
pub(super) use window::set_window_fullscreen;

pub fn execute(action: Action, world: &mut World) {
    match action {
        Action::Spawn { cmd, args } => spawn::run(&cmd, &args),
        Action::Close => window::close_focused(world),
        Action::FocusDirection(dir) => focus::focus_direction(world, dir),
        Action::SwapDirection(dir) => window::swap_direction(world, dir),
        Action::ToggleFloat => window::toggle_float(world),
        Action::ToggleFullscreen => window::toggle_fullscreen(world),
        Action::Layout { key, args } => layout::apply_layout(world, &key, &args),
        Action::EnterMode(name) => mode::enter_mode(world, name),
        Action::ExitMode => mode::enter_mode(world, DEFAULT_MODE.into()),
        Action::Bind { spec, action, mode } => mode::bind(world, spec, *action, mode),
        Action::Unbind { spec, mode } => mode::unbind(world, &spec, &mode),
        Action::FocusTag(n) => tag::focus_tag(world, n),
        Action::ToggleTag(n) => tag::toggle_tag(world, n),
        Action::MoveToTag(n) => tag::move_to_tag(world, n),
        Action::ToggleWindowTag(n) => tag::toggle_window_tag(world, n),
        Action::FocusOutput(target) => output::focus_output(world, &target),
        Action::SendToOutput(target) => output::send_to_output(world, &target),
        Action::SetOutputFocusWarp(value) => output::set_output_focus_warp(world, value),
    }
}
