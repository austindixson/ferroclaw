# Path: /Users/ghost/Desktop/ferroclaw/scripts/uninstall.sh
# Module: scripts
# Purpose: Uninstall module
# Dependencies: 
# Related: 
# Keywords: uninstall, users, ghost, desktop, ferroclaw, scripts, uninstall.sh
# Last Updated: 2026-03-25

#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# Ferroclaw Uninstaller
#
# Cleanly removes ferroclaw binary, config, and data.
# Confirms before each destructive action.
#
# Usage:
#   bash scripts/uninstall.sh
#   curl -fsSL https://raw.githubusercontent.com/user/ferroclaw/main/scripts/uninstall.sh | bash
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Paths ─────────────────────────────────────────────────────────────────────

BINARY_PATH="$HOME/.local/bin/ferroclaw"
CONFIG_DIR="$HOME/.config/ferroclaw"
DATA_DIR="$HOME/.local/share/ferroclaw"
CACHE_DIR="$HOME/.cache/ferroclaw"

# macOS-specific paths (dirs crate uses Library/ on macOS)
if [ "$(uname -s)" = "Darwin" ]; then
    CONFIG_DIR="$HOME/Library/Application Support/ferroclaw"
    DATA_DIR="$HOME/Library/Application Support/ferroclaw"
    CACHE_DIR="$HOME/Library/Caches/ferroclaw"
fi

# ── Color & formatting ───────────────────────────────────────────────────────

if [ -t 1 ] && command -v tput &>/dev/null && [ "$(tput colors 2>/dev/null || echo 0)" -ge 8 ]; then
    BOLD=$(tput bold)
    DIM=$(tput dim)
    RESET=$(tput sgr0)
    RED=$(tput setaf 1)
    GREEN=$(tput setaf 2)
    YELLOW=$(tput setaf 3)
    CYAN=$(tput setaf 6)
    MAGENTA=$(tput setaf 5)
else
    BOLD="" DIM="" RESET=""
    RED="" GREEN="" YELLOW="" CYAN="" MAGENTA=""
fi

CHECKMARK="${GREEN}${BOLD}[ok]${RESET}"
WARN="${YELLOW}${BOLD}[!!]${RESET}"
SKIP="${DIM}[--]${RESET}"

# ── Helper functions ──────────────────────────────────────────────────────────

confirm() {
    local prompt="$1"
    local default="${2:-n}"

    if [ "$default" = "y" ]; then
        local hint="[Y/n]"
    else
        local hint="[y/N]"
    fi

    printf "  %s %s " "$prompt" "$hint"

    if [ -t 0 ]; then
        read -r answer </dev/tty
    else
        read -r answer
    fi

    answer="${answer:-$default}"
    case "$answer" in
        [yY]|[yY][eE][sS]) return 0 ;;
        *) return 1 ;;
    esac
}

success() {
    echo "  ${CHECKMARK} $1"
}

skip() {
    echo "  ${SKIP} $1"
}

warn() {
    echo "  ${WARN} ${YELLOW}$1${RESET}"
}

# ── Banner ────────────────────────────────────────────────────────────────────

echo ""
echo "${MAGENTA}${BOLD}"
echo "     ___                     _              "
echo "    / __\\__ _ __ _ __ ___   ___| | __ ___      __"
echo '   / _\ / _ \ '"'"'__| '"'"'__/ _ \ / __| |/ _` \ \ /\ / /'
echo "  / /  |  __/ |  | | | (_) | (__| | (_| |\\ V  V / "
echo "  \\/    \\___|_|  |_|  \\___/ \\___|_|\\__,_| \\_/\\_/  "
echo "${RESET}"
echo "  ${DIM}Uninstaller${RESET}"
echo ""
echo "  ${DIM}$(printf '%.0s─' {1..50})${RESET}"
echo ""

# ── Pre-flight: check what exists ────────────────────────────────────────────

FOUND_SOMETHING=false

echo "  ${BOLD}Scanning for ferroclaw files...${RESET}"
echo ""

if [ -f "$BINARY_PATH" ]; then
    BINARY_SIZE=$(du -h "$BINARY_PATH" 2>/dev/null | cut -f1 | xargs)
    echo "    ${CYAN}Binary:${RESET}  $BINARY_PATH ${DIM}(${BINARY_SIZE})${RESET}"
    FOUND_SOMETHING=true
else
    echo "    ${DIM}Binary:  not found${RESET}"
fi

if [ -d "$CONFIG_DIR" ]; then
    CONFIG_SIZE=$(du -sh "$CONFIG_DIR" 2>/dev/null | cut -f1 | xargs)
    CONFIG_COUNT=$(find "$CONFIG_DIR" -type f 2>/dev/null | wc -l | xargs)
    echo "    ${CYAN}Config:${RESET}  $CONFIG_DIR ${DIM}(${CONFIG_SIZE}, ${CONFIG_COUNT} files)${RESET}"
    FOUND_SOMETHING=true
else
    echo "    ${DIM}Config:  not found${RESET}"
fi

if [ -d "$DATA_DIR" ] && [ "$DATA_DIR" != "$CONFIG_DIR" ]; then
    DATA_SIZE=$(du -sh "$DATA_DIR" 2>/dev/null | cut -f1 | xargs)
    echo "    ${CYAN}Data:${RESET}    $DATA_DIR ${DIM}(${DATA_SIZE})${RESET}"
    FOUND_SOMETHING=true
elif [ -d "$DATA_DIR" ]; then
    # On macOS, config and data may be the same directory — avoid double-counting
    :
else
    echo "    ${DIM}Data:    not found${RESET}"
fi

