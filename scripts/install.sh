# Path: /Users/ghost/Desktop/ferroclaw/scripts/install.sh
# Module: scripts
# Purpose: Install module
# Dependencies: git, rust/cargo, curl or wget
# Related: 
# Keywords: install, users, ghost, desktop, ferroclaw, scripts, install.sh
# Last Updated: 2026-03-25

#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# Ferroclaw Installer
#
# Security-first AI agent with native MCP and DietMCP compression.
#
# One-liner:
#   curl -fsSL https://raw.githubusercontent.com/RuneweaverStudios/ferroclaw/main/scripts/install.sh | bash
#
# Options:
#   --skip-setup    Skip the interactive onboarding wizard after install
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

REPO_URL="https://github.com/RuneweaverStudios/ferroclaw.git"
REPO_BRANCH="main"
INSTALL_DIR="$HOME/.local/bin"
BINARY_NAME="ferroclaw"
BUILD_DIR="${TMPDIR:-/tmp}/ferroclaw-install-$$"
VERSION="0.1.0"

# ── Color & formatting ───────────────────────────────────────────────────────

if [ -t 1 ] && command -v tput &>/dev/null && [ "$(tput colors 2>/dev/null || echo 0)" -ge 8 ]; then
    BOLD=$(tput bold)
    DIM=$(tput dim)
    RESET=$(tput sgr0)
    RED=$(tput setaf 1)
    GREEN=$(tput setaf 2)
    YELLOW=$(tput setaf 3)
    BLUE=$(tput setaf 4)
    MAGENTA=$(tput setaf 5)
    CYAN=$(tput setaf 6)
    WHITE=$(tput setaf 7)
else
    BOLD="" DIM="" RESET=""
    RED="" GREEN="" YELLOW="" BLUE="" MAGENTA="" CYAN="" WHITE=""
fi

CHECKMARK="${GREEN}${BOLD}[ok]${RESET}"
ARROW="${CYAN}${BOLD} >> ${RESET}"
WARN="${YELLOW}${BOLD}[!!]${RESET}"
FAIL="${RED}${BOLD}[!!]${RESET}"
STEP_NUM=0

# ── Parse arguments ──────────────────────────────────────────────────────────

SKIP_SETUP=false
for arg in "$@"; do
    case "$arg" in
        --skip-setup) SKIP_SETUP=true ;;
        --help|-h)
            echo "Usage: install.sh [--skip-setup]"
            echo ""
            echo "Options:"
            echo "  --skip-setup    Skip the interactive onboarding wizard"
            echo "  --help, -h      Show this help message"
            exit 0
            ;;
        *)
            echo "${FAIL} Unknown argument: $arg"
            echo "Run with --help for usage."
            exit 1
            ;;
    esac
done

# ── Helper functions ──────────────────────────────────────────────────────────

step() {
    STEP_NUM=$((STEP_NUM + 1))
    echo ""
    echo "${BLUE}${BOLD}  [$STEP_NUM]  $1${RESET}"
    echo "${DIM}  $(printf '%.0s─' {1..50})${RESET}"
}

info() {
    echo "${ARROW}$1"
}

success() {
    echo "  ${CHECKMARK} $1"
}

warn() {
    echo "  ${WARN} ${YELLOW}$1${RESET}"
}

fail() {
    echo ""
    echo "  ${FAIL} ${RED}${BOLD}$1${RESET}"
    echo ""
    exit 1
}

cleanup() {
    if [ -d "$BUILD_DIR" ]; then
        rm -rf "$BUILD_DIR"
    fi
}

trap cleanup EXIT

# ── Banner ────────────────────────────────────────────────────────────────────

print_banner() {
    echo ""
    echo "${MAGENTA}${BOLD}"
    echo "     ___                     _              "
    echo "    / __\\__ _ __ _ __ ___   ___| | __ ___      __"
    echo '   / _\ / _ \ '"'"'__| '"'"'__/ _ \ / __| |/ _` \ \ /\ / /'
    echo "  / /  |  __/ |  | | | (_) | (__| | (_| |\\ V  V / "
    echo "  \\/    \\___|_|  |_|  \\___/ \\___|_|\\__,_| \\_/\\_/  "
    echo "${RESET}"
    echo "  ${DIM}v${VERSION}  --  Security-first AI agent${RESET}"
    echo "  ${DIM}84 skills | 7 channels | 4 providers | DietMCP${RESET}"
    echo ""
    echo "  ${DIM}$(printf '%.0s─' {1..50})${RESET}"
}

