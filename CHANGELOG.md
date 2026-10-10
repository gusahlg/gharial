# Changelog

All notable changes to gharial. Versions follow semantic versioning;
0.x means the wire/IPC grammar may still evolve.

## [0.4.0] — scripted placement and reliability

### Added

- **Window rules** (`rule add|list|del|clear`): a matcher (`app-id=GLOB`,
  `title=GLOB`, `pid=PID`) plus a placement, applied as a matching window
  first appears — before its first configure, so the client's first
  frame already has its final size and place. Later rules override
  earlier ones; `oneshot` rules apply once; `--until-exit PID` (or a
  `pid=` matcher) removes the rule when that process exits, so a crashed
  test run leaves nothing behind. `pid=` matches the window's process,
  any descendant, and anything in the session it leads.
- **Placement vocabulary**, shared by rules, `spawn`, and `window`:
  `output`, `tags`, `float`/`tile`, `fullscreen`, `sticky`, `lock`,
  `border`/`noborder`, `focus`/`nofocus`, `size WxH`, `position X,Y`,
  `center`, `geometry WxH+X+Y`. Floating positions are relative to the
  output's usable area; negative offsets count from the far edge.
- **Sticky windows**: visible on every tag of their output.
- **Locked windows**: once sized and placed, layout changes never resize
  or move them (and never send them another configure for bounds) — for
  benchmarks whose swapchain must not be recreated mid-run.
- **`spawn` with placement**: `spawn --output DP-2 --tags 9 --float --
  game` places the first window the process tree opens, and prints the
  child's pid.
- **Window commands**: `window SEL close` and `window SEL <placement>`
  for any window, selected by `focused`, `all`, id, or matcher.
  `window SEL focus` reveals a window on a hidden tag first.
- **`window list`**: one tab-separated line per window (id, app_id,
  title, pid, output, tags, visible, floating, fullscreen, sticky,
  locked, focused, and global content geometry ready for `grim -g`), or
  `--json`; `--header` adds column names.
- **`wait window SEL [--timeout] [--new]`**: blocks until a matching
  window is placed and drawn at its final size, then prints its id.
  `--new` ignores windows that were already mapped when the wait began.
- **`output view TARGET TAGS [until-exit PID]`**: show tags on a screen
  without moving focus; with `until-exit`, the previous view returns when
  the process exits (unless someone changed it meanwhile).
- **`output presentation TARGET vsync|async|auto [until-exit PID]`**:
  tearing-allowed presentation for benchmarks; `auto` tears only for a
  fullscreen window that asks to.
- Dialogs (windows with a parent) open floating and centred on their
  parent's screen; fixed-size windows (min == max) open floating.
- Hotplug memory: windows on a disconnected output move to the focused
  one and go back — with their tags — when it reconnects; the output's
  view and presentation mode come back too.
- `toggle-sticky` action; `Client` methods for all of the above; typed
  `WindowInfo`/`OutputInfo` records in `gharial_ipc::info`.
- `scripts/e2e.sh` runs gharial in a headless nested river with a
  deterministic test client (`examples/testclient.rs`) and checks
  tiling, focus, rules, placement, waits, leases, and hotplug.

### Changed

- **Commands answer when applied, not when queued.** An `ok` from a
  state-changing command means the compositor has the new state; errors
  (unknown output, no matching window, unbound mode) are reported to the
  caller instead of being logged and dropped.
- **Directional focus rewritten.** `focus left/right/up/down` picks the
  window immediately adjacent in that direction (nearest column first,
  then most recently focused, then overlap), across screens. Previously
  a better-aligned window on another screen could beat the adjacent one,
  and ties were broken by hash-map order. With nothing in that
  direction, focus moves to the screen there, even if it is empty.
- `swap left/right/up/down` works across screens (the windows trade
  outputs and tags) and moves the window to the next screen at an edge;
  `swap next/prev` stays on the focused screen.
- Every client connection is served on its own thread, so a blocking
  `wait window` never delays other commands; requests and replies are
  length-bounded and time out instead of hanging.
- Single-owner state: settings live on the wayland thread (no mutex, no
  dirty-flag ping, no output mirror). The request grammar is one typed
  `Command` in `gharial-ipc`, shared by the daemon, `gharialctl`, and
  `Client`.
- Bindings are kept per (mode, chord) and created per seat on demand: a
  `bind` that arrives before river announces a seat is no longer lost.
- New windows take focus only when they open visibly on the focused
  screen (or a rule says `focus`); a window placed elsewhere no longer
  steals focus.
