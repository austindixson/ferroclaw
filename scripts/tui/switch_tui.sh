#!/bin/bash
# Script to quickly switch between TUI designs in Ferroclaw
# Usage: ./switch_tui.sh [kinetic|hermes|orchestrator|minimal|standard]

set -e

TUI_TYPE="${1:-kinetic}"

echo "=== Ferroclaw TUI Switcher ==="
echo ""

# Read the current main.rs
MAIN_RS="src/main.rs"

case "$TUI_TYPE" in
  kinetic)
    echo "Switching to KINETIC TUI (motion-focused, glitter verbs, animation)"
    # This is the default - already in place
    if grep -q "run_kinetic_tui" "$MAIN_RS"; then
      echo "✓ Kinetic TUI is already active"
    else
      echo "✗ Error: Kinetic TUI line not found in main.rs"
      echo "  Current line 62 should contain: run_kinetic_tui"
    fi
    ;;
  hermes)
    echo "Switching to HERMES TUI (dark theme, message bubbles, task sidebar)"
    sed -i.bak 's/ferroclaw::tui::kinetic_tui::run_kinetic_tui/ferroclaw::tui::hermes_tui::run_hermes_tui/' "$MAIN_RS"
    echo "✓ Updated main.rs - now rebuild with: cargo build --release"
    ;;
  orchestrator)
    echo "Switching to ORCHESTRATOR TUI (real-time transcript, tool visibility)"
    sed -i.bak 's/ferroclaw::tui::kinetic_tui::run_kinetic_tui/ferroclaw::tui::orchestrator_tui::run_orchestrator_tui/' "$MAIN_RS"
    echo "✓ Updated main.rs - now rebuild with: cargo build --release"
    ;;
  minimal)
    echo "Switching to MINIMAL TUI (brutalist, no borders, glitter verbs)"
    sed -i.bak 's/ferroclaw::tui::kinetic_tui::run_kinetic_tui/ferroclaw::tui::minimal_tui::run_minimal_tui/' "$MAIN_RS"
    echo "✓ Updated main.rs - now rebuild with: cargo build --release"
    ;;
  standard)
    echo "Switching to STANDARD TUI (classic with structured layout)"
    sed -i.bak 's/ferroclaw::tui::kinetic_tui::run_kinetic_tui/ferroclaw::tui::run_tui/' "$MAIN_RS"
    echo "✓ Updated main.rs - now rebuild with: cargo build --release"
    ;;
  *)
    echo "Unknown TUI type: $TUI_TYPE"
    echo ""
    echo "Usage: $0 [kinetic|hermes|orchestrator|minimal|standard]"
    echo ""
    echo "Available TUI designs:"
    echo "  kinetic      - Motion-focused with glitter verbs and animation (default)"
    echo "  hermes       - Dark theme with message bubbles and task sidebar"
    echo "  orchestrator - Real-time transcript showing every tool call"
    echo "  minimal      - Brutalist design with no borders, maximum content"
    echo "  standard     - Classic TUI with structured layout and borders"
    exit 1
    ;;
esac

echo ""
echo "Next steps:"
echo "1. Rebuild: cargo build --release"
echo "2. Launch:  ./target/release/ferroclaw run"
echo ""
