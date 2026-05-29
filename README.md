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

**0.2.0** — first usable release. Targets river 0.4+ (the
non-monolithic architecture). Tested against upstream `riverwm/river`
at commit `da8cf20`, which is the rev the vendored protocol XMLs are
pinned to. The older river-classic 0.3.x is **not** supported (it uses
the obsolete `river-layout-v3` protocol).

What 0.2.0 ships:

- master-stack tiling, per output
- keyboard bindings via xkb chords, named modes, per-mode enable/disable
- tags 1..32 with `focus` / `toggle` / `move` / `window-toggle`
- focus / swap / close / toggle-float / spawn
- layer shell — waybar, tofi, mako, panels and launchers all work
- non-exclusive area tracking — tiles stop short of waybar's reserved zone
- single-colour focus-aware borders, configurable width and colours
- server-side decorations: `use_ssd` + `set_tiled` so apps drop their
  own titlebars where they can
- always-something-focused invariant: closing or hiding a window
  always lands focus on the next visible window
- spawn that actually works (`SIGCHLD` reset + `setsid()` in the child)

What's not in 0.2.0 (planned for 0.3):

- pointer bindings (interactive move/resize via the `op_*` requests)
- per-tag layout-param overrides
- multi-seat (currently first seat wins)
- decoration surfaces beyond the single coloured border
- custom layouts beyond master-stack
- per-output window assignment

## Design

### Scope

gharial is the *window manager* in river's split architecture. It
receives state from river (new windows, new outputs, key bindings
firing) and tells river what to do (focus this window, propose these
dimensions, place that node here). River draws frames; gharial makes
the decisions about what should be drawn.

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
│       ├── action.rs          Action enum + parser (chord, tag, mode, ...)
│       ├── keysyms.rs         hand-rolled xkbcommon keysym/modifier table
│       ├── layout.rs          pure master-stack algorithm
│       ├── wayland_proto.rs   generated bindings for all three protocols
│       ├── state/             IPC-side parameter + border store + action sender
│       ├── ipc/               Unix-socket server + per-verb handlers
│       └── wm/                the wayland-thread state machine
│           ├── sequence.rs        Phase enum + transition methods
│           ├── world.rs           the central state struct
│           ├── globals.rs         binds the three river globals
│           ├── windows.rs         per-window entry
│           ├── outputs.rs         per-output entry + non-exclusive area
│           ├── seats.rs           per-seat entry + layer-shell focus
│           ├── bindings.rs        xkb binding registry + mode-aware enable
│           ├── modes.rs           active-mode tracking
│           ├── tags.rs            tag bitmask + visibility flush
│           ├── render.rs          layout::compute → propose/borders/set_position
│           ├── actions.rs         Action execution (the big match)
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
the repo for a declarative install that places `/etc/river/init`.

## Configuring

Your entire desktop lives in `~/.config/river/init`. It's a plain
`sh` script. After `gharial &`, every line is a `gharialctl` call:
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

# Autostart
gharialctl spawn waybar
wait
```

See [`config/init`](config/init) for a fully-worked example.

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
