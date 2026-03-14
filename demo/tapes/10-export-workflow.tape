# 10 — Export & Workflow
#
# Complete investigation workflow: query → filter → search → bookmark
# → export. Save queries for later, export logs as JSON, and manage
# saved view configurations. Shows oxo as a complete investigation tool.

Output demo/output/10-export-workflow.gif
Output demo/output/10-export-workflow.webm

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

# ── Step 1: Query for specific logs ────────────────────────────────
Type ":"
Sleep 500ms
Set TypingSpeed 40ms
Type '{service="api-gateway"}'
Enter
Sleep 3s

# ── Step 2: Search within results ──────────────────────────────────
Type "/"
Sleep 500ms
Type "500"
Enter
Sleep 1500ms

# Jump through matches
Type "n"
Sleep 600ms
Type "n"
Sleep 600ms

# ── Step 3: Bookmark interesting entries ───────────────────────────
Type "m"
Sleep 800ms

Set TypingSpeed 200ms
Type "jjjjjjjj"
Sleep 300ms
Type "m"
Sleep 800ms

# ── Step 4: Copy a line ────────────────────────────────────────────
Type "y"
Sleep 1500ms

# ── Step 5: Save the query ─────────────────────────────────────────
# Press S to save the current query for later
Type "S"
Sleep 2s

# ── Step 6: Export logs ─────────────────────────────────────────────
# Press e to export visible logs as JSON
Type "e"
Sleep 2500ms

# ── Step 7: View saved configurations ──────────────────────────────
# Press V to browse saved views
Type "V"
Sleep 3s

Escape
Sleep 800ms

# ── Step 8: Time range picker ──────────────────────────────────────
# Press T to select a historical time range
Type "T"
Sleep 3s

Escape
Sleep 800ms

# ── Rapid workflow demo ────────────────────────────────────────────
# Show how fast you can switch between investigation modes
Type "f"
Sleep 1200ms
Escape
Sleep 400ms
Type "s"
Sleep 1200ms
Type "s"
Sleep 400ms
Type "c"
Sleep 1200ms
Type "c"
Sleep 400ms

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