if [ -d "$CACHE_DIR" ]; then
    CACHE_SIZE=$(du -sh "$CACHE_DIR" 2>/dev/null | cut -f1 | xargs)
    echo "    ${CYAN}Cache:${RESET}   $CACHE_DIR ${DIM}(${CACHE_SIZE})${RESET}"
    FOUND_SOMETHING=true
else
    echo "    ${DIM}Cache:   not found${RESET}"
fi

echo ""

if [ "$FOUND_SOMETHING" = false ]; then
    echo "  ${DIM}Nothing to remove. Ferroclaw does not appear to be installed.${RESET}"
    echo ""
    exit 0
fi

# ── Confirm overall ──────────────────────────────────────────────────────────

echo "  ${DIM}$(printf '%.0s─' {1..50})${RESET}"
echo ""

if ! confirm "${BOLD}Proceed with uninstall?${RESET}" "n"; then
    echo ""
    echo "  ${DIM}Aborted. No changes made.${RESET}"
    echo ""
    exit 0
fi

echo ""

# ── Remove binary ────────────────────────────────────────────────────────────

if [ -f "$BINARY_PATH" ]; then
    rm -f "$BINARY_PATH"
    success "Removed binary: ${DIM}$BINARY_PATH${RESET}"
else
    skip "Binary not found (already removed)"
fi

# ── Remove config ────────────────────────────────────────────────────────────

if [ -d "$CONFIG_DIR" ]; then
    echo ""
    echo "  ${YELLOW}${BOLD}Config directory contains:${RESET}"

    # List important files
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        echo "    - config.toml (provider settings, channels, security)"
    fi
    if [ -f "$CONFIG_DIR/.env" ]; then
        echo "    - .env (API keys and secrets)"
    fi

    # List any other files
    OTHER_FILES=$(find "$CONFIG_DIR" -type f ! -name "config.toml" ! -name ".env" 2>/dev/null | head -5)
    if [ -n "$OTHER_FILES" ]; then
        echo "$OTHER_FILES" | while IFS= read -r f; do
            echo "    - $(basename "$f")"
        done
    fi

    echo ""
    if confirm "${RED}Delete config directory?${RESET} (API keys in .env will be lost)" "n"; then
        rm -rf "$CONFIG_DIR"
        success "Removed config: ${DIM}$CONFIG_DIR${RESET}"
    else
        skip "Kept config directory"
    fi
else
    skip "Config directory not found"
fi

# ── Remove data ──────────────────────────────────────────────────────────────

if [ -d "$DATA_DIR" ] && [ "$DATA_DIR" != "$CONFIG_DIR" ]; then
    echo ""
    echo "  ${YELLOW}${BOLD}Data directory contains:${RESET}"
    echo "    - Memory database (conversation history, embeddings)"
    echo "    - Audit logs"
    echo ""
    if confirm "${RED}Delete data directory?${RESET} (memory and audit logs will be lost)" "n"; then
        rm -rf "$DATA_DIR"
        success "Removed data: ${DIM}$DATA_DIR${RESET}"
    else
        skip "Kept data directory"
    fi
elif [ "$DATA_DIR" = "$CONFIG_DIR" ]; then
    # Already handled above on macOS
    :
else
    skip "Data directory not found"
fi

# ── Remove cache ─────────────────────────────────────────────────────────────

if [ -d "$CACHE_DIR" ]; then
    rm -rf "$CACHE_DIR"
    success "Removed cache: ${DIM}$CACHE_DIR${RESET}"
else
    skip "Cache directory not found"
fi

# ── Clean PATH reference ────────────────────────────────────────────────────

echo ""

SHELL_NAME="$(basename "${SHELL:-/bin/bash}")"
SHELL_RC=""

case "$SHELL_NAME" in
    zsh)   SHELL_RC="$HOME/.zshrc" ;;
    bash)
        if [ -f "$HOME/.bashrc" ]; then
            SHELL_RC="$HOME/.bashrc"
        elif [ -f "$HOME/.bash_profile" ]; then
            SHELL_RC="$HOME/.bash_profile"
        fi
        ;;
    fish)  SHELL_RC="$HOME/.config/fish/config.fish" ;;
    *)     SHELL_RC="$HOME/.profile" ;;
esac

if [ -n "$SHELL_RC" ] && [ -f "$SHELL_RC" ]; then
    if grep -q "# Added by Ferroclaw installer" "$SHELL_RC" 2>/dev/null; then
        # Remove the comment and the PATH line
        # Use a temp file for portable sed behavior
        TMPFILE=$(mktemp)
        grep -v "# Added by Ferroclaw installer" "$SHELL_RC" | grep -v 'local/bin.*ferroclaw\|fish_add_path.*local/bin' > "$TMPFILE" || true

        # Only write back if content actually changed
        if ! diff -q "$SHELL_RC" "$TMPFILE" &>/dev/null; then
            cp "$TMPFILE" "$SHELL_RC"
            success "Cleaned PATH entry from ${DIM}$SHELL_RC${RESET}"
        fi
        rm -f "$TMPFILE"
    fi
fi

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "  ${DIM}$(printf '%.0s─' {1..50})${RESET}"
echo ""
echo "  ${GREEN}${BOLD}Uninstall complete.${RESET}"
echo ""

if [ -d "$CONFIG_DIR" ] || ([ -d "$DATA_DIR" ] && [ "$DATA_DIR" != "$CONFIG_DIR" ]); then
    echo "  ${DIM}Some directories were kept at your request.${RESET}"
    [ -d "$CONFIG_DIR" ] && echo "    ${DIM}$CONFIG_DIR${RESET}"
    [ -d "$DATA_DIR" ] && [ "$DATA_DIR" != "$CONFIG_DIR" ] && echo "    ${DIM}$DATA_DIR${RESET}"
    echo ""
fi

echo "  ${DIM}Thanks for trying Ferroclaw.${RESET}"
echo ""
