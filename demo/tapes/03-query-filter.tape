# 03 — Query & Filter Power
#
# Demonstrates LogQL query editing, the label filter side panel,
# and real-time filtering. Shows how oxo lets you drill into logs
# as fast as you can type.

Output demo/output/03-query-filter.gif
Output demo/output/03-query-filter.webm

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
Type "oxo"
Enter
Sleep 4s

# ── Enter query mode ───────────────────────────────────────────────
# Press : to open the query bar (like vim command mode)
Type ":"
Sleep 800ms

# Type a LogQL query filtering for errors in the API gateway
Set TypingSpeed 40ms
Type '{service="api-gateway"} |= "error"'
Sleep 500ms

# Submit the query
Enter
Sleep 3s

# ── Open the filter panel ──────────────────────────────────────────
# Press f to toggle the label filter sidebar
Type "f"
Sleep 2s

# Navigate down through available label filters
Set TypingSpeed 250ms
Type "jjj"
Sleep 1s

# Select a filter value
Enter
Sleep 2s

# Navigate to another filter
Type "jj"
Sleep 500ms
Enter
Sleep 2s

# ── Close filter panel ─────────────────────────────────────────────
Escape
Sleep 1s

# ── New query: filter by namespace and parse JSON ──────────────────
Type ":"
Sleep 500ms

Set TypingSpeed 40ms
Type '{namespace="prod"} | json | level="error"'
Sleep 500ms
Enter
Sleep 3s

# ── One more query: pattern matching ───────────────────────────────
Type ":"
Sleep 500ms
Type '{service=~"payment.*|order.*"} |~ "timeout|refused"'
Sleep 500ms
Enter
Sleep 3s

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
