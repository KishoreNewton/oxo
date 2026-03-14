# 06 — Analytics & Statistics
#
# Built-in analytics engine: log statistics, pattern clustering,
# anomaly detection, trend analysis, and smart deduplication.
# No external tools needed — the intelligence is in the TUI.

Output demo/output/06-analytics-stats.gif
Output demo/output/06-analytics-stats.webm

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

# ── Launch and accumulate logs ──────────────────────────────────────
Type "oxo"
Enter
Sleep 6s

# ── Log statistics overlay ─────────────────────────────────────────
# Press s to see real-time log stats
Type "s"
Sleep 4s

# Close stats
Type "s"
Sleep 800ms

# ── Analytics dashboard ────────────────────────────────────────────
# Press i for the full analytics view: patterns, anomalies, trends
Type "i"
Sleep 5s

# Close analytics
Type "i"
Sleep 800ms

# ── Smart deduplication ────────────────────────────────────────────
# Press D to collapse identical/similar consecutive log lines
Type "D"
Sleep 3s

# Toggle dedup off to show the difference
Type "D"
Sleep 1500ms

# Turn it back on
Type "D"
Sleep 2s

# Turn it off
Type "D"
Sleep 800ms

# ── Search with context lines ──────────────────────────────────────
# Search for something, then use C to add surrounding context
Type "/"
Sleep 500ms
Set TypingSpeed 40ms
Type "timeout"
Enter
Sleep 1500ms

# Cycle context: 0 → 3 lines of context around each match
Type "C"
Sleep 2s

# 3 → 5 lines
Type "C"
Sleep 2s

# Back to 0
Type "C"
Sleep 800ms
Type "C"
Sleep 800ms

Escape
Sleep 500ms

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
