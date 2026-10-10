#!/bin/sh
# End-to-end tests: run gharial inside a headless, nested river with two
# virtual outputs and drive it with gharialctl and the `testclient`
# example. Never touches the running desktop session.
#
#   cargo build && cargo build --example testclient && scripts/e2e.sh
#
# (`cargo test` alone does not rebuild the gharialctl binary.)
#
# Needs `river` (0.4+) on PATH; `wlr-randr` enables the hotplug test.

set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${BIN:-$ROOT/target/debug}
WORK=$(mktemp -d "${TMPDIR:-/tmp}/gharial-e2e.XXXXXX")
# Unix socket paths are limited to ~108 bytes; keep this one short.
export GHARIAL_SOCKET="${XDG_RUNTIME_DIR:-/tmp}/gharial-e2e-$$.sock"
G="$BIN/gharialctl"
TC="$BIN/examples/testclient"

PASS=0
FAIL=0
RIVER_PID=

cleanup() {
    [ -n "$RIVER_PID" ] && kill "$RIVER_PID" 2>/dev/null
    rm -f "$GHARIAL_SOCKET"
    jobs -p | xargs -r kill 2>/dev/null
    wait 2>/dev/null
    if [ "$FAIL" -eq 0 ] && [ -z "${KEEP:-}" ]; then rm -rf "$WORK"; else echo "logs kept in $WORK"; fi
}
trap cleanup EXIT INT TERM

