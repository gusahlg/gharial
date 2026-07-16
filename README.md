# gharial

A minimal external window manager for the [river] Wayland compositor.
Owns layout, focus, tags, keybindings, and decorations end-to-end; the
compositor draws and routes input, gharial decides everything else.

| binary       | what it does                                                                                                          |
| ------------ | --------------------------------------------------------------------------------------------------------------------- |
| `gharial`    | The WM daemon. Talks to river over `river-window-management-v1`, `river-xkb-bindings-v1`, and `river-layer-shell-v1`. |
| `gharialctl` | One-shot CLI. The only configuration surface.                                                                         |

The user-facing surface is your config script (a plain `sh` file) and
`gharialctl`. River's `riverctl` plays no part in a normal gharial
setup — gharial owns the policy layer entirely.

[river]: https://github.com/riverwm/river

## Status

**0.3.0** — multi-screen release. Targets river 0.4+ (the
non-monolithic architecture). Tested against upstream `riverwm/river`
at commit `da8cf20`, which is the rev the vendored protocol XMLs are
pinned to. The older river-classic 0.3.x is **not** supported (it uses
the obsolete `river-layout-v3` protocol).

What 0.3.0 ships:

- master-stack tiling, computed per output
- **multiple screens**: every output is an independent view into the
  tag space (own active tags, own focus memory). One output is
  *focused* — new windows land there, tag commands apply there.
  `output focus <dir|name>` switches screens (keyboard focus follows;
  the pointer follows too by default), `output send <dir|name>` moves
  the focused window. `output focus-warp off` disables only that
  automatic warp
- keyboard bindings via xkb chords, named modes, per-mode enable/disable
- tags 1..32 with `focus` / `toggle` / `move` / `window-toggle`,
  per screen
- focus / swap / close / toggle-float / toggle-fullscreen / spawn;
  spatial focus/swap crosses screen boundaries
- click-to-focus: interacting with a window focuses it (and its screen)
- layer shell — waybar, tofi, mako, panels and launchers all work
- non-exclusive area tracking — tiles stop short of waybar's reserved zone
- single-colour focus-aware borders, configurable width and colours
- server-side decorations: `use_ssd` + `set_tiled` so apps drop their
  own titlebars where they can
- always-something-focused invariant: closing or hiding a window
  always lands focus on the next visible window
- spawn that actually works (`SIGCHLD` reset + `setsid()` in the child)

What's not in 0.3.0 (planned later):

- pointer bindings (interactive move/resize via the `op_*` requests)
- per-tag layout-param overrides
- multi-seat (currently first seat wins)
- decoration surfaces beyond the single coloured border
- custom layouts beyond master-stack

## Design

### Scope

gharial is the *window manager* in river's split architecture. It
receives state from river (new windows, new outputs, key bindings
firing) and tells river what to do (focus this window, propose these
dimensions, place that node here). River draws frames; gharial makes
the decisions about what should be drawn.

Output mode, scale, transform, position, and therefore the boundaries
across which the pointer can move are compositor policy. Configure them
with an output manager such as kanshi; gharial only switches its focused
output and moves windows between outputs.

It is not a desktop environment. No bar, no launcher, no notification
daemon. Use waybar / tofi / mako / etc. alongside.

### One control surface

There's exactly one configuration interface: `gharialctl`. It speaks
to the daemon over a Unix socket. Your init script runs `gharialctl
bind …`, `gharialctl set …`, `gharialctl spawn …` to wire up your
desktop.

Bindings reflect back through the same path: when you press
`Super+Q`, river fires a `pressed` event on the bound xkb-binding
object, gharial looks up the action attached to it, and dispatches.
The same action vocabulary is accepted from gharialctl (config-time)
and bindings (run-time), so anything you can bind you can also fire
directly from a shell.

### The sequence machine

`river-window-management-v1` enforces a manage/render sequence
discipline: manage-state changes (focus, propose-dimensions, enable
binding) may only happen inside a manage sequence; render-state
changes (position, show/hide, borders) inside a render sequence.
Violating that kills the WM with `sequence_order`.

gharial's `Sequence::Phase` enum (`Idle` / `Managing` / `Rendering`)
is the only thing that flips phase. Dispatch impls go through it so
the manage/render rule is provable from one place. Actions land in
`pending_actions`, drain at `manage_start`, then propose-dimensions,
borders, and set-position fire in their natural phases. The protocol's
"manage sequence is always followed by at least one render sequence"
guarantee removes most of the buffering complexity.

### Why not Zig

River itself is in Zig. gharial is a Wayland *client*; all the
communication is over the protocol wire, no FFI to river internals.
Rust's `wayland-client` is mature and pure-Rust, no system
`libwayland` required. Zig would earn its place only if we end up
patching river upstream.

## Layout

```
crates/
├── gharial-ipc/       shared IPC wire types (used by both binaries)
├── gharial/           the daemon
│   ├── protocol/      vendored river-window-management-v1
│   │                  + river-xkb-bindings-v1 + river-layer-shell-v1
│   └── src/
│       ├── main.rs            entry point
│       ├── layout.rs          pure master-stack algorithm
│       ├── wayland_proto.rs   generated bindings for all three protocols
│       ├── state/             IPC-side parameter + border store + action sender
│       ├── ipc/               Unix-socket server + per-verb handlers
│       └── wm/                the wayland-thread state machine
│           ├── sequence.rs        Phase enum + transition methods
│           ├── world.rs           the central state struct
│           ├── globals.rs         binds the three river globals + registry
│           ├── windows.rs         per-window entry (incl. output assignment)
│           ├── outputs.rs         per-output entry: tags, focus memory, name
│           ├── seats.rs           per-seat entry + layer-shell focus + pointer
│           ├── bindings.rs        xkb binding registry + mode-aware enable
│           ├── modes.rs           active-mode tracking
│           ├── tags.rs            tag bitmask + per-output visibility flush
│           ├── render.rs          layout::compute → propose/borders/set_position
│           ├── actions/           Action execution, split per concern
│           └── dispatch/          Dispatch impls split per protocol interface
└── gharialctl/        the CLI
```

