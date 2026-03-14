#!/usr/bin/env bash
# Record all oxo demo tapes using VHS.
#
# Usage:
#   ./demo/record-all.sh              # record all tapes
#   ./demo/record-all.sh 01 03 07     # record specific tapes
#   ./demo/record-all.sh --loki       # include the Loki/Docker tape (02)
#
# Prerequisites:
#   - vhs (charmbracelet.io/vhs)
#   - docker & docker compose (only for tape 02)
#   - The oxo binary must be built (this script builds it)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TAPE_DIR="$SCRIPT_DIR/tapes"
OUTPUT_DIR="$SCRIPT_DIR/output"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${CYAN}▸${NC} $*"; }
ok()    { echo -e "${GREEN}✓${NC} $*"; }
warn()  { echo -e "${YELLOW}⚠${NC} $*"; }
fail()  { echo -e "${RED}✗${NC} $*"; exit 1; }

# ── Parse arguments ─────────────────────────────────────────────────
INCLUDE_LOKI=false
SPECIFIC_TAPES=()

for arg in "$@"; do
  case "$arg" in
    --loki) INCLUDE_LOKI=true ;;
    --help|-h)
      echo "Usage: $0 [--loki] [TAPE_NUMBERS...]"
      echo ""
      echo "Options:"
      echo "  --loki       Include tape 02 (requires Docker + Loki)"
      echo "  NUMBERS      Record only specific tapes (e.g., 01 03 07)"
      echo ""
      echo "Examples:"
      echo "  $0                 # Record tapes 01, 03-10 (skip Loki)"
      echo "  $0 --loki          # Record all tapes including Loki"
      echo "  $0 01 03 07        # Record only tapes 1, 3, 7"
      exit 0
      ;;
    [0-9][0-9]) SPECIFIC_TAPES+=("$arg") ;;
    *) warn "Unknown argument: $arg" ;;
  esac
done

# ── Check dependencies ──────────────────────────────────────────────
command -v vhs >/dev/null 2>&1 || fail "vhs is not installed. See: https://github.com/charmbracelet/vhs"

if $INCLUDE_LOKI || [[ " ${SPECIFIC_TAPES[*]:-} " == *" 02 "* ]]; then
  command -v docker >/dev/null 2>&1 || fail "docker is required for tape 02 (Loki demo)"
fi

# ── Build oxo ───────────────────────────────────────────────────────
info "Building oxo (release)..."
cd "$PROJECT_ROOT"
cargo build --release 2>&1 | tail -1
ok "Binary built: target/release/oxo"

# Add to PATH so VHS tapes can find it
export PATH="$PROJECT_ROOT/target/release:$PATH"

# Verify the binary works
oxo --help > /dev/null 2>&1 || fail "oxo binary not working"
ok "oxo binary verified"

# ── Create output directory ─────────────────────────────────────────
mkdir -p "$OUTPUT_DIR"

# ── Determine which tapes to record ─────────────────────────────────
should_record() {
  local num="$1"

  # If specific tapes were requested, only record those
  if [[ ${#SPECIFIC_TAPES[@]} -gt 0 ]]; then
    [[ " ${SPECIFIC_TAPES[*]} " == *" $num "* ]]
    return
  fi

  # Skip tape 02 (Loki) unless --loki was passed
  if [[ "$num" == "02" ]] && ! $INCLUDE_LOKI; then
    return 1
  fi

  return 0
}

# ── Docker lifecycle for tape 02 ────────────────────────────────────
DOCKER_STARTED=false

start_docker() {
  info "Starting Loki + log generator via Docker Compose..."
  cd "$SCRIPT_DIR"
  docker compose up -d --build 2>&1 | tail -3
  info "Waiting for Loki to be healthy..."
  local retries=0
  while ! curl -sf http://localhost:3100/ready > /dev/null 2>&1; do
    retries=$((retries + 1))
    if [[ $retries -ge 30 ]]; then
      fail "Loki did not become healthy after 30s"
    fi
    sleep 1
  done
  ok "Loki is ready"
  # Let the log generator push some initial data
  sleep 5
  ok "Log generator seeded initial data"
  DOCKER_STARTED=true
  cd "$PROJECT_ROOT"
}

stop_docker() {
  if $DOCKER_STARTED; then
    info "Stopping Docker Compose stack..."
    cd "$SCRIPT_DIR"
    docker compose down 2>&1 | tail -1
    ok "Docker stack stopped"
    cd "$PROJECT_ROOT"
  fi
}
trap stop_docker EXIT

# ── Record tapes ────────────────────────────────────────────────────
TOTAL=0
SUCCESS=0
FAILED=0

echo ""
echo -e "${BOLD}╔══════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║       oxo Demo Recording Suite           ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════╝${NC}"
echo ""

for tape in "$TAPE_DIR"/*.tape; do
  filename="$(basename "$tape")"
  num="${filename:0:2}"

  if ! should_record "$num"; then
    warn "Skipping $filename"
    continue
  fi

  # Start Docker if needed for tape 02
  if [[ "$num" == "02" ]] && ! $DOCKER_STARTED; then
    start_docker
  fi

  TOTAL=$((TOTAL + 1))
  info "Recording ${BOLD}$filename${NC}..."

  if vhs "$tape" 2>&1; then
    ok "Recorded $filename"
    SUCCESS=$((SUCCESS + 1))
  else
    warn "Failed to record $filename"
    FAILED=$((FAILED + 1))
  fi

  echo ""
done

# ── Summary ─────────────────────────────────────────────────────────
echo -e "${BOLD}═══════════════════════════════════════════${NC}"
echo -e "  Recorded: ${GREEN}$SUCCESS${NC}/$TOTAL"
if [[ $FAILED -gt 0 ]]; then
  echo -e "  Failed:   ${RED}$FAILED${NC}"
fi
echo ""

if [[ -d "$OUTPUT_DIR" ]]; then
  info "Output files:"
  ls -lh "$OUTPUT_DIR"/*.gif 2>/dev/null | awk '{printf "  %-40s %s\n", $NF, $5}'
  echo ""
  total_size=$(du -sh "$OUTPUT_DIR" 2>/dev/null | cut -f1)
  info "Total size: $total_size"
fi