print_banner

# ── Step 1: Detect OS & Architecture ─────────────────────────────────────────

step "Detecting platform"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)  OS_NAME="macOS" ;;
    Linux)   OS_NAME="Linux" ;;
    *)       fail "Unsupported operating system: $OS. Ferroclaw supports macOS and Linux." ;;
esac

case "$ARCH" in
    x86_64|amd64)    ARCH_NAME="x86_64" ;;
    aarch64|arm64)   ARCH_NAME="aarch64" ;;
    *)               fail "Unsupported architecture: $ARCH. Ferroclaw supports x86_64 and aarch64." ;;
esac

success "OS: ${BOLD}${OS_NAME}${RESET}  Arch: ${BOLD}${ARCH_NAME}${RESET}"

# ── Step 2: Check dependencies ───────────────────────────────────────────────

step "Checking dependencies"

# Check for git
if command -v git &>/dev/null; then
    GIT_VERSION=$(git --version | head -1)
    success "git: ${DIM}${GIT_VERSION}${RESET}"
else
    fail "git is not installed. Please install git first:
       macOS:  xcode-select --install
       Linux:  sudo apt install git  (or your package manager)"
fi

# Check for Rust/cargo
if command -v cargo &>/dev/null; then
    RUST_VERSION=$(rustc --version 2>/dev/null || echo "unknown")
    CARGO_VERSION=$(cargo --version 2>/dev/null || echo "unknown")
    success "rustc: ${DIM}${RUST_VERSION}${RESET}"
    success "cargo: ${DIM}${CARGO_VERSION}${RESET}"
else
    warn "Rust/cargo not found."
    echo ""
    echo "${ARROW}Ferroclaw requires Rust to build from source."
    echo "${ARROW}Install now via rustup (the official Rust installer)?"
    echo ""
    printf "  ${BOLD}Install Rust? [Y/n]${RESET} "
    read -r INSTALL_RUST </dev/tty || INSTALL_RUST="y"
    INSTALL_RUST="${INSTALL_RUST:-y}"

    case "$INSTALL_RUST" in
        [yY]|[yY][eE][sS]|"")
            info "Installing Rust via rustup..."
            echo ""
            if command -v curl &>/dev/null; then
                curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
            elif command -v wget &>/dev/null; then
                wget -qO- https://sh.rustup.rs | sh -s -- -y
            else
                fail "Neither curl nor wget found. Install one of them first, or install Rust manually:
       https://rustup.rs"
            fi

            # Source the cargo env so we can use it immediately
            if [ -f "$HOME/.cargo/env" ]; then
                # shellcheck source=/dev/null
                . "$HOME/.cargo/env"
            fi

            if command -v cargo &>/dev/null; then
                success "Rust installed successfully"
            else
                fail "Rust installation completed but cargo not found in PATH.
       Try restarting your shell and running this script again."
            fi
            ;;
        *)
            fail "Rust is required to build Ferroclaw. Install it from https://rustup.rs and try again."
            ;;
    esac
fi

# Check for curl or wget (needed for potential rustup install, also good to verify)
if command -v curl &>/dev/null; then
    success "curl: ${DIM}available${RESET}"
elif command -v wget &>/dev/null; then
    success "wget: ${DIM}available${RESET}"
else
    warn "Neither curl nor wget found (non-critical, but recommended)"
fi

# ── Step 3: Get source code ──────────────────────────────────────────────────

step "Getting source code"

# Check if we are already inside a ferroclaw checkout
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || echo "")"
PARENT_DIR="$(dirname "$SCRIPT_DIR" 2>/dev/null || echo "")"

SOURCE_DIR=""

