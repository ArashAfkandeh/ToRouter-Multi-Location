#!/bin/bash
export DEBIAN_FRONTEND=noninteractive

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
REPO_OWNER="ArashAfkandeh"
REPO_NAME="ToRouter-Multi-Location"
GITHUB_API="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases"
TARBALL_PATH="/root/ToRouter-Multi-Location.tar.gz"

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
    print_colored "$CYAN" "║          📦 ToRouter Installation Manager v2.3               ║"
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

# Get latest version tag
get_latest_version() {
    local tag=$(curl -s "${GITHUB_API}/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' | tr -d '[:space:]')
    echo "$tag"
}

# Get actual asset filename for download
get_asset_name() {
    local tag=$1
    local asset=$(curl -s "${GITHUB_API}/tags/${tag}" | grep -o '"name": "[^"]*tar.gz"' | head -1 | sed -E 's/.*"name": "([^"]+)".*/\1/')
    if [ -z "$asset" ]; then
        # Fallback
        asset="ToRouter-Multi-Location-v0.1.0.tar.gz"
    fi
    echo "$asset"
}

# Function to download the tarball
download_tarball() {
    local requested_version=$1
    local tag
    local asset_name
    local download_url
    
    if [ -z "$requested_version" ] || [ "$requested_version" = "latest" ]; then
        print_colored "$YELLOW" "🔍 Fetching latest release..."
        tag=$(get_latest_version)
        print_colored "$GREEN" "✓ Latest tag detected: ${tag}"
    else
        tag="$requested_version"
        print_colored "$YELLOW" "🔍 Using specified version/tag: ${tag}"
    fi
    
    asset_name=$(get_asset_name "$tag")
    download_url="https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${tag}/${asset_name}"
    
    print_colored "$YELLOW" "📥 Downloading: ${asset_name}"
    curl -L -o "$TARBALL_PATH" "$download_url" --fail --show-error
    
    if [ $? -ne 0 ] || [ ! -f "$TARBALL_PATH" ]; then
        print_colored "$RED" "✗ Error: Failed to download from ${download_url}"
        exit 1
    fi
    
    print_colored "$GREEN" "✓ Download completed successfully"
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
    print_colored "$GREEN" "✓ Tarball removed"
}

# Function to install dependencies
install_dependencies() {
    print_colored "$YELLOW" "📦 Installing dependencies..."
    apt update
    apt install -y curl
    print_colored "$GREEN" "✓ Dependencies installed successfully"
}

# Function to execute installation script
execute_installation_script() {
    local command=$1
    
    if [ ! -f "$INSTALLATION_SCRIPT" ]; then
        print_colored "$RED" "✗ Error: Installation script not found at $INSTALLATION_SCRIPT"
        exit 1
    fi
    
    chmod +x "$INSTALLATION_SCRIPT"
    print_colored "$GREEN" "✓ Installation script is now executable"
    
    echo ""
    print_colored "$CYAN" "════════════════════════════════════════════════════════════════"
    print_colored "$GREEN" "▶ Executing: ${YELLOW}$command${GREEN}"
    print_colored "$CYAN" "════════════════════════════════════════════════════════════════"
    echo ""
    
    "$INSTALLATION_SCRIPT" "$command"
    local exit_code=$?
    
    if [ $exit_code -eq 0 ]; then
        print_colored "$GREEN" "\n✅ Installation script executed successfully!"
    else
        print_colored "$RED" "\n❌ Installation script failed with exit code: $exit_code"
        exit $exit_code
    fi
}

# Full installation
full_install() {
    local version=$1
    print_header
    
    if [ -n "$version" ] && [ "$version" != "latest" ]; then
        print_colored "$GREEN" "🚀 Starting installation of ToRouter (version ${version})..."
    else
        print_colored "$GREEN" "🚀 Starting full installation of ToRouter (latest version)..."
    fi
    echo ""
    
    check_root
    install_dependencies
    echo ""
    download_tarball "$version"
    echo ""
    extract_tarball
    echo ""
    cleanup_tarball
    echo ""
    execute_installation_script "start"
}

# Update function
update_application() {
    print_header
    print_colored "$GREEN" "🔄 Starting update process for ToRouter..."
    echo ""
    
    check_root
    
    # Step 1: Download the latest version
    print_colored "$YELLOW" "📥 Step 1/5: Downloading latest version..."
    download_tarball "latest"
    echo ""
    
    # Step 2: Stop the service
    print_colored "$YELLOW" "⏹ Step 2/5: Stopping the service..."
    if [ -f "$INSTALLATION_SCRIPT" ]; then
        chmod +x "$INSTALLATION_SCRIPT"
        "$INSTALLATION_SCRIPT" "stop"
        if [ $? -ne 0 ]; then
            print_colored "$RED" "✗ Warning: Failed to stop service, continuing anyway..."
        else
            print_colored "$GREEN" "✓ Service stopped successfully"
        fi
    else
        print_colored "$YELLOW" "⚠ Installation script not found, skipping service stop"
    fi
    echo ""
    
    # Step 3: Extract and replace files
    print_colored "$YELLOW" "📂 Step 3/5: Extracting and replacing files..."
    
    # Backup old directory if exists
    if [ -d "$APP_DIR" ]; then
        print_colored "$YELLOW" "📁 Removing old installation at $APP_DIR..."
        rm -rf "$APP_DIR"
    fi
    
    extract_tarball
    echo ""
    
    # Step 4: Remove compressed file
    print_colored "$YELLOW" "🗑 Step 4/5: Removing compressed file..."
    cleanup_tarball
    echo ""
    
    # Step 5: Start the service
    print_colored "$YELLOW" "▶ Step 5/5: Starting the service..."
    if [ -f "$INSTALLATION_SCRIPT" ]; then
        chmod +x "$INSTALLATION_SCRIPT"
        "$INSTALLATION_SCRIPT" "start"
        if [ $? -eq 0 ]; then
            print_colored "$GREEN" "✓ Service started successfully"
        else
            print_colored "$RED" "✗ Error: Failed to start service"
            exit 1
        fi
    else
        print_colored "$RED" "✗ Error: Installation script not found at $INSTALLATION_SCRIPT"
        exit 1
    fi
    
    echo ""
    print_colored "$GREEN" "✅ Update completed successfully!"
}

# Show usage
show_usage() {
    print_header
    echo -e "${GREEN}Usage:${NC} $0 [${YELLOW}COMMAND${NC}]"
    echo ""
    echo -e "${CYAN}Commands:${NC}"
    echo -e "  ${GREEN}(no args)${NC}   → Install the latest version"
    echo -e "  ${GREEN}VERSION${NC}     → Install a specific version (e.g. v0.1.0)"
    echo -e "  ${GREEN}update${NC}      → Update to the latest version"
    echo -e "  ${GREEN}start${NC}       → Start the service"
    echo -e "  ${GREEN}stop${NC}        → Stop the service"
    echo -e "  ${GREEN}uninstall${NC}   → Uninstall the application"
    echo -e "  ${GREEN}help${NC}        → Show this help message"
    echo ""
    echo -e "${CYAN}Examples:${NC}"
    echo -e "  $0              # Install latest version"
    echo -e "  $0 v0.1.0       # Install version v0.1.0"
    echo -e "  $0 update       # Update to latest version"
    echo -e "  $0 start        # Start the service"
    echo -e "  $0 help         # Show this help"
    echo ""
}

# ===================== MAIN =====================

# If help is requested, show usage and exit
if [ "$1" = "help" ] || [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
    show_usage
    exit 0
fi

check_root

case "$1" in
    start|stop|uninstall)
        execute_installation_script "$1"
        ;;
    update)
        update_application
        ;;
    "")
        full_install "latest"
        ;;
    *)
        full_install "$1"
        ;;
esac

exit 0