- Toggling a window to floating keeps its size and centres it; floating
  windows are always positioned explicitly. Render order is explicit
  (tiled < floating < fullscreen, most recently raised on top).
- The layer-shell default output follows the focused screen, so
  launchers open where you are working.
- Focus memory only restores windows still on that screen; keyboard
  focus is re-sent when a launcher releases it.
- `output list` and listings are tab-separated lines (was one
  semicolon-joined line). `gharialctl` exits 3 when the daemon is
  unreachable (1 = refused, 2 = usage).
- Response bodies escape backslashes, so text containing a literal `\n`
  round-trips exactly.
- A client's own fullscreen request with an output hint moves the window
  to that output's visible tags (it used to keep its old tags and could
  vanish); a `lock`ed window ignores the hint and stays where its rule
  put it.
- Non-exclusive layer-shell focus (on-demand panels) no longer blocks
  click-to-focus; only exclusive focus (launchers) does.
- A window that re-names itself before its first frame is re-placed by
  the rules — keeping the placement of a `spawn` it came from.
- Border width, gaps, and padding are clamped to sane ranges.
- MSRV is 1.82.

### Fixed

- A failed `spawn` (missing binary) while another spawned child was alive
  could abort the daemon: the global `waitpid(-1)` reaper raced std's own
  wait after a failed exec. Children are now reaped individually.
- A non-ASCII border colour or a border width above `i32::MAX` crashed
  the daemon (panic / protocol error).
- An `output view … until-exit` that was active when its output was
  unplugged stuck after re-plug; windows displaced twice lost their
  original screen.
- `bind CHORD set KEY VALUE` was rejected though documented.

### Removed

- `Action::Bind`/`Action::Unbind` (never bindable); binding requests are
  `Command::Bind`/`Command::Unbind`.

## [0.3.0] — multiple screens

### Added

- **Multi-output support**: every screen is an independent view into
  the tag space, exactly like the single screen behaved before — each
  output carries its own active tag mask and its own per-tag focus
  memory, and runs its own master-stack layout. Windows belong to one
  output; new windows land on the *focused* output. One output is
  focused at a time: tag commands apply there, keyboard focus is
  restored there, and switching warps the pointer to that screen by
  default unless it's already on it.
  - `output focus <next|prev|left|right|up|down|NAME>` switches the
    focused screen (`next`/`prev` cycle advertisement order; the
    cardinals pick the spatially nearest screen; `NAME` is a connector
    name like `DP-1` or a 1-based index). `output focus-warp off`
    disables its automatic pointer warp; the setting also accepts `on`
    and `toggle`, and defaults to enabled.
  - `output send <TARGET>` moves the focused window to another screen;
    the window adopts that screen's currently visible tags.
  - `output list` describes outputs (name, geometry, tags, and focused
    marker).
  - Spatial `focus`/`swap` (`left`/`right`/`up`/`down`) work across
    screen boundaries; output focus follows the window.
  - Click-to-focus: interacting with any window (`window_interaction`)
    focuses it, which also moves output focus to its screen.
  - Connector names come from binding the `wl_output` global river
    points us at (`wl_output.name`, v4); outputs are also always
    addressable by 1-based advertisement index.

- All of the above is exposed through `gharialctl output …`, bindable
  actions (`bind Super+comma output focus prev`), typed `Client`
  methods (`focus_output`, `set_output_focus_warp`, `send_to_output`,
  `list_outputs`), and the compile-time config builder
  (`Config::warp_pointer_on_output_focus`, plus the `OutputTarget`
  vocabulary in `gharial_ipc::output`).

### Changed

- Physical output layout and pointer adjacency are intentionally left to
  river output managers such as kanshi. Gharial no longer adds pointer
  portals on top of the compositor's configured output geometry.
- Fullscreen now covers the window's *own* output instead of always
  the first advertised one.

## [0.2.2]

### Added

- **Compile-time-checked Rust config**: the whole control vocabulary
  (`Action`, `Color`, `Orientation`, `BoolValue`, the keysym table, and
  the `Client` handle) now lives in the dependency-light `gharial-ipc`
  crate, so a config binary can speak it without pulling in the Wayland
  stack — the `gharial` crate re-exports it unchanged. A new
  `gharial_ipc::config` module adds `Layout` / `Bindings` / `Config`
  builders and three macros that turn runtime foot-guns into compile
  errors: `ratio!` (rejects `main-ratio` outside `0.05..=0.95`), `tag!`
  (rejects tags outside `1..=32`), and `chord!` (rejects an unknown
  modifier/key or empty chord). The keysym/modifier lookups and the
  chord parser are now `const`-evaluable, which is what backs `chord!`.
  See `crates/gharial-ipc/examples/typed_config.rs`.

