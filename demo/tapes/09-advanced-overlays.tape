# 09 — Advanced Features
#
# Power-user overlays: trace waterfall for distributed tracing,
# regex playground for testing patterns, incident timeline,
# diff mode, and natural language queries. Features that usually
# require separate SaaS tools — all in your terminal.

Output demo/output/09-advanced-overlays.gif
Output demo/output/09-advanced-overlays.webm

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
Sleep 4s

# ── Regex playground ───────────────────────────────────────────────
# Press R to open the regex testing overlay
Type "R"
Sleep 3s

# Close regex playground
Type "R"
Sleep 800ms

# ── Trace waterfall ────────────────────────────────────────────────
# Press W to visualize distributed traces as a waterfall chart
Type "W"
Sleep 3s

# Close trace
Type "W"
Sleep 800ms

# ── Incident timeline ──────────────────────────────────────────────
# Press I to see the incident/anomaly timeline
Type "I"
Sleep 3s

# Close incident timeline
Type "I"
Sleep 800ms

# ── Diff mode ──────────────────────────────────────────────────────
# Compare two log streams side-by-side
# Press d to toggle diff mode
Sleep 500ms

# ── Live metrics dashboard ─────────────────────────────────────────
# Press L for real-time metrics visualization
Type "L"
Sleep 3s

# Close
Type "L"
Sleep 800ms

# ── Natural language query ─────────────────────────────────────────
# Ctrl+L opens the NL query interface — type in English, get LogQL
Ctrl+l
Sleep 3s

# Close NL query
Escape
Sleep 800ms

# ── Saved views ────────────────────────────────────────────────────
# Press V to browse and load saved view configurations
Type "V"
Sleep 2500ms

Escape
Sleep 800ms

# ── Quick feature tour recap ───────────────────────────────────────
# Toggle through a few more features rapidly
Type "s"
Sleep 1500ms
Type "s"
Sleep 500ms

Type "i"
Sleep 1500ms
Type "i"
Sleep 500ms

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
