# Changelog

All notable changes to gharial. Versions follow semantic versioning;
0.x means the wire/IPC grammar may still evolve.

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
