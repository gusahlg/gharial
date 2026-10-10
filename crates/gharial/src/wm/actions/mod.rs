//! Action execution. Everything that changes window-management state
//! goes through [`execute`], called inside a manage sequence (so every
//! protocol request it makes is legal) — from key bindings and from IPC
//! alike. Each action reports an [`Outcome`]: the IPC caller gets it as
//! the reply; a key binding just logs failures.
//!
//! | sub-module | actions                                               |
//! |------------|-------------------------------------------------------|
//! | `focus`    | `FocusDirection`, plus the focus invariants           |
//! | `window`   | close, toggles, `SwapDirection`, `window SELECTOR …`  |
//! | `tag`      | `FocusTag` / `ToggleTag` / `MoveToTag` / window-tag    |
//! | `output`   | output focus/send/view/presentation, focus-warp        |
//! | `mode`     | `EnterMode` / `ExitMode`, bind/unbind                  |
//! | `layout`   | `Layout { key, args }`                                |
//! | `spawn`    | `Spawn { cmd, args, placement }`                      |

pub mod focus;
mod layout;
mod mode;
pub mod output;
mod spawn;
pub mod tag;
pub mod window;

use crate::action::Action;

use super::modes::DEFAULT_MODE;
use super::world::World;

pub use mode::{bind, unbind};

/// `Ok(reply body)` or `Err(message)`.
pub type Outcome = Result<String, String>;

pub fn execute(world: &mut World, action: Action) -> Outcome {
    match action {
        Action::Spawn {
            cmd,
            args,
            placement,
        } => spawn::spawn(world, &cmd, &args, &placement),
        Action::Close => window::close_focused(world),
        Action::FocusDirection(dir) => focus::focus_direction(world, dir),
        Action::SwapDirection(dir) => window::swap_direction(world, dir),
        Action::ToggleFloat => window::toggle(world, window::toggle_float),
        Action::ToggleFullscreen => window::toggle(world, window::toggle_fullscreen),
        Action::ToggleSticky => window::toggle(world, window::toggle_sticky),
        Action::Layout { key, args } => layout::apply_layout(world, &key, &args),
        Action::EnterMode(name) => mode::enter_mode(world, &name),
        Action::ExitMode => mode::enter_mode(world, DEFAULT_MODE),
        Action::FocusTag(n) => tag::focus_tag(world, n),
        Action::ToggleTag(n) => tag::toggle_tag(world, n),
        Action::MoveToTag(n) => tag::move_to_tag(world, n),
        Action::ToggleWindowTag(n) => tag::toggle_window_tag(world, n),
        Action::FocusOutput(target) => output::focus_output(world, &target),
        Action::SendToOutput(target) => output::send_to_output(world, &target),
        Action::SetOutputFocusWarp(value) => output::set_output_focus_warp(world, value),
        Action::ViewOutputTags {
            output,
            tags,
            until_exit,
        } => output::view_tags(world, &output, tags, until_exit),
        Action::SetPresentation {
            output,
            mode,
            until_exit,
        } => output::set_presentation(world, &output, mode, until_exit),
        Action::Window { selector, op } => window::window_command(world, &selector, &op),
    }
}