if [ -n "$PARENT_DIR" ] && [ -f "$PARENT_DIR/Cargo.toml" ]; then
    # Check if the Cargo.toml belongs to ferroclaw
    if grep -q 'name = "ferroclaw"' "$PARENT_DIR/Cargo.toml" 2>/dev/null; then
        SOURCE_DIR="$PARENT_DIR"
        success "Using existing checkout: ${DIM}${SOURCE_DIR}${RESET}"
    fi
fi

if [ -z "$SOURCE_DIR" ]; then
    # Check if we're in a ferroclaw repo already
    if [ -f "./Cargo.toml" ] && grep -q 'name = "ferroclaw"' "./Cargo.toml" 2>/dev/null; then
        SOURCE_DIR="$(pwd)"
        success "Using current directory: ${DIM}${SOURCE_DIR}${RESET}"
    else
        # Clone fresh
        info "Cloning ferroclaw from ${DIM}${REPO_URL}${RESET}"
        mkdir -p "$BUILD_DIR"
        git clone --depth 1 --branch "$REPO_BRANCH" "$REPO_URL" "$BUILD_DIR/ferroclaw" 2>&1 | while IFS= read -r line; do
            echo "       ${DIM}${line}${RESET}"
        done
        SOURCE_DIR="$BUILD_DIR/ferroclaw"
        success "Cloned to temporary directory"
    fi
fi

# ── Step 4: Build release binary ─────────────────────────────────────────────

step "Building release binary"

info "This may take a few minutes on first build..."
info "Profile: ${BOLD}release${RESET} (LTO enabled, stripped)"
echo ""

cd "$SOURCE_DIR"

if cargo build --release 2>&1 | while IFS= read -r line; do
    # Show cargo output indented
    echo "       ${DIM}${line}${RESET}"
done; then
    success "Build completed successfully"
else
    fail "Build failed. Check the error output above.
       Common fixes:
         - Update Rust: rustup update
         - Install build tools: xcode-select --install (macOS)
         - Install build tools: sudo apt install build-essential (Linux)"
fi

# Verify the binary exists
BINARY_PATH="$SOURCE_DIR/target/release/$BINARY_NAME"
if [ ! -f "$BINARY_PATH" ]; then
    fail "Binary not found at expected path: $BINARY_PATH"
fi

BINARY_SIZE=$(du -h "$BINARY_PATH" | cut -f1 | xargs)
success "Binary size: ${BOLD}${BINARY_SIZE}${RESET} (stripped + LTO)"

# ── Step 5: Install binary ──────────────────────────────────────────────────

step "Installing binary"

# Create install directory if needed
if [ ! -d "$INSTALL_DIR" ]; then
    info "Creating ${DIM}${INSTALL_DIR}${RESET}"
    mkdir -p "$INSTALL_DIR"
    success "Created $INSTALL_DIR"
fi

# Copy the binary
info "Installing to ${DIM}${INSTALL_DIR}/${BINARY_NAME}${RESET}"
cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
chmod 755 "$INSTALL_DIR/$BINARY_NAME"
success "Binary installed"

# Verify it runs (short timeout — under memory pressure macOS may SIGKILL new ferroclaw processes)
VERIFY_OK=false
if command -v perl >/dev/null 2>&1; then
    if perl -e 'alarm 3; exec @ARGV' "$INSTALL_DIR/$BINARY_NAME" --version &>/dev/null; then
        VERIFY_OK=true
    fi
elif "$INSTALL_DIR/$BINARY_NAME" --version &>/dev/null; then
    VERIFY_OK=true
fi
if $VERIFY_OK; then
    INSTALLED_VERSION=$("$INSTALL_DIR/$BINARY_NAME" --version 2>/dev/null || echo "unknown")
    success "Verified: ${DIM}${INSTALLED_VERSION}${RESET}"
else
    warn "Binary installed but could not verify (run: pkill -9 -f 'ferroclaw serve'; ferroclaw --version)"
fi

# ── Step 6: Update PATH ─────────────────────────────────────────────────────

step "Checking PATH"

PATH_UPDATED=false

if echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    success "$INSTALL_DIR is already in PATH"
else
    warn "$INSTALL_DIR is not in your PATH"
    info "Adding it now..."

    # Detect which shell config to modify
    SHELL_NAME="$(basename "${SHELL:-/bin/bash}")"
    SHELL_RC=""

    case "$SHELL_NAME" in
        zsh)
            SHELL_RC="$HOME/.zshrc"
            ;;
        bash)
            # Prefer .bashrc; fall back to .bash_profile on macOS
            if [ -f "$HOME/.bashrc" ]; then
                SHELL_RC="$HOME/.bashrc"
            elif [ -f "$HOME/.bash_profile" ]; then
                SHELL_RC="$HOME/.bash_profile"
            else
                SHELL_RC="$HOME/.bashrc"
            fi
            ;;
        fish)
            SHELL_RC="$HOME/.config/fish/config.fish"
            ;;
        *)
            SHELL_RC="$HOME/.profile"
            ;;
    esac

    PATH_LINE="export PATH=\"$INSTALL_DIR:\$PATH\""

    # Check if we already added it (idempotent)
    if [ -f "$SHELL_RC" ] && grep -qF "$INSTALL_DIR" "$SHELL_RC" 2>/dev/null; then
        success "PATH entry already exists in ${DIM}${SHELL_RC}${RESET}"
    else
        if [ "$SHELL_NAME" = "fish" ]; then
            PATH_LINE="fish_add_path $INSTALL_DIR"
        fi

        echo "" >> "$SHELL_RC"
        echo "# Added by Ferroclaw installer" >> "$SHELL_RC"
        echo "$PATH_LINE" >> "$SHELL_RC"
        success "Added to ${DIM}${SHELL_RC}${RESET}"
        PATH_UPDATED=true
    fi

    # Update PATH for the current session
    export PATH="$INSTALL_DIR:$PATH"
fi

# ── Step 7: Run setup wizard ────────────────────────────────────────────────

if [ "$SKIP_SETUP" = true ]; then
    step "Setup wizard (skipped)"
    info "Run ${BOLD}ferroclaw setup${RESET} later to configure providers and channels."
else
    step "Running onboarding wizard"
    info "Launching ${BOLD}ferroclaw setup${RESET}..."
    echo ""

    # Run setup interactively — needs terminal access
    if [ -t 0 ]; then
        "$INSTALL_DIR/$BINARY_NAME" setup </dev/tty || {
            warn "Setup wizard exited with an error."
            info "You can run it again anytime: ${BOLD}ferroclaw setup${RESET}"
        }
    else
        warn "No interactive terminal detected (piped install)."
        info "Run the wizard manually: ${BOLD}ferroclaw setup${RESET}"
    fi
fi

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "  ${DIM}$(printf '%.0s─' {1..50})${RESET}"
echo ""
echo "  ${GREEN}${BOLD}Installation complete!${RESET}"
echo ""
echo "  ${DIM}Binary:${RESET}    $INSTALL_DIR/$BINARY_NAME"
echo "  ${DIM}Config:${RESET}    ~/.config/ferroclaw/config.toml"
echo "  ${DIM}Data:${RESET}      ~/.local/share/ferroclaw/"
echo "  ${DIM}Logs:${RESET}      ~/.cache/ferroclaw/"
echo ""

if [ "$PATH_UPDATED" = true ]; then
    echo "  ${YELLOW}${BOLD}NOTE:${RESET} Restart your shell or run:"
    echo "       ${BOLD}source ${SHELL_RC}${RESET}"
    echo ""
fi

echo "  ${BOLD}Quick start:${RESET}"
echo ""
echo "    ${CYAN}ferroclaw setup${RESET}             Configure providers, skills, channels"
echo "    ${CYAN}ferroclaw run${RESET}               Start interactive REPL"
echo "    ${CYAN}ferroclaw exec \"Hello\"${RESET}      One-shot mode"
echo "    ${CYAN}ferroclaw serve${RESET}             Start gateway + messaging bots"
echo ""
echo "  ${DIM}$(printf '%.0s─' {1..50})${RESET}"
echo ""
