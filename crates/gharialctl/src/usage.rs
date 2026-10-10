//! Help text for gharialctl. Kept out of `main` so the dispatcher stays
//! short and the help block can grow without crowding the control flow.

const HELP: &str = "\
Layout parameters:
  set <key> <value>          Set or adjust a parameter (see KEYS)
  get <key>                  Print the current value
  status                     Print all parameters as key=value pairs
  <key> <value>              Shorthand for `set <key> <value>`
  KEYS: main-ratio main-count gaps outer-padding orientation smart-gaps
        border-width border-color-focused border-color-unfocused

Focused window:
  close                      Close it
  toggle-float               Tiled <-> floating
  toggle-fullscreen          Fullscreen on its output <-> normal
  toggle-sticky              Shown on every tag of its output <-> normal
  focus <dir>                Move focus. next|prev cycle the stack of the
                             focused screen; left|right|up|down pick the
                             window visually next to it, across screens
  swap <dir>                 Swap with that neighbour (across screens; at a
                             screen edge, move to the next screen)

Any window:
  window list [SEL] [--json] [--header]
                             One tab-separated line per window:
                             id app_id title pid output tags visible floating
                             fullscreen sticky locked focused x y width height
                             (geometry is global: grim -g \"$x,$y ${w}x$h\")
  window <SEL> close         Close the selected windows
  window <SEL> <PLACEMENT>   Change them (see PLACEMENT)
  wait window <SEL> [--timeout 10s] [--new]
                             Block until a matching window is mapped at its
                             final size; print its id. --new ignores windows
                             already mapped when the wait starts
  SEL: focused | all | ID | app-id=GLOB | title=GLOB | pid=PID (several
       matchers combine; pid matches the process, its descendants, and
       its session)

Rules (applied when a matching window first appears):
  rule add [--until-exit PID] <MATCHER...> <PLACEMENT...> [oneshot]
                             Print the new rule's id. Later rules override
                             earlier ones; oneshot rules apply once;
                             --until-exit (or a pid= matcher) removes the
                             rule when that process exits
  rule list | rule del <ID> | rule clear

Spawning:
  spawn [--PLACEMENT...] [--] <cmd> [args...]
                             Launch detached, print its pid. Placement
                             flags apply to the first window the process
                             (or any child of it) opens, e.g.
                             spawn --output DP-2 --tags 9 --float -- foot

PLACEMENT (key value | key=value | --key value | --key=value):
  output NAME|next|prev|left|right|up|down
  tags 9 | 1,3 | all         Tag membership (output view unchanged)
  float [on|off|toggle]      tile = float off
  fullscreen [on|off|toggle]
  sticky [on|off|toggle]     Visible on every tag of its output
  lock [on|off|toggle]       Never resized or moved by the layout again
  border [on|off|toggle]     noborder = border off
  focus [on|off] / nofocus   Take keyboard focus (rules: on open)
  size WxH                   Floating content size (implies float)
  position X,Y | center      Floating position in the output's usable area;
                             negative X/Y count from the right/bottom edge
  geometry WxH+X+Y           Size and position together

Tags (1..32, per output — every screen is its own view):
  tag focus <N>              Show only tag N on the focused screen
  tag toggle <N>             Add/remove tag N from the focused screen's view
  tag move <N>               Send the focused window to tag N
  tag window-toggle <N>      Add/remove tag N on the focused window

Outputs (screens):
  output focus <TARGET>      Switch the focused screen (new windows, tag
                             commands, and keyboard focus follow)
  output send <TARGET>       Move the focused window to a screen
  output view <TARGET> <TAGS> [until-exit PID]
                             Show TAGS on a screen without moving focus; with
                             until-exit, the old view returns when PID exits
  output presentation <TARGET> vsync|async|auto [until-exit PID]
                             async allows tearing (benchmarks); auto tears
                             only for a fullscreen window that asks to
  output focus-warp on|off|toggle
                             Move the pointer on `output focus` (default on)
  output list [--json] [--header]
                             name x y width height tags focused presentation
                             usable_x usable_y usable_width usable_height
  TARGET: next|prev|left|right|up|down, a connector name (DP-1), or a
          1-based index

Bindings and modes:
  bind [--mode MODE] <chord> <action...>
                             Bind a chord (Super+Shift+Q; case-insensitive)
                             to any command above that changes state
  unbind [--mode MODE] <chord>
  mode <name> | mode exit    Switch binding mode

Misc:
  ping | version
  wait [TIMEOUT]             Block until the daemon answers (default 2000ms;
                             bare numbers are ms; suffixes ms, s)

Values: numbers accept absolute (0.55, 8) and relative (+0.05, -1) forms.
State-changing commands return once the change is applied.
Exit status: 0 ok, 1 refused by the daemon, 2 usage error, 3 unreachable.
Socket: $GHARIAL_SOCKET, else $XDG_RUNTIME_DIR/gharial-$WAYLAND_DISPLAY.sock";

pub fn usage(long: bool) {
    println!(
        "gharialctl {} - control the gharial window manager\n",
        env!("CARGO_PKG_VERSION")
    );
    println!("Usage: gharialctl [-s SOCKET] <command> [args...]");
    if long {
        println!("\n{HELP}");
    } else {
        println!("Try `gharialctl --help` for the full command list.");
    }
}
