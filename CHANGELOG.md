# Changelog

All notable changes to gharial. Versions follow semantic versioning;
0.x means the wire/IPC grammar may still evolve.

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
