# 05 — Multi-Source & Tabs
#
# Multiple concurrent log streams in separate tabs, each with its
# own query and scroll position. Switch between them instantly.
# The source picker lets you hot-swap backends at runtime.

Output demo/output/05-multi-source-tabs.gif
Output demo/output/05-multi-source-tabs.webm

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

# ── Open a second tab ──────────────────────────────────────────────
# Ctrl+t creates a new tab
Ctrl+t
Sleep 1500ms

# Set a different query for tab 2
Type ":"
Sleep 500ms
Set TypingSpeed 40ms
Type '{service="auth-service"}'
Enter
Sleep 2s

# ── Open a third tab ───────────────────────────────────────────────
Ctrl+t
Sleep 1500ms

# Errors-only query for tab 3
Type ":"
Sleep 500ms
Type '{level="error"}'
Enter
Sleep 2s

# ── Switch between tabs ────────────────────────────────────────────
# Number keys 1-9 switch tabs instantly
Type "1"
Sleep 1500ms
Type "2"
Sleep 1500ms
Type "3"
Sleep 1500ms
Type "1"
Sleep 1500ms

# ── Source picker ──────────────────────────────────────────────────
# Press b to open the source/backend picker
Type "b"
Sleep 2500ms

# Navigate the source list
Set TypingSpeed 300ms
Type "jj"
Sleep 800ms

# Close without switching
Escape
Sleep 1s

# ── Close a tab ────────────────────────────────────────────────────
# Switch to tab 3 and close it
Type "3"
Sleep 800ms
Ctrl+w
Sleep 1500ms

# Back on tab 2
Sleep 1s

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
