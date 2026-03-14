# 01 — Quick Start
#
# First launch experience: oxo starts with the demo backend, logs stream
# in immediately, and the user gets a tour of the core UI layout —
# sparkline, log viewer, status bar, and the help overlay.

Output demo/output/01-quickstart.gif
Output demo/output/01-quickstart.webm

Require oxo

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
Set TypingSpeed 80ms

# ── Launch ──────────────────────────────────────────────────────────
Type "# oxo — a terminal UI for log aggregation"
Enter
Sleep 600ms
Type "# Zero config: just run it with the built-in demo backend"
Enter
Sleep 600ms
Type "oxo"
Enter

# Wait for the TUI to render and logs to start streaming
Sleep 4s

# ── Show logs streaming ────────────────────────────────────────────
# Logs are flowing in real-time — the sparkline shows throughput
Sleep 3s

# ── Open the help overlay ──────────────────────────────────────────
Type "?"
Sleep 4s

# ── Close help ─────────────────────────────────────────────────────
Type "?"
Sleep 1s

# ── Basic navigation ───────────────────────────────────────────────
# Scroll down through logs (vim-style j)
Set TypingSpeed 150ms
Type "jjjjjjjjjj"
Sleep 1s

# Scroll back up
Type "kkkkk"
Sleep 1s

# Jump to bottom (resume tail mode)
Type "G"
Sleep 2s

# Watch new logs appear in real-time
Sleep 3s

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
