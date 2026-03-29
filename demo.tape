Output assets/demo.gif

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 800
Set Padding 10
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms

# Launch the dashboard
Type "mcp-dashboard"
Enter
Sleep 4s

# Navigate servers
Sleep 1s
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms
Type "k"
Sleep 1s

# Cycle through detail tabs (Tools -> Resources -> Prompts)
Type "	"
Sleep 1.5s
Type "	"
Sleep 1.5s
Type "	"
Sleep 1s

# Switch to Inspector tab
Type "2"
Sleep 2s

# Navigate tools
Type "j"
Sleep 600ms
Type "j"
Sleep 600ms
Type "k"
Sleep 1s

# Switch to Protocol tab
Type "3"
Sleep 2s

# Switch to Logs tab
Type "4"
Sleep 2s

# Back to Dashboard
Type "1"
Sleep 1s

# Show search
Type "/"
Sleep 500ms
Type "db"
Sleep 1s
Escape
Sleep 1s

# Show help overlay
Type "?"
Sleep 3s
Escape
Sleep 1s

# Quit
Type "q"
Sleep 500ms