## Building

```sh
cargo build --release
# binaries land in target/release/{gharial,gharialctl}
```

Runtime dependencies: just `river` 0.4+ at a matching protocol rev.
The `wayland-client` crate uses a pure-Rust backend, so no system
`libwayland` is required at runtime.

## Installing

```sh
install -Dm755 target/release/gharial   ~/.local/bin/gharial
install -Dm755 target/release/gharialctl ~/.local/bin/gharialctl
install -Dm755 config/init ~/.config/river/init
```

Then start river as usual. The example `config/init` mirrors a typical
dwm/river daily-driver. On NixOS, see the example module that ships in
the repo for a declarative install that keeps the River init in
`/etc/nixos/river/init`.

## Configuring

The portable fallback lives in `~/.config/river/init` as a plain `sh`
script. For a typed configuration, use the Rust init binary from
`config/init-rs`; it starts gharial, applies the same policy through the
compile-time-checked IPC API, and waits on the daemon.

After `gharial &`, every line of the shell fallback is a `gharialctl` call:
bindings, modes, layout params, autostart programs.

The recommended skeleton:

```sh
#!/bin/sh
gharial &
gharialctl wait

# Layout
gharialctl set gaps 8
gharialctl set main-ratio 0.55

# Borders (focused red, unfocused green, 3px)
gharialctl set border-width 3
gharialctl set border-color-focused   0xC8324BFF
gharialctl set border-color-unfocused 0x00C896FF

# Bindings
gharialctl bind Super+Q       close
gharialctl bind Super+Return  spawn rio
gharialctl bind Super+L       focus next
gharialctl bind Super+1       tag focus 1

# Multiple screens: focused screen gets new windows + keyboard.
# Output geometry and pointer adjacency are configured outside gharial.
gharialctl bind Super+Period  output focus next
gharialctl bind Super+Comma   output focus prev
gharialctl bind Super+Shift+Period output send next
# Explicit output focus moves the pointer by default. To keep it in
# place instead, uncomment this:
# gharialctl output focus-warp off
gharialctl output list

# Autostart
gharialctl spawn waybar # (if you use waybar)
wait
```

See [`config/init`](config/init) for a fully-worked example.

### Configuring in Rust

Prefer a typed config? The `gharial-ipc` crate carries the whole control
vocabulary — `Action`, `Color`, `Orientation`, the keysym table, and a
`Client` handle — with **no Wayland dependencies**, so a config binary
that depends on it alone builds in well under a second. Its `config`
module adds builders plus three macros that turn the most common
foot-guns into *compile* errors instead of runtime surprises:

```rust
use gharial_ipc::config::{Bindings, Layout};
use gharial_ipc::{chord, ratio, tag, Action, Direction};

let g = gharial_ipc::Client::new();
Layout::new()
    .main_ratio(ratio!(0.55)) // ratio!(1.5)        → compile error
    .gaps(8)
    .apply(&g)?;
Bindings::new()
    .bind(chord!("Super+Q"), Action::Close)        // chord!("Supr+Q")  → compile error
    .bind(chord!("Super+1"), tag!(1).focus())      // tag!(33)          → compile error
    .bind(chord!("Super+L"), Action::focus(Direction::Next))
    .apply(&g)?;
```

`ratio!` rejects a `main-ratio` outside `0.05..=0.95`, `tag!` rejects a
tag outside `1..=32`, and `chord!` rejects an unknown modifier/key or an
empty chord — all at compile time. Everything stays available at the low
level too: `Action` has a typed constructor for every verb and
`Client::raw` is the escape hatch. See
[`crates/gharial-ipc/examples/typed_config.rs`](crates/gharial-ipc/examples/typed_config.rs)
for a complete config, and [`config/init-rs`](config/init-rs) for a
Rust binary that also brings up the session.

The top-level `Config` builder also exposes output-focus pointer
warping. It is enabled when omitted; disable that automatic warp like
this:

```rust
use gharial_ipc::config::Config;

let config = Config::new().warp_pointer_on_output_focus(false);
```

## IPC

The daemon listens at `$GHARIAL_SOCKET`, or
`$XDG_RUNTIME_DIR/gharial-$WAYLAND_DISPLAY.sock`, mode `0600`. One
line of UTF-8 text in, one line back:

```
<command> <arg>...\n          -> ok [<body>]\n
                              -> err <message>\n
```

Arguments containing whitespace can be double-quoted with `\\` and
`\"` escapes. `gharialctl` handles quoting for you.

Run `gharialctl --help` for the full verb list.

## Protocol pin

The three vendored XML files in `crates/gharial/protocol/` are pinned
to upstream river commit `da8cf20fcb2c993c1c048ced4020c58d6208ef26`.
See `crates/gharial/protocol/README.md` for upgrade instructions.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
