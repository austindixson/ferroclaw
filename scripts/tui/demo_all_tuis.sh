#!/bin/bash
# Demo script to showcase all TUI designs in Ferroclaw
# This script switches between TUIs and runs them with example prompts

set -e

echo "╔════════════════════════════════════════════════════════════╗"
echo "║         Ferroclaw TUI Design Showcase & Demo              ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Prompt examples
PROMPT_SIMPLE="Hello! Can you help me build a TUI?"
PROMPT_FILE="Read the main.rs file and tell me what it does"
PROMPT_TOOLS="List all the TUI designs in the project"
PROMPT_COMPLEX="Analyze the kinetic_tui.rs file and explain its design philosophy"

# Function to display TUI info
show_tui_info() {
    local tui_type=$1
    local description=$2
    local features=$3

    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}$tui_type${NC}"
    echo ""
    echo "$description"
    echo ""
    echo -e "${YELLOW}Key Features:${NC}"
    echo "$features"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

# Function to run TUI demo
run_tui_demo() {
    local tui_type=$1
    local prompt=$2

    echo -e "${BLUE}→ Launching $tui_type...${NC}"
    echo ""
    read -p "Press Enter to continue (or Ctrl+C to skip)..."
    echo ""

    # Launch the TUI with the prompt
    echo "$prompt" | ./target/release/ferroclaw run
}

# Check if binary exists
if [ ! -f "./target/release/ferroclaw" ]; then
    echo -e "${RED}Error: ferroclaw binary not found!${NC}"
    echo ""
    echo "Please build it first:"
    echo "  cargo build --release"
    exit 1
fi

# Display menu
echo -e "${YELLOW}Available TUI Designs:${NC}"
echo ""
echo "  1. Kinetic TUI       (Motion-focused, animated)"
echo "  2. Hermes TUI        (Dark, polished, task sidebar)"
echo "  3. Orchestrator TUI  (Real-time transcript, tool visibility)"
echo "  4. Minimal TUI       (Brutalist, no borders)"
echo "  5. Standard TUI      (Classic, structured)"
echo "  6. Demo All TUIs     (Showcase each in sequence)"
echo "  7. Quick Comparison  (Side-by-side info)"
echo ""
echo "  0. Exit"
echo ""

read -p "Select an option (0-7): " choice

case $choice in
    1)
        show_tui_info \
            "KINETIC TUI" \
            "Motion is information - the interface breathes with the agent. Features animated glitter verbs and a kinetic status bar with progress indicator." \
            "• Animated glitter verbs (thinking, contemplating, mulling)
             • Kinetic status bar with pulse animation
             • Progress bar showing token usage
             • Glitch aesthetic - raw and alive
             • No borders - typography over chrome"
        run_tui_demo "Kinetic TUI" "$PROMPT_SIMPLE"
        ;;
    2)
        show_tui_info \
            "HERMES TUI" \
            "Dark theme with polished chat interface. Inspired by the Hermes agent TUI, featuring message bubbles and a task management sidebar." \
            "• Dark theme throughout
             • Message bubbles with headers
             • Left sidebar with task list
             • Bottom status bar with model info
             • Tool calls/results in chat history"
        run_tui_demo "Hermes TUI" "$PROMPT_FILE"
        ;;
    3)
        show_tui_info \
            "ORCHESTRATOR TUI" \
            "Nyx-inspired transcript interface showing every tool call in real-time. Perfect for debugging and understanding agent decision-making." \
            "• Real-time tool line visibility
             • Iteration counter showing LLM round
             • Verbs for different actions (Reading…, Writing…, Executing…)
             • Parallel tool batch notifications
             • Long wait nudges (after 30s)
             • Dark palette with teal accents"
        run_tui_demo "Orchestrator TUI" "$PROMPT_TOOLS"
        ;;
    4)
        show_tui_info \
            "MINIMAL TUI" \
            "Brutalist design with no borders, maximum screen real estate for content. Pure utilitarian aesthetic for power users." \
            "• Zero borders - pure content focus
             • Brutalist terminal aesthetic
             • Glitter verbs for status
             • Status through typography (not UI widgets)
             • Maximum usable content area (95%)"
        run_tui_demo "Minimal TUI" "$PROMPT_COMPLEX"
        ;;
    5)
        show_tui_info \
            "STANDARD TUI" \
            "Classic ratatui TUI with structured layout. Traditional block borders, top banner bar, and organized sections." \
            "• Traditional block borders
             • Top banner bar (model, tokens)
             • Scrollable chat history
             • Multiline input with cursor support
             • Status bar with connection info
             • Full keyboard shortcut support"
        run_tui_demo "Standard TUI" "$PROMPT_SIMPLE"
        ;;
    6)
        echo -e "${YELLOW}Starting TUI Showcase...${NC}"
        echo ""
        echo "This will cycle through all TUI designs. Press Ctrl+C at any time to exit."
        echo ""

        # Demo all TUIs in sequence
        for tui in "Kinetic" "Hermes" "Orchestrator" "Minimal" "Standard"; do
            echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
            echo -e "${GREEN}$tui TUI${NC}"
            echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
            echo ""
            read -p "Press Enter to launch $tui TUI (or Ctrl+C to stop)..."
            echo ""
        done
        ;;
    7)
        echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${CYAN}║                  TUI QUICK COMPARISON                      ║${NC}"
        echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
        echo ""
        echo -e "${GREEN}KINETIC${NC}      Motion-focused, animated glitter verbs, progress bar"
        echo -e "${GREEN}HERMES${NC}       Dark theme, message bubbles, task sidebar"
        echo -e "${GREEN}ORCHESTRATOR${NC} Real-time transcript, tool visibility, debug mode"
        echo -e "${GREEN}MINIMAL${NC}      Brutalist, no borders, 95% content area"
        echo -e "${GREEN}STANDARD${NC}     Classic, structured, traditional layout"
        echo ""
        echo "─────────────────────────────────────────────────────────────────"
        echo ""
        echo "Best for:"
        echo "  Visual feedback    → Kinetic"
        echo "  Chat with tasks     → Hermes"
        echo "  Debugging flow     → Orchestrator"
        echo "  Power users        → Minimal"
        echo "  Traditional UI     → Standard"
        echo ""
        echo "─────────────────────────────────────────────────────────────────"
        echo ""
        echo "See full details in: TUI_DESIGNS.md, TUI_COMPARISON.md, TUI_PREVIEW_GUIDE.md"
        ;;
    0)
        echo "Goodbye!"
        exit 0
        ;;
    *)
        echo -e "${RED}Invalid option!${NC}"
        exit 1
        ;;
esac

echo ""
echo -e "${GREEN}Demo complete!${NC}"
echo ""
echo "To switch between TUIs, use: ./switch_tui.sh [kinetic|hermes|orchestrator|minimal|standard]"
echo "See TUI_PREVIEW_GUIDE.md for detailed instructions."
