# 04 — Search & Navigation
#
# Vim-style navigation, in-buffer search with highlighting,
# bookmark system, and page scrolling. Shows oxo is built for
# keyboard-first power users.

Output demo/output/04-search-navigate.gif
Output demo/output/04-search-navigate.webm

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

# ── Launch and let logs accumulate ──────────────────────────────────
Type "oxo"
Enter
Sleep 5s

# ── Vim-style scrolling ────────────────────────────────────────────
# j/k to scroll line by line
Set TypingSpeed 180ms
Type "jjjjjjjjjjjjjjj"
Sleep 800ms

Type "kkkkkkkkk"
Sleep 800ms

# ── Jump to top with g ─────────────────────────────────────────────
Type "g"
Sleep 1s

# ── Jump to bottom with G ─────────────────────────────────────────
Type "G"
Sleep 1s

# ── Page scrolling ─────────────────────────────────────────────────
PageUp
Sleep 800ms
PageUp
Sleep 800ms
PageDown
Sleep 800ms

# ── Search ─────────────────────────────────────────────────────────
# Press / to open search (like vim)
Type "/"
Sleep 600ms

Set TypingSpeed 40ms
Type "error"
Enter
Sleep 1500ms

# ── Jump between matches ───────────────────────────────────────────
# n = next match, N = previous match
Type "n"
Sleep 700ms
Type "n"
Sleep 700ms
Type "n"
Sleep 700ms
Type "N"
Sleep 700ms
Type "N"
Sleep 700ms

# ── Bookmarks ──────────────────────────────────────────────────────
# Mark this line as a bookmark
Type "m"
Sleep 800ms

# Move down and bookmark another line
Set TypingSpeed 180ms
Type "jjjjjjjj"
Sleep 500ms
Type "m"
Sleep 800ms

# Move further down
Type "jjjjjjjjjj"
Sleep 500ms
Type "m"
Sleep 800ms

# ── Jump between bookmarks ─────────────────────────────────────────
# ' jumps to next bookmark
Type "'"
Sleep 800ms
Type "'"
Sleep 800ms
Type "'"
Sleep 800ms

# ── Search context lines ───────────────────────────────────────────
# C cycles context lines around search matches (0 → 3 → 5 → 10)
Type "C"
Sleep 1500ms
Type "C"
Sleep 1500ms

# ── Quit ───────────────────────────────────────────────────────────
Type "q"
Sleep 1s
