//! Help text for gharialctl. Kept out of `main` so the dispatcher stays
//! short and the help block can grow without crowding the control flow.

pub fn usage(long: bool) {
    println!("gharialctl {} - control the gharial window manager", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage: gharialctl [-s SOCKET] <command> [args...]");
    if !long {
        println!("Try `gharialctl --help` for the full command list.");
        return;
    }
    println!();
    println!("Layout parameters:");
    println!("  set <key> <value>          Set or adjust a parameter (see KEYS)");
    println!("  get <key>                  Print the current value");
    println!("  status                     Print all parameters as key=value pairs");
    println!();
    println!("Shorthands (equivalent to `set <key> <value>`):");
    println!("  main-ratio <value>");
    println!("  main-count <value>");
    println!("  gaps <value>");
    println!("  outer-padding <value>");
    println!("  orientation <left|right|top|bottom>");
    println!("  smart-gaps <on|off|toggle>");
    println!();
    println!("Window management:");
    println!("  spawn <cmd> [args...]      Launch a program, detached");
    println!("  close                      Close the focused window");
    println!("  toggle-float               Toggle the focused window's float state");
    println!("  focus <direction>          Shift keyboard focus to a neighbour");
    println!("  swap <direction>           Swap the focused window with the neighbour");
    println!("                             <direction> = next|prev|left|right|up|down");
    println!("                             (next/prev cycle stack order; left/right/up/down");
    println!("                              pick the spatially closest tiled neighbour)");
    println!();
    println!("Tags (1..32):");
    println!("  tag focus <N>              Show only tag N");
    println!("  tag toggle <N>             Add/remove tag N from the active set");
    println!("  tag move <N>               Send focused window to tag N");
    println!("  tag window-toggle <N>      Add/remove tag N from focused window");
    println!();
    println!("Bindings and modes:");
    println!("  bind [--mode MODE] <chord> <action ...>");
    println!("                             Install a keyboard binding. Chord is");
    println!("                             '+'-separated, case-insensitive, e.g.");
    println!("                             Super+Shift+Q. Action is any of the");
    println!("                             gharialctl verbs above (close, focus,");
    println!("                             spawn, tag focus, main-ratio, ...).");
    println!("  unbind [--mode MODE] <chord>");
    println!("                             Remove a binding by chord.");
    println!("  mode <name>                Enter a named binding mode");
    println!("  mode exit                  Return to the default mode");
    println!();
    println!("Misc:");
    println!("  ping                       Verify the daemon is reachable");
    println!("  version                    Print the daemon's version");
    println!("  wait [TIMEOUT]             Block until the daemon answers ping");
    println!("                             (TIMEOUT defaults to 2000ms; suffixes: ms, s)");
    println!();
    println!("VALUES");
    println!("  Numeric values accept absolute (`0.55`, `8`), relative-add (`+0.05`,");
    println!("  `+1`) or relative-subtract (`-0.05`, `-1`) forms. Booleans accept");
    println!("  on|off|true|false|yes|no|toggle.");
    println!();
    println!("SOCKET");
    println!("  Defaults to $GHARIAL_SOCKET, then");
    println!("  $XDG_RUNTIME_DIR/gharial-$WAYLAND_DISPLAY.sock.");
}
