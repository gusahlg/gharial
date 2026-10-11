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

**0.4.1** — scripted placement and reliability. Targets river 0.4+ (the
non-monolithic architecture). Tested against upstream `riverwm/river`
at commit `da8cf20` (the rev the vendored protocol XMLs are pinned to)
and river 0.4.8. The older river-classic 0.3.x is **not** supported (it
uses the obsolete `river-layout-v3` protocol).

What it does:

- master-stack tiling, computed per output
- **multiple screens**: every output is an independent view into the
  tag space (own active tags, own focus memory). One output is
  *focused* — new windows land there, tag commands apply there.
  Windows on an unplugged screen move over and return when it comes back
- **window rules**: match by app_id/title glob or process tree and decide
  output, tags, floating/tiled, fullscreen, size, position, sticky,
  locked geometry, border, and focus — before the window's first frame
- **scripting**: `window list` (tab-separated or JSON, geometry ready for
  `grim`), `window SEL <placement>`, `spawn --placement -- cmd`,
  `wait window SEL`, process-scoped `output view` / `output presentation`
- tags 1..32 with `focus` / `toggle` / `move` / `window-toggle`, per
  screen; **sticky** windows show on every tag
- directional focus/swap that goes to the window you see next to the
  focused one, across screens; click-to-focus
- dialogs and fixed-size windows float, centred on their parent's screen
- keyboard bindings via xkb chords, named modes
- layer shell (waybar, tofi, mako, …), exclusive-zone aware tiling;
  launchers open on the focused screen
- focus-aware borders, server-side decorations, explicit stacking
- every command answers once applied, with a real error when it can't be

Not yet (planned):

- pointer bindings (interactive move/resize via the `op_*` requests)
- per-tag layout-param overrides
- multi-seat (first seat wins for focus)
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
├── gharial-ipc/       the whole control vocabulary, no dependencies:
│                      wire format, Command grammar, Action, Placement,
│                      Rule, WindowSelector, listings, typed Client
├── gharial/           the daemon
│   ├── protocol/      vendored river-window-management-v1
│   │                  + river-xkb-bindings-v1 + river-layer-shell-v1
│   ├── examples/      testclient (deterministic e2e client), rust_init
│   └── src/
│       ├── main.rs            entry point
│       ├── layout.rs          pure master-stack algorithm
│       ├── settings/          layout + border parameters
│       ├── bridge.rs          IPC threads → wayland thread envelopes
│       ├── ipc/               Unix-socket server, one thread per client
│       └── wm/                the wayland-thread state machine
│           ├── world.rs           the central state struct
│           ├── commands.rs        IPC commands: answer, or queue for manage
│           ├── placement.rs       defaults + rules + window commands
│           ├── rules.rs           rule store and lifetimes
│           ├── spatial.rs         directional neighbour picking
│           ├── leases.rs          process-scoped output changes
│           ├── waits.rs           `wait window`
│           ├── ticker.rs          deadlines and process exits
│           ├── procs.rs           /proc ancestry and liveness
│           ├── windows.rs         per-window state
│           ├── outputs.rs         per-output tags, focus memory, names
│           ├── seats.rs, bindings.rs, modes.rs, tags.rs, focus.rs
│           ├── render/            manage flush + render flush + stacking
│           ├── actions/           action execution, per concern
│           └── dispatch/          one Dispatch impl per protocol interface
├── gharialctl/        the CLI
└── scripts/e2e.sh     end-to-end tests in a headless nested river
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
gharialctl bind Super+H       focus left
gharialctl bind Super+L       focus right
gharialctl bind Super+1       tag focus 1

# Rules
gharialctl rule add app-id=pavucontrol float size 900x600 center

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

## Rules and scripted control

Rules decide where a window goes the moment it appears — before its
first frame, so it never flashes in the wrong place or at the wrong
size:

```sh
gharialctl rule add app-id=pavucontrol float size 900x600 center
gharialctl rule add app-id=firefox output DP-2 tags 2
gharialctl rule add title='Picture-in-Picture' float sticky size 480x270 position -20,-20
```

Matchers: `app-id=GLOB`, `title=GLOB`, `pid=PID` (the process, its
descendants, and its session). Placement keys: `output`, `tags`,
`float`/`tile`, `fullscreen`, `sticky`, `lock`, `border`/`noborder`,
`focus`/`nofocus`, `size WxH`, `position X,Y`, `center`,
`geometry WxH+X+Y`. Positions are relative to the output's usable area;
negative offsets count from the right/bottom.

The same keys work on running windows and on `spawn`, which makes
gharial scriptable from test harnesses and benchmarks:

```sh
# Launch a benchmark on DP-2, tag 9, fullscreen, without stealing focus;
# show tag 9 there and allow tearing only while it runs.
pid=$(gharialctl spawn --output DP-2 --tags 9 --fullscreen --nofocus --lock -- ./bench)
gharialctl output view DP-2 9 until-exit "$pid"
gharialctl output presentation DP-2 async until-exit "$pid"
id=$(gharialctl wait window pid="$pid" --timeout 20s)   # placed at final size

# Exact geometry for a screenshot.
gharialctl window list "$id" | awk -F'\t' '{printf "%s,%s %sx%s\n", $13, $14, $15, $16}' | xargs -I{} grim -g {} out.png

# Any window, any time.
gharialctl window app-id=mpv sticky float size 640x360 position -10,-10
```

Rules added with `--until-exit PID` (and rules with a `pid=` matcher)
disappear when that process exits; `oneshot` rules apply to one window.

## IPC

The daemon listens at `$GHARIAL_SOCKET`, or
`$XDG_RUNTIME_DIR/gharial-$WAYLAND_DISPLAY.sock`, mode `0600`. One
line of UTF-8 text in, one line back:

```
<command> <arg>...\n          -> ok [<body>]\n
                              -> err <message>\n
```

Arguments containing whitespace can be double-quoted with `\\` and
`\"` escapes; multi-line bodies (listings) escape `\n`. `gharialctl`
handles both. State-changing commands reply once the change has been
applied; queries reply immediately. Every connection is served on its
own thread, so `wait window` never holds up other clients.

Run `gharialctl --help` for the full verb list.

## Testing

```sh
cargo test --workspace
cargo build && cargo build --example testclient && scripts/e2e.sh
```

`scripts/e2e.sh` starts a headless river with two virtual outputs,
runs gharial in it, and drives it with `gharialctl` and the
deterministic `testclient` — it never touches your running session.

## Protocol pin

The three vendored XML files in `crates/gharial/protocol/` are pinned
to upstream river commit `da8cf20fcb2c993c1c048ced4020c58d6208ef26`.
See `crates/gharial/protocol/README.md` for upgrade instructions.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
