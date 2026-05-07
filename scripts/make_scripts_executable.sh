#!/bin/bash
# Make all TUI scripts executable
# Run this once before using the scripts

echo "Making TUI scripts executable..."
chmod +x switch_tui.sh
chmod +x demo_all_tuis.sh
chmod +x make_scripts_executable.sh

echo ""
echo "✓ All scripts are now executable"
echo ""
echo "You can now run:"
echo "  ./switch_tui.sh [kinetic|hermes|orchestrator|minimal|standard]"
echo "  ./demo_all_tuis.sh"