- **Fullscreen**: `toggle-fullscreen` (alias `fullscreen`) makes the
  focused window cover its output and drops it out of the tiling layout;
  toggling again restores it. Client-driven fullscreen requests
  (`fullscreen_requested` / `exit_fullscreen_requested`) are now honoured
  too, so apps that ask to go fullscreen on their own work. Exposed via
  gharialctl, the `Client` API (`toggle_fullscreen`), and
  `Action::ToggleFullscreen`.

### Changed

- **Render hot path**: the manage/render flushes now borrow the layout
  target cache in place and walk the window set through a disjoint
  order/entry borrow, eliminating the per-cycle `HashMap` and `Vec`
  clones. The cache is refreshed once per cycle via `ensure_targets`;
  only the infrequent spatial focus/swap paths take an owned snapshot.

## [0.2.0] — first usable WM release

The protocol pivot. v0.1 was a layout daemon for river-classic
(`river-layout-v3`); v0.2 is a full external window manager for
river 0.4+ (`river-window-management-v1`). Same daemon, same control
tool, completely different protocol surface underneath.

### Added

- **Window management**: master-stack tiling, focus, swap, close,
  spawn, toggle-float, fullscreen state tracking. New windows
  auto-focus; closing or hiding never leaves an empty focus.
- **Tags 1..32** with `tag focus|toggle|move|window-toggle`. Active
  mask is preserved across mode switches; toggling to 0 falls back
  rather than blanking the screen.
- **Keyboard bindings** via `river-xkb-bindings-v1`, with a hand-rolled
  ~200-entry xkbcommon keysym table covering ASCII, named keys, F1-F20,
  numpad, and the XF86 media/brightness/wireless block (plus
  `0xRRRRRRRR` hex fallback). No xkbcommon C dep.
- **Binding modes** with `gharialctl mode <name>` / `mode exit`.
  Switching modes atomically disables old-mode bindings and enables
  new-mode bindings via a single manage-sequence flush.
- **Layer shell** (`river-layer-shell-v1`): waybar, tofi, mako, and
  other panels/launchers now work. Layer-shell focus is tracked
  per seat so launchers actually receive keystrokes.
- **Non-exclusive area tracking**: tiles avoid waybar's exclusive
  zone instead of rendering underneath it.
- **Borders**: focus-aware, per-window, single-colour outlines. Each
  window owns its full border within its slot — neighbours touch but
  never share pixels. `border-width` / `border-color-focused` /
  `border-color-unfocused` configurable at runtime via gharialctl.
  Hex colour parser premultiplies alpha per protocol.
- **Server-side decorations**: every managed window receives `use_ssd`
  + `set_tiled(all_edges)` so client-side titlebars and chrome are
  suppressed on apps that honour the hint.
- **NixOS module** (`config/init` + `/etc/nixos/modules/river.nix`
  example) — pure `gharialctl`, no `riverctl` left in the user config.

### Changed

- **Protocol target**: `river-layout-v3` → `river-window-management-v1`
  (+ `river-xkb-bindings-v1` + `river-layer-shell-v1`). XMLs pinned
  to upstream `riverwm/river` commit `da8cf20fcb2c993c1c048ced4020c58d6208ef26`.
- **Spawn semantics**: `pre_exec` resets `SIGCHLD`/`SIGPIPE` to
  `SIG_DFL` and calls `setsid()` in the child, so apps that
  fork helpers (waybar, qutebrowser, tofi) and shell pipelines
  (screenshot bindings) work correctly.
- **State machine**: `Sequence::Phase` enum + per-manage generation
  counter is the only thing that flips wm protocol phase; dispatch
  impls go through it to make the manage/render rule provable.

### Fixed

- `event_created_child` panic on the first `output` event from river —
  the `RiverWindowManagerV1` dispatch impl was missing the
  `event_created_child!` specialization for the `window`/`output`/`seat`
  opcodes, killing gharial within milliseconds of startup.

### Deferred to v0.3

- Pointer bindings (`move-view` / `resize-view`): needs the full
  `op_*` interactive-pointer state machine.
- Per-tag overrides for layout params.
- Multi-seat (currently first seat wins).
- Decoration surfaces beyond a single coloured border.

## [0.1.0]

Initial layout-only release against river-classic 0.3.x. Master-stack
tiling, IPC, gharialctl. Superseded by 0.2.0; protocol incompatible.
