# 08 — Alerting & Health Monitoring
#
# Built-in alert engine matches patterns in real-time — no external
# alert manager needed. The health dashboard monitors backend
# connectivity and throughput metrics.

Output demo/output/08-alerting.gif
Output demo/output/08-alerting.webm

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
Set TypingSpeed 50ms

# ── Launch ──────────────────────────────────────────────────────────
Type "# oxo has a built-in alert engine — no Prometheus/Alertmanager needed"
Enter
Sleep 600ms
Type "oxo"
Enter
Sleep 5s

# ── Alert panel ────────────────────────────────────────────────────
# Press a to view alert history — alerts fire automatically on patterns
Type "a"
Sleep 4s

# Close alert panel
Type "a"
Sleep 1s

# ── Mute alerts ────────────────────────────────────────────────────
# Press A to mute/unmute all alerts
Type "A"
Sleep 2s

# Unmute
Type "A"
Sleep 1s

# ── Health dashboard ───────────────────────────────────────────────
# Press H to see backend health, throughput, error rates
Type "H"
Sleep 4s

# Close health
Type "H"
Sleep 1s

# ── Live dashboard ─────────────────────────────────────────────────
# Press L for the real-time metrics dashboard
Type "L"
Sleep 4s

# Close live dashboard
Type "L"
Sleep 1s

# ── Let some time pass to see throughput ───────────────────────────
Sleep 3s

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
