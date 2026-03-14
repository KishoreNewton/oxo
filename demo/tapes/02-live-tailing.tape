# 02 — Live Tailing with Loki
#
# Connects to a real Grafana Loki instance running in Docker.
# Shows real-time log streaming from multiple microservices,
# the sparkline throughput meter, and tail mode behavior.
#
# Prerequisites: docker compose up -d (handled by record-all.sh)

Output demo/output/02-live-tailing.gif
Output demo/output/02-live-tailing.webm

Require oxo
Require docker

Set Shell "bash"
Set FontSize 15
Set Width 1400
Set Height 750
Set Padding 15
Set Framerate 30
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Set WindowBarSize 40
Set CursorBlink false
Set TypingSpeed 60ms

# ── Show running infrastructure ────────────────────────────────────
Type "# Loki is running in Docker with 8 microservices pushing logs"
Enter
Sleep 600ms
Type "docker compose -f demo/docker-compose.yml ps"
Enter
Sleep 2s

# ── Connect to Loki ────────────────────────────────────────────────
Type "oxo --backend loki --url http://localhost:3100"
Enter

# Wait for connection + first logs
Sleep 5s

# ── Real-time streaming ────────────────────────────────────────────
# Logs pour in from api-gateway, auth-service, payment-service, etc.
Sleep 4s

# ── Scroll up to inspect older entries ─────────────────────────────
Set TypingSpeed 200ms
Type "kkkkkkkkkk"
Sleep 2s

# ── Notice: auto-scroll paused (status bar shows "paused") ────────
Sleep 1s

# ── Jump back to bottom to resume tail mode ────────────────────────
Type "G"
Sleep 1s

# Watch the live tail for a few more seconds
Sleep 4s

# ── Toggle timestamps to see when each line arrived ────────────────
Type "t"
Sleep 2s

# Toggle back
Type "t"
Sleep 1s

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
