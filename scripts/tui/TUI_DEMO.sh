#!/bin/bash
# Quick visual demo of the minimal TUI
# Run this to see what the interface looks like

cat <<'EOF'
╔──────────────────────────────────────────────────────╗
║                                                      ║
║  ← AI agent response here                              ║
║    → tool_name (tool call)                           ║
║    ← ✓ tool_result (success)                          ║
║                                                       ║
║  [You] Your message to the agent                       ║
║ >  Type your prompt here and press Enter_               ║
║                                                       ║
║                                                       ║
║●thinking claude-3-sonnet·4·45%                          ║
╚──────────────────────────────────────────────────────╝
EOF

echo ""
echo "MINIMAL TUI - KEY FEATURES"
echo "========================="
echo ""
echo "STATUS LINE (top):"
echo "  ●thinking     → Agent is running (cyan)"
echo "  ○ready        → Idle (green)"
echo "  ●error        → Error occurred (red)"
echo ""
echo "SYMBOLS:"
echo "  → tool_name  → Tool being called (yellow)"
echo "  ← ✓ tool    → Tool succeeded (green)"
echo "  ← ✗ tool    → Tool failed (red)"
echo ""
echo "INPUT PROMPT:"
echo "  > _          → Cursor waiting for input"
echo ""
echo "SCROLLING:"
echo "  ↑/↓         → Scroll line by line"
echo "  PageUp/Down → Scroll by page"
echo ""
echo "KEYBOARD SHORTCUTS:"
echo "  Enter        → Send message"
echo "  Shift+Enter  → New line in input"
echo "  Ctrl+C       → Quit"
echo ""
echo "COLOR SCHEME:"
echo "  Cyan         → Your input, status"
echo "  White        → AI responses"
echo "  Yellow       → Tool calls"
echo "  Green        → Success"
echo "  Red          → Errors"
echo "  Dark Gray    → Metadata, markers"
echo ""
