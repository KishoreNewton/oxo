#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════
# record-kitty.sh — Record oxo demos using Kitty + wf-recorder
#
# Uses the real Kitty terminal (GPU-rendered) + Hyprland window
# management + wf-recorder for pixel-perfect capture. Produces
# MP4 → high-quality GIF.
#
# Usage:
#   ./demo/record-kitty.sh              # record all scenarios
#   ./demo/record-kitty.sh 01 03        # record specific scenarios
#   ./demo/record-kitty.sh --list       # list available scenarios
#
# Prerequisites: kitty, hyprctl, wf-recorder, ffmpeg, oxo in PATH
# ═══════════════════════════════════════════════════════════════════
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="$SCRIPT_DIR/output"

# ── Recording config ────────────────────────────────────────────
REC_MONITOR="HDMI-A-4"
MON_X=-1920
MON_Y=-768
MON_W=1360
MON_H=768

TERM_W=1100
TERM_H=680
FONT_SIZE=18
WINDOW_TITLE="oxo-rec"
SOCK="/tmp/oxo-rec-$$.sock"

# ── Colors ──────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'
info()  { echo -e "${CYAN}▸${NC} $*"; }
ok()    { echo -e "${GREEN}✓${NC} $*"; }
warn()  { echo -e "${YELLOW}⚠${NC} $*"; }
fail()  { echo -e "${RED}✗${NC} $*"; exit 1; }

# ── Helpers ─────────────────────────────────────────────────────
wait_ms() { sleep "$(echo "scale=3; $1/1000" | bc)"; }

send() {
  # Send text/keystrokes to the kitty window
  kitty @ --to "unix:$SOCK" send-text -- "$1" 2>/dev/null || true
}

send_key() {
  # Send special keys (Enter, Escape, Tab, etc.)
  kitty @ --to "unix:$SOCK" send-key -- "$1" 2>/dev/null || true
}

send_enter() { send_key "Return"; }
send_escape() { send_key "Escape"; }
send_tab() { send_key "Tab"; }
send_ctrl() { send_key "ctrl+$1"; }

