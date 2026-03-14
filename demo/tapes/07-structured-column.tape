# 07 — Structured Logs & Column Mode
#
# JSON logs are automatically detected and parsed. Column mode renders
# them as a sortable table. The detail panel lets you inspect every
# field. Multi-line entries expand inline.

Output demo/output/07-structured-column.gif
Output demo/output/07-structured-column.webm

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

# ── Toggle column/table mode ───────────────────────────────────────
# Press c to switch to structured column view
Type "c"
Sleep 3s

# Scroll through the table
Set TypingSpeed 200ms
Type "jjjjjjjjjj"
Sleep 1s

# ── Inspect a log entry ────────────────────────────────────────────
# Press Enter to open the detail panel
Enter
Sleep 3s

# Scroll through detail fields
Type "jjjjj"
Sleep 1s

# Expand nested content
Type "x"
Sleep 1500ms

# Close detail
Escape
Sleep 1s

# ── Toggle timestamps ──────────────────────────────────────────────
# Press t to show/hide timestamps
Type "t"
Sleep 2s
Type "t"
Sleep 800ms

# ── Toggle line wrap ───────────────────────────────────────────────
# Press w to wrap long lines instead of truncating
Type "w"
Sleep 2s
Type "w"
Sleep 800ms

# ── Back to normal log view ────────────────────────────────────────
Type "c"
Sleep 2s

# ── Copy a line to clipboard ──────────────────────────────────────
# Select a line and press y to copy
Set TypingSpeed 200ms
Type "jjjjj"
Sleep 500ms
Type "y"
Sleep 1500ms

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