ok() { PASS=$((PASS + 1)); echo "  ok    $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; [ $# -gt 1 ] && printf '        %s\n' "$2"; }
check() { # check NAME EXPECTED ACTUAL
    if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "expected [$2], got [$3]"; fi
}

# Field N (1-based) of window ID's `window list` line.
field() { "$G" window list "$1" | cut -f "$2"; }
geom() { "$G" window list "$1" | cut -f 13-16 | tr '\t' ' '; }

client() { # client NAME [testclient args...] — runs in the background
    name=$1
    shift
    WAYLAND_DISPLAY=$DISPLAY_NAME "$TC" --app-id "$name" --title "$name" "$@" >"$WORK/$name.log" 2>&1 &
    echo $!
}

# ── bring up river + gharial ──────────────────────────────────────────
cat >"$WORK/init" <<INIT
#!/bin/sh
echo "\$WAYLAND_DISPLAY" > "$WORK/display"
exec "$BIN/gharial" > "$WORK/gharial.log" 2>&1
INIT
chmod +x "$WORK/init"
env -u WAYLAND_DISPLAY -u DISPLAY WLR_BACKENDS=headless WLR_RENDERER=pixman \
    WLR_HEADLESS_OUTPUTS=2 WLR_LIBINPUT_NO_DEVICES=1 \
    river -no-xwayland -c "$WORK/init" >"$WORK/river.log" 2>&1 &
RIVER_PID=$!
"$G" wait 5000 || { echo "gharial did not come up"; cat "$WORK/gharial.log"; exit 1; }
DISPLAY_NAME=$(cat "$WORK/display")
"$G" set border-width 2 >/dev/null
"$G" set gaps 0 >/dev/null
"$G" set outer-padding 0 >/dev/null
"$G" set smart-gaps off >/dev/null

OUT1=$("$G" output list | sed -n 1p | cut -f1)
OUT2=$("$G" output list | sed -n 2p | cut -f1)
echo "outputs: $OUT1 $OUT2"

# ── tiling ────────────────────────────────────────────────────────────
echo "tiling"
client tile-a >/dev/null
A=$("$G" wait window app-id=tile-a --timeout 5s)
check "first window fills its output" "2 2 1276 716" "$(geom "$A")"
client tile-b >/dev/null
B=$("$G" wait window app-id=tile-b --timeout 5s)
sleep 0.3
check "second window is focused" "1" "$(field "$B" 12)"
check "main shrinks to the ratio" "2 2 700 716" "$(geom "$A")"
check "stack gets the rest" "706 2 572 716" "$(geom "$B")"

# ── directional focus ────────────────────────────────────────────────
echo "directional focus"
"$G" focus left
check "focus left reaches main" "1" "$(field "$A" 12)"
"$G" focus right
check "focus right reaches stack" "1" "$(field "$B" 12)"
"$G" focus right
check "focus right past the edge moves to the empty screen" \
    "$OUT2	1" "$("$G" output list | grep focused -m1 >/dev/null; "$G" output list | awk -F'\t' '$7==1{print $1"\t"$7}')"
"$G" focus left
check "focus left comes back to the window" "1" "$(field "$B" 12)"

# ── rules: float, size, position, nofocus ─────────────────────────────
echo "rules"
RULE=$("$G" rule add app-id=bench-float float size 640x360 position 10,20 nofocus)
check "rule add prints an id" "1" "$(echo "$RULE" | grep -c '^[0-9]\+$')"
client bench-float >/dev/null
F=$("$G" wait window app-id=bench-float --timeout 5s)
check "floating at exact size and position" "12 22 640 360" "$(geom "$F")"
check "first configure was already final" "configure 0 640 360" \
    "$(grep -m1 '^configure' "$WORK/bench-float.log" | cut -d' ' -f1-4)"
check "nofocus kept focus where it was" "1" "$(field "$B" 12)"
check "floating flag" "1" "$(field "$F" 8)"
"$G" rule del "$RULE"

# ── rules: output + tag, show it until a process exits ───────────────
echo "output, tags, view leases"
"$G" rule add app-id=bench-off output "$OUT2" tags 9 >/dev/null
client bench-off >/dev/null
O=$("$G" wait window app-id=bench-off --timeout 5s)
check "lands on the named output" "$OUT2" "$(field "$O" 5)"
check "with the named tag, hidden" "9	0" "$(field "$O" 6-7)"
sleep 1 &
HOLDER=$!
"$G" output view "$OUT2" 9 until-exit "$HOLDER"
check "view shows it" "1" "$(field "$O" 7)"
check "without moving output focus" "$OUT1" "$("$G" output list | awk -F'\t' '$7==1{print $1}')"
wait "$HOLDER"
sleep 0.6
check "view restored after the process exited" "0" "$(field "$O" 7)"

# ── spawn with placement, pid matching through a shell ───────────────
echo "spawn placement"
PID=$("$G" spawn --output "$OUT2" --fullscreen --nofocus -- sh -c "exec env WAYLAND_DISPLAY=$DISPLAY_NAME $TC --app-id spawned > $WORK/spawned.log 2>&1")
S=$("$G" wait window pid="$PID" --timeout 5s)
check "spawned window found by pid" "spawned" "$(field "$S" 2)"
check "fullscreen covers its output" "1280 0 1280 720" "$(geom "$S")"
check "fullscreen flag" "1" "$(field "$S" 9)"
check "its first configure was already fullscreen" "configure 0 1280 720 fullscreen" \
    "$(grep -m1 '^configure' "$WORK/spawned.log")"
check "window command replies with the ids" "$S" "$("$G" window "$S" fullscreen off float size 300x200 center)"
sleep 0.4
check "window command re-places it" "1770 260 300 200" "$(geom "$S")"

# ── sticky ────────────────────────────────────────────────────────────
echo "sticky"
"$G" window "$A" sticky >/dev/null
"$G" tag focus 2
check "sticky window stays visible on another tag" "1" "$(field "$A" 7)"
check "non-sticky window is hidden" "0" "$(field "$B" 7)"
"$G" tag focus 1

# ── lock ──────────────────────────────────────────────────────────────
echo "lock"
"$G" window "$A" lock sticky off >/dev/null
BEFORE=$(geom "$A")
client tile-c >/dev/null
C=$("$G" wait window app-id=tile-c --timeout 5s)
sleep 0.3
check "locked window keeps its geometry" "$BEFORE" "$(geom "$A")"
"$G" window "$A" lock off >/dev/null

# ── nofocus survives fallback focus ──────────────────────────────────
echo "nofocus"
client tmp-close >/dev/null
T=$("$G" wait window app-id=tmp-close --timeout 5s)
check "a new window on the focused screen takes focus" "1" "$(field "$T" 12)"
"$G" window "$T" close >/dev/null
sleep 0.4
check "closing it never hands focus to a nofocus window" "0" "$(field "$F" 12)"
check "focus fell back to a regular window" "1" \
    "$("$G" window list | awk -F'\t' '$12==1' | wc -l | tr -d ' ')"

# ── waits ────────────────────────────────────────────────────────────
echo "waits"
"$G" wait window app-id=never --timeout 300ms 2>"$WORK/wait.err"
check "wait times out with status 1" "1" "$?"
ID=$("$G" wait window app-id=tile-a --timeout 1s)
check "wait answers at once for a settled window" "$A" "$ID"
"$G" wait window app-id=tile-a --new --timeout 300ms 2>/dev/null
check "--new ignores already-mapped windows" "1" "$?"

# ── errors are reported, not swallowed ───────────────────────────────
echo "errors"
"$G" output focus NOPE 2>/dev/null
check "unknown output is an error" "1" "$?"
"$G" window 9999 float 2>/dev/null
check "unknown window is an error" "1" "$?"
"$G" mode typo 2>/dev/null
check "entering an unbound mode is refused" "1" "$?"
"$G" spawn sleep 3 >/dev/null
"$G" spawn gharial-no-such-command 2>/dev/null
check "a failed exec is an error (with a live child around)" "1" "$?"
"$G" set border-color-focused 0xaé23456 2>/dev/null
check "a non-ASCII colour is an error" "1" "$?"
"$G" set border-width 2147483648 >/dev/null
check "a huge border width is clamped" "100" "$("$G" get border-width)"
"$G" set border-width 2 >/dev/null
"$G" bind Super+F12 set main-ratio -0.05
check "bind accepts the set form" "0" "$?"

# ── hotplug: windows come home ───────────────────────────────────────
if command -v wlr-randr >/dev/null 2>&1; then
    echo "hotplug"
    WAYLAND_DISPLAY=$DISPLAY_NAME wlr-randr --output "$OUT2" --off
    sleep 0.5
    check "evacuated window moved to the remaining output" "$OUT1" "$(field "$O" 5)"
    WAYLAND_DISPLAY=$DISPLAY_NAME wlr-randr --output "$OUT2" --on
    sleep 0.8
    check "and returns when its output comes back" "$OUT2	9" "$(field "$O" 5-6)"
    sleep 30 &
    HOLDER=$!
    "$G" output view "$OUT2" 9 until-exit "$HOLDER"
    WAYLAND_DISPLAY=$DISPLAY_NAME wlr-randr --output "$OUT2" --off
    sleep 0.5
    WAYLAND_DISPLAY=$DISPLAY_NAME wlr-randr --output "$OUT2" --on
    sleep 0.8
    check "a temporary view doesn't outlive an unplug" "1" \
        "$("$G" output list | awk -F'\t' -v o="$OUT2" '$1==o{print $6}')"
    kill "$HOLDER"
fi

# ── health ───────────────────────────────────────────────────────────
echo "health"
"$G" ping >/dev/null
check "daemon still alive" "0" "$?"
check "no protocol errors logged" "0" "$(grep -ci 'protocol error\|error(wm)' "$WORK/gharial.log" "$WORK/river.log" | awk -F: '{s+=$NF} END{print s+0}')"

echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