type_slow() {
  # Type characters one at a time with delay
  local text="$1"
  local delay="${2:-35}"
  for (( i=0; i<${#text}; i++ )); do
    send "${text:$i:1}"
    wait_ms "$delay"
  done
}

cleanup() {
  pkill -f "kitty.*${WINDOW_TITLE}" 2>/dev/null || true
  pkill -f wf-recorder 2>/dev/null || true
  rm -f "$SOCK" 2>/dev/null || true
  hyprctl dispatch workspace 1 2>/dev/null || true
}
trap cleanup EXIT

# ── Recording engine ───────────────────────────────────────────
start_kitty() {
  cleanup 2>/dev/null || true
  sleep 0.5

  info "Launching Kitty ${TERM_W}x${TERM_H} font=${FONT_SIZE}"
  kitty \
    --listen-on "unix:$SOCK" \
    --override "font_size=${FONT_SIZE}" \
    --override "remember_window_size=no" \
    --override "initial_window_width=${TERM_W}" \
    --override "initial_window_height=${TERM_H}" \
    --override "confirm_os_window_close=0" \
    --override "cursor_blink_interval=0" \
    --title "${WINDOW_TITLE}" \
    --detach

  sleep 2

  # Move to recording monitor
  hyprctl dispatch "focuswindow title:${WINDOW_TITLE}" 2>/dev/null; sleep 0.3
  hyprctl dispatch "movetoworkspace 99" 2>/dev/null; sleep 0.3
  hyprctl dispatch "workspace 99" 2>/dev/null; sleep 0.3
  hyprctl dispatch "movecurrentworkspacetomonitor ${REC_MONITOR}" 2>/dev/null; sleep 0.3
  hyprctl dispatch "setfloating" 2>/dev/null; sleep 0.3
  hyprctl dispatch "resizeactive exact ${TERM_W} ${TERM_H}" 2>/dev/null; sleep 0.3

  # Center on monitor
  local cx=$(( MON_X + (MON_W - TERM_W) / 2 ))
  local cy=$(( MON_Y + (MON_H - TERM_H) / 2 ))
  hyprctl dispatch "movewindowpixel exact ${cx} ${cy},title:${WINDOW_TITLE}" 2>/dev/null
  sleep 0.5

  # Get exact geometry
  local client_json
  client_json=$(hyprctl clients -j 2>/dev/null)
  GEOMETRY=$(echo "$client_json" | python3 -c "
import sys, json
clients = json.load(sys.stdin)
for c in clients:
    if '${WINDOW_TITLE}' in (c.get('title','') + c.get('initialTitle','')):
        x, y = c['at']
        w, h = c['size']
        print(f'{x},{y} {w}x{h}')
        break
" 2>/dev/null)

  if [[ -z "$GEOMETRY" ]]; then
    fail "Could not find Kitty window geometry"
  fi
  info "Window geometry: $GEOMETRY"
}

start_recording() {
  local output_path="$1"
  local duration="$2"

  info "Recording ${duration}s → $(basename "$output_path")"
  timeout --signal=INT "$duration" \
    wf-recorder -o "$REC_MONITOR" -g "$GEOMETRY" \
    -f "$output_path" --codec libx264 -p crf=18 -p preset=medium -r 30 \
    2>/dev/null || true
  sleep 0.5
}

stop_kitty() {
  pkill -f "kitty.*${WINDOW_TITLE}" 2>/dev/null || true
  sleep 0.3
  hyprctl dispatch workspace 1 2>/dev/null || true
  sleep 0.5
}

# Convert raw MP4 → high-quality GIF using palette method
mp4_to_gif() {
  local input="$1"
  local output="$2"
  local fps="${3:-15}"
  local scale="${4:-960}"

  local palette="/tmp/oxo-palette-$$.png"
  ffmpeg -y -i "$input" \
    -vf "fps=${fps},scale=${scale}:-1:flags=lanczos,palettegen=stats_mode=diff" \
    "$palette" 2>/dev/null

  ffmpeg -y -i "$input" -i "$palette" \
    -filter_complex "fps=${fps},scale=${scale}:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=floyd_steinberg" \
    "$output" 2>/dev/null

  rm -f "$palette"
  ok "GIF: $(du -h "$output" | cut -f1) → $(basename "$output")"
}

# ═══════════════════════════════════════════════════════════════
# SCENARIOS
# ═══════════════════════════════════════════════════════════════

scenario_01_quickstart() {
  local raw="$OUTPUT_DIR/01-quickstart-raw.mp4"
  local gif="$OUTPUT_DIR/01-quickstart.gif"
  local webm="$OUTPUT_DIR/01-quickstart.webm"

  start_kitty

  # Clear and show intro comment
  send "clear"
  send_enter
  sleep 1

  # Start recording
  start_recording "$raw" 32 &
  local rec_pid=$!
  sleep 1

  # Type the oxo command
  type_slow "# oxo — k9s for your logs" 30
  send_enter; sleep 0.6
  type_slow "oxo" 60
  send_enter

  # Wait for TUI to render + logs to stream
  sleep 4

  # Open help overlay
  send "?"
  sleep 4

  # Close help
  send "?"
  sleep 1

  # Scroll through logs (vim-style)
  for i in $(seq 1 12); do send "j"; wait_ms 120; done
  sleep 0.8
  for i in $(seq 1 6); do send "k"; wait_ms 120; done
  sleep 0.8

  # Jump to bottom (tail mode)
  send "G"
  sleep 3

  # Watch live tail
  sleep 3

  # Quit
  send "q"
  sleep 1

  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM: $(du -h "$webm" | cut -f1)"
}

scenario_03_query_filter() {
  local raw="$OUTPUT_DIR/03-query-filter-raw.mp4"
  local gif="$OUTPUT_DIR/03-query-filter.gif"
  local webm="$OUTPUT_DIR/03-query-filter.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 38 &
  local rec_pid=$!
  sleep 1

  type_slow "oxo" 60; send_enter
  sleep 4

  # Enter query mode
  send ":"
  sleep 0.8
  type_slow '{service="api-gateway"} |= "error"' 30
  send_enter
  sleep 3

  # Toggle filter panel
  send "f"
  sleep 2

  # Navigate filters
  for i in $(seq 1 3); do send "j"; wait_ms 200; done
  sleep 0.5
  send_enter
  sleep 2

  # Close filter
  send_escape
  sleep 1

  # New query
  send ":"
  sleep 0.5
  type_slow '{namespace="prod"} | json | level="error"' 30
  send_enter
  sleep 3

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

scenario_04_search_navigate() {
  local raw="$OUTPUT_DIR/04-search-navigate-raw.mp4"
  local gif="$OUTPUT_DIR/04-search-navigate.gif"
  local webm="$OUTPUT_DIR/04-search-navigate.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 38 &
  local rec_pid=$!
  sleep 1

  type_slow "oxo" 60; send_enter
  sleep 5

  # Vim scrolling
  for i in $(seq 1 15); do send "j"; wait_ms 100; done
  sleep 0.5
  for i in $(seq 1 8); do send "k"; wait_ms 100; done
  sleep 0.5

  # Jump top/bottom
  send "g"; sleep 0.8
  send "G"; sleep 0.8

  # Search
  send "/"
  sleep 0.5
  type_slow "error" 40
  send_enter
  sleep 1.5

  # Next/prev match
  for i in $(seq 1 4); do send "n"; sleep 0.6; done
  send "N"; sleep 0.6
  send "N"; sleep 0.6

  # Bookmarks
  send "m"; sleep 0.8
  for i in $(seq 1 8); do send "j"; wait_ms 80; done
  send "m"; sleep 0.8
  send "'"; sleep 0.8
  send "'"; sleep 0.8

  # Context cycling
  send "C"; sleep 1.5
  send "C"; sleep 1.5

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

scenario_05_multi_source() {
  local raw="$OUTPUT_DIR/05-multi-source-tabs-raw.mp4"
  local gif="$OUTPUT_DIR/05-multi-source-tabs.gif"
  local webm="$OUTPUT_DIR/05-multi-source-tabs.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 36 &
  local rec_pid=$!
  sleep 1

  type_slow "oxo" 60; send_enter
  sleep 4

  # New tab
  send_ctrl "t"; sleep 1.5
  send ":"; sleep 0.5
  type_slow '{service="auth-service"}' 30
  send_enter; sleep 2

  # Third tab
  send_ctrl "t"; sleep 1.5
  send ":"; sleep 0.5
  type_slow '{level="error"}' 30
  send_enter; sleep 2

  # Switch tabs
  send "1"; sleep 1.2
  send "2"; sleep 1.2
  send "3"; sleep 1.2
  send "1"; sleep 1.2

  # Source picker
  send "b"; sleep 2.5
  send "j"; sleep 0.5
  send_escape; sleep 1

  # Close tab
  send "3"; sleep 0.8
  send_ctrl "w"; sleep 1.5

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

scenario_06_analytics() {
  local raw="$OUTPUT_DIR/06-analytics-stats-raw.mp4"
  local gif="$OUTPUT_DIR/06-analytics-stats.gif"
  local webm="$OUTPUT_DIR/06-analytics-stats.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 36 &
  local rec_pid=$!
  sleep 1

  type_slow "oxo" 60; send_enter
  sleep 6

  # Stats overlay
  send "s"; sleep 4
  send "s"; sleep 0.8

  # Analytics dashboard
  send "i"; sleep 5
  send "i"; sleep 0.8

  # Dedup
  send "D"; sleep 3
  send "D"; sleep 1

  # Search with context
  send "/"; sleep 0.5
  type_slow "timeout" 35
  send_enter; sleep 1.5
  send "C"; sleep 2
  send "C"; sleep 2
  send_escape; sleep 0.5

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

scenario_07_structured() {
  local raw="$OUTPUT_DIR/07-structured-column-raw.mp4"
  local gif="$OUTPUT_DIR/07-structured-column.gif"
  local webm="$OUTPUT_DIR/07-structured-column.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 34 &
  local rec_pid=$!
  sleep 1

  type_slow "oxo" 60; send_enter
  sleep 4

  # Column mode
  send "c"; sleep 3
  for i in $(seq 1 10); do send "j"; wait_ms 120; done
  sleep 1

  # Detail panel
  send_enter; sleep 3
  for i in $(seq 1 5); do send "j"; wait_ms 150; done
  sleep 1
  send "x"; sleep 1.5
  send_escape; sleep 1

  # Timestamps
  send "t"; sleep 2
  send "t"; sleep 0.8

  # Line wrap
  send "w"; sleep 2
  send "w"; sleep 0.8

  # Back to normal
  send "c"; sleep 1.5

  # Copy
  for i in $(seq 1 5); do send "j"; wait_ms 100; done
  send "y"; sleep 1.5

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

scenario_08_alerting() {
  local raw="$OUTPUT_DIR/08-alerting-raw.mp4"
  local gif="$OUTPUT_DIR/08-alerting.gif"
  local webm="$OUTPUT_DIR/08-alerting.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 34 &
  local rec_pid=$!
  sleep 1

  type_slow "# Built-in alerting — no Prometheus needed" 25
  send_enter; sleep 0.6
  type_slow "oxo" 60; send_enter
  sleep 5

  # Alert panel
  send "a"; sleep 4
  send "a"; sleep 1

  # Mute/unmute
  send "A"; sleep 2
  send "A"; sleep 1

  # Health dashboard
  send "H"; sleep 4
  send "H"; sleep 1

  # Live dashboard
  send "L"; sleep 4
  send "L"; sleep 1

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

scenario_09_advanced() {
  local raw="$OUTPUT_DIR/09-advanced-overlays-raw.mp4"
  local gif="$OUTPUT_DIR/09-advanced-overlays.gif"
  local webm="$OUTPUT_DIR/09-advanced-overlays.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 40 &
  local rec_pid=$!
  sleep 1

  type_slow "oxo" 60; send_enter
  sleep 4

  # Regex playground
  send "R"; sleep 3
  send "R"; sleep 0.8

  # Trace waterfall
  send "W"; sleep 3
  send "W"; sleep 0.8

  # Incident timeline
  send "I"; sleep 3
  send "I"; sleep 0.8

  # Live dashboard
  send "L"; sleep 3
  send "L"; sleep 0.8

  # NL query
  send_ctrl "l"; sleep 3
  send_escape; sleep 0.8

  # Saved views
  send "V"; sleep 2.5
  send_escape; sleep 0.8

  # Quick stats/analytics flash
  send "s"; sleep 1.5; send "s"; sleep 0.5
  send "i"; sleep 1.5; send "i"; sleep 0.5

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

scenario_10_export() {
  local raw="$OUTPUT_DIR/10-export-workflow-raw.mp4"
  local gif="$OUTPUT_DIR/10-export-workflow.gif"
  local webm="$OUTPUT_DIR/10-export-workflow.webm"

  start_kitty

  send "clear"; send_enter; sleep 0.5
  start_recording "$raw" 38 &
  local rec_pid=$!
  sleep 1

  type_slow "oxo" 60; send_enter
  sleep 4

  # Query
  send ":"; sleep 0.5
  type_slow '{service="api-gateway"}' 30
  send_enter; sleep 3

  # Search
  send "/"; sleep 0.5
  type_slow "500" 40
  send_enter; sleep 1.5
  send "n"; sleep 0.6
  send "n"; sleep 0.6

  # Bookmark
  send "m"; sleep 0.8
  for i in $(seq 1 8); do send "j"; wait_ms 80; done
  send "m"; sleep 0.8

  # Copy
  send "y"; sleep 1.5

  # Save query
  send "S"; sleep 2

  # Export
  send "e"; sleep 2.5

  # Saved views
  send "V"; sleep 3
  send_escape; sleep 0.8

  # Time picker
  send "T"; sleep 3
  send_escape; sleep 0.8

  # Rapid mode switching
  send "f"; sleep 1.2; send_escape; sleep 0.4
  send "s"; sleep 1.2; send "s"; sleep 0.4
  send "c"; sleep 1.2; send "c"; sleep 0.4

  send "q"; sleep 1
  wait $rec_pid 2>/dev/null || true
  stop_kitty

  mp4_to_gif "$raw" "$gif"
  ffmpeg -y -i "$raw" -c:v libvpx-vp9 -crf 30 -b:v 0 "$webm" 2>/dev/null
  ok "WebM done"
}

# ═══════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════

SCENARIOS=(
  "01:quickstart:scenario_01_quickstart"
  "03:query-filter:scenario_03_query_filter"
  "04:search-navigate:scenario_04_search_navigate"
  "05:multi-source:scenario_05_multi_source"
  "06:analytics:scenario_06_analytics"
  "07:structured:scenario_07_structured"
  "08:alerting:scenario_08_alerting"
  "09:advanced:scenario_09_advanced"
  "10:export:scenario_10_export"
)

# Check dependencies
for cmd in kitty hyprctl wf-recorder ffmpeg python3 bc; do
  command -v "$cmd" >/dev/null 2>&1 || fail "$cmd is required"
done

# Build oxo
info "Building oxo (release)..."
cd "$PROJECT_ROOT"
cargo build --release 2>&1 | tail -1
export PATH="$PROJECT_ROOT/target/release:$PATH"
oxo --help > /dev/null 2>&1 || fail "oxo binary not working"
ok "oxo binary ready"

mkdir -p "$OUTPUT_DIR"

# Parse args
if [[ "${1:-}" == "--list" ]]; then
  echo ""
  for s in "${SCENARIOS[@]}"; do
    IFS=: read -r num name _ <<< "$s"
    printf "  %s  %s\n" "$num" "$name"
  done
  echo ""
  exit 0
fi

REQUESTED=("$@")

should_record() {
  local num="$1"
  if [[ ${#REQUESTED[@]} -eq 0 ]]; then return 0; fi
  for r in "${REQUESTED[@]}"; do
    [[ "$r" == "$num" ]] && return 0
  done
  return 1
}

echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║  oxo Kitty Recording Suite                   ║${NC}"
echo -e "${BOLD}║  kitty + wf-recorder + hyprctl → GIF/WebM    ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════╝${NC}"
echo ""

TOTAL=0; SUCCESS=0

for s in "${SCENARIOS[@]}"; do
  IFS=: read -r num name func <<< "$s"
  if ! should_record "$num"; then
    warn "Skipping $num ($name)"
    continue
  fi

  TOTAL=$((TOTAL + 1))
  echo ""
  info "${BOLD}Recording $num: $name${NC}"
  echo "────────────────────────────────────────"

  if "$func"; then
    SUCCESS=$((SUCCESS + 1))
  else
    warn "Failed: $num ($name)"
  fi
done

echo ""
echo -e "${BOLD}════════════════════════════════════════════${NC}"
echo -e "  Recorded: ${GREEN}${SUCCESS}${NC}/${TOTAL}"
echo ""
if [[ -d "$OUTPUT_DIR" ]]; then
  info "Output:"
  ls -lhS "$OUTPUT_DIR"/*.gif 2>/dev/null | awk '{printf "  %-45s %s\n", $NF, $5}'
  echo ""
  info "Total: $(du -sh "$OUTPUT_DIR" 2>/dev/null | cut -f1)"
fi
