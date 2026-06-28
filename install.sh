#!/bin/bash

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Configuration
APP_DIR="/opt/ToRouter-Multi-Location"
INSTALLATION_SCRIPT="${APP_DIR}/installation.sh"
DOWNLOAD_URL="https://github.com/ArashAfkandeh/ToRouter-Multi-Location/releases/download/ToRouter/ToRouter-Multi-Location-v0.1.0.tar.gz"
TARBALL_NAME="ToRouter-Multi-Location-v0.1.0.tar.gz"
TARBALL_PATH="/root/${TARBALL_NAME}"

# Function to print colored output
print_colored() {
    local color=$1
    local message=$2
    echo -e "${color}${message}${NC}"
}

# Function to print header
print_header() {
    clear
    print_colored "$CYAN" "╔═══════════════════════════════════════════════════════════════╗"
    print_colored "$CYAN" "║          📦 ToRouter Installation Manager v2.0               ║"
    print_colored "$CYAN" "╚═══════════════════════════════════════════════════════════════╝"
    echo ""
}

# Function to check if running as root
check_root() {
    if [ "$EUID" -ne 0 ]; then
        print_colored "$RED" "✗ Error: This script must be run as root (use sudo)"
        exit 1
    fi
}

# Function to install dependencies
install_dependencies() {
    print_colored "$YELLOW" "📦 Installing dependencies..."
    apt update
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Error: Failed to run apt update"
        exit 1
    fi
    
    apt install -y curl
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Error: Failed to install curl"
        exit 1
    fi
    print_colored "$GREEN" "✓ Dependencies installed successfully"
}

# Function to download the tarball
download_tarball() {
    print_colored "$YELLOW" "📥 Downloading ToRouter package..."
    curl -L -o "$TARBALL_PATH" "$DOWNLOAD_URL"
    if [ $? -ne 0 ] || [ ! -f "$TARBALL_PATH" ]; then
        print_colored "$RED" "✗ Error: Failed to download the package"
        exit 1
    fi
    print_colored "$GREEN" "✓ Package downloaded successfully to $TARBALL_PATH"
}

# Function to extract the tarball
extract_tarball() {
    print_colored "$YELLOW" "📂 Extracting package to /opt..."
    tar -xzf "$TARBALL_PATH" -C /opt
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Error: Failed to extract the package"
        exit 1
    fi
    print_colored "$GREEN" "✓ Package extracted successfully to $APP_DIR"
}

# Function to clean up the tarball
cleanup_tarball() {
    print_colored "$YELLOW" "🗑 Removing downloaded tarball..."
    rm -f "$TARBALL_PATH"
    if [ $? -ne 0 ]; then
        print_colored "$YELLOW" "⚠️  Warning: Failed to remove tarball (may not exist)"
    else
        print_colored "$GREEN" "✓ Tarball removed successfully"
    fi
}

# Function to execute installation script
execute_installation_script() {
    local command=$1
    
    # Check if installation script exists
    if [ ! -f "$INSTALLATION_SCRIPT" ]; then
        print_colored "$RED" "✗ Error: Installation script not found at $INSTALLATION_SCRIPT"
        exit 1
    fi
    
    # Make the script executable
    print_colored "$YELLOW" "🔧 Making installation script executable..."
    chmod +x "$INSTALLATION_SCRIPT"
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Error: Failed to make script executable"
        exit 1
    fi
    print_colored "$GREEN" "✓ Script is now executable"
    
    # Execute the script with the provided command
    echo ""
    print_colored "$CYAN" "════════════════════════════════════════════════════════════════"
    print_colored "$GREEN" "▶ Executing installation script with command: ${YELLOW}$command${GREEN}"
    print_colored "$CYAN" "════════════════════════════════════════════════════════════════"
    echo ""
    
    # Run the script and capture output
    "$INSTALLATION_SCRIPT" "$command"
    local exit_code=$?
    
    if [ $exit_code -eq 0 ]; then
        print_colored "$GREEN" "\n✅ Installation script executed successfully!"
    else
        print_colored "$RED" "\n❌ Installation script failed with exit code: $exit_code"
        exit $exit_code
    fi
}

# Function to perform full installation
full_install() {
    print_header
    print_colored "$GREEN" "🚀 Starting full installation of ToRouter..."
    echo ""
    
    check_root
    install_dependencies
    echo ""
    download_tarball
    echo ""
    extract_tarball
    echo ""
    cleanup_tarball
    echo ""
    execute_installation_script "start"
}

# Function to show usage
show_usage() {
    print_header
    echo -e "${GREEN}Usage:${NC} $0 [${YELLOW}start${NC}|${YELLOW}stop${NC}|${YELLOW}uninstall${NC}]"
    echo ""
    echo -e "${BLUE}Commands:${NC}"
    echo -e "  ${GREEN}(no args)${NC} - Perform full installation (download, extract, install, start)"
    echo -e "  ${GREEN}start${NC}      - Start the ToRouter service (requires existing installation)"
    echo -e "  ${YELLOW}stop${NC}       - Stop and remove the ToRouter service"
    echo -e "  ${RED}uninstall${NC}   - Stop service and completely remove application"
    echo ""
    echo -e "${MAGENTA}Examples:${NC}"
    echo -e "  ${YELLOW}sudo $0${NC}           # Full installation"
    echo -e "  ${YELLOW}sudo $0 start${NC}     # Start existing installation"
    echo -e "  ${YELLOW}sudo $0 stop${NC}      # Stop service"
    echo -e "  ${YELLOW}sudo $0 uninstall${NC} # Complete uninstall"
    echo ""
}

# Main script logic
check_root

case "$1" in
    start|stop|uninstall)
        # For existing installation, just pass through to installation script
        execute_installation_script "$1"
        ;;
    "")
        # No arguments - perform full installation
        full_install
        ;;
    *)
        show_usage
        exit 1
        ;;
esac

exit 0
