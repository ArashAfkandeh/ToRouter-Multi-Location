#!/bin/bash

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Variables
APP_DIR="/opt/ToRouter-Multi-Location"
INSTALLATION_SCRIPT="${APP_DIR}/installation.sh"
GITHUB_REPO="ArashAfkandeh/ToRouter-Multi-Location"
BASE_URL="https://github.com/${GITHUB_REPO}/releases"
LATEST_RELEASE_URL="${BASE_URL}/latest"
TAR_FILE="/root/ToRouter-Multi-Location.tar.gz"

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
    print_colored "$CYAN" "║          📦 ToRouter Multi-Location Installer                 ║"
    print_colored "$CYAN" "╚═══════════════════════════════════════════════════════════════╝"
    echo ""
}

# Function to check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        print_colored "$RED" "✗ This script must be run as root (use sudo)"
        exit 1
    fi
}

# Function to update system and install curl
install_dependencies() {
    print_colored "$YELLOW" "📦 Updating package list..."
    apt update -qq
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Failed to update package list"
        exit 1
    fi
    print_colored "$GREEN" "✓ Package list updated successfully"
    
    # Check if curl is installed
    if ! command -v curl &> /dev/null; then
        print_colored "$YELLOW" "📦 Installing curl..."
        apt install -y curl -qq
        if [ $? -ne 0 ]; then
            print_colored "$RED" "✗ Failed to install curl"
            exit 1
        fi
        print_colored "$GREEN" "✓ Curl installed successfully"
    else
        print_colored "$GREEN" "✓ Curl is already installed"
    fi
    
    # Check if jq is installed (for JSON parsing)
    if ! command -v jq &> /dev/null; then
        print_colored "$YELLOW" "📦 Installing jq (for JSON parsing)..."
        apt install -y jq -qq
        if [ $? -ne 0 ]; then
            print_colored "$RED" "✗ Failed to install jq"
            exit 1
        fi
        print_colored "$GREEN" "✓ Jq installed successfully"
    else
        print_colored "$GREEN" "✓ Jq is already installed"
    fi
}

# Function to get the latest release version
get_latest_version() {
    print_colored "$BLUE" "🔍 Checking for latest release..."
    
    # Get latest release info from GitHub API
    local api_url="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
    local response=$(curl -s "$api_url")
    
    # Check if API request was successful
    if [ $? -ne 0 ] || [ -z "$response" ]; then
        print_colored "$RED" "✗ Failed to get latest release information"
        return 1
    fi
    
    # Extract tag name using jq
    local version=$(echo "$response" | jq -r '.tag_name')
    
    if [ -z "$version" ] || [ "$version" = "null" ]; then
        print_colored "$RED" "✗ Failed to parse version from API response"
        return 1
    fi
    
    echo "$version"
    return 0
}

# Function to get download URL for a specific version
get_download_url() {
    local version=$1
    
    # If version is "latest", get the latest release
    if [ "$version" = "latest" ]; then
        local api_url="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
        local response=$(curl -s "$api_url")
        
        # Extract the first asset URL
        local download_url=$(echo "$response" | jq -r '.assets[0].browser_download_url')
        
        if [ -z "$download_url" ] || [ "$download_url" = "null" ]; then
            print_colored "$RED" "✗ Failed to get download URL for latest release"
            return 1
        fi
        
        echo "$download_url"
        return 0
    else
        # For specific version, use the releases API
        local api_url="https://api.github.com/repos/${GITHUB_REPO}/releases/tags/${version}"
        local response=$(curl -s "$api_url")
        
        # Extract the first asset URL
        local download_url=$(echo "$response" | jq -r '.assets[0].browser_download_url')
        
        if [ -z "$download_url" ] || [ "$download_url" = "null" ]; then
            print_colored "$RED" "✗ Failed to get download URL for version: $version"
            return 1
        fi
        
        echo "$download_url"
        return 0
    fi
}

# Function to download the package
download_package() {
    local version=$1
    local download_url
    
    if [ -z "$version" ] || [ "$version" = "latest" ]; then
        print_colored "$YELLOW" "📥 Downloading latest version..."
        download_url=$(get_download_url "latest")
        if [ $? -ne 0 ]; then
            return 1
        fi
    else
        print_colored "$YELLOW" "📥 Downloading version: $version..."
        download_url=$(get_download_url "$version")
        if [ $? -ne 0 ]; then
            return 1
        fi
    fi
    
    print_colored "$BLUE" "   URL: $download_url"
    print_colored "$BLUE" "   Destination: $TAR_FILE"
    
    curl -L -o "$TAR_FILE" "$download_url" --progress-bar
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Failed to download package"
        return 1
    fi
    
    print_colored "$GREEN" "✓ Package downloaded successfully"
    return 0
}

# Function to extract package
extract_package() {
    print_colored "$YELLOW" "📂 Extracting package to /opt..."
    
    # Check if file exists
    if [ ! -f "$TAR_FILE" ]; then
        print_colored "$RED" "✗ Package file not found: $TAR_FILE"
        return 1
    fi
    
    # Remove existing directory if it exists
    if [ -d "$APP_DIR" ]; then
        print_colored "$YELLOW" "⚠️  Removing existing installation..."
        rm -rf "$APP_DIR"
    fi
    
    # Extract the package
    tar -xzf "$TAR_FILE" -C /opt
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Failed to extract package"
        return 1
    fi
    
    # Remove the tar file after extraction
    rm -f "$TAR_FILE"
    print_colored "$GREEN" "✓ Package extracted successfully to $APP_DIR"
    return 0
}

# Function to run installation script
run_installation_script() {
    local action=$1
    
    # Check if installation script exists
    if [ ! -f "$INSTALLATION_SCRIPT" ]; then
        print_colored "$RED" "✗ Installation script not found: $INSTALLATION_SCRIPT"
        return 1
    fi
    
    # Make the script executable
    print_colored "$YELLOW" "🔧 Setting executable permission on installation script..."
    chmod +x "$INSTALLATION_SCRIPT"
    print_colored "$GREEN" "✓ Permission set successfully"
    
    # Run the installation script with the action
    print_colored "$CYAN" "═══════════════════════════════════════════════════════════════"
    print_colored "$YELLOW" "▶ Running installation script with action: $action"
    print_colored "$CYAN" "═══════════════════════════════════════════════════════════════"
    echo ""
    
    "$INSTALLATION_SCRIPT" "$action"
    local exit_code=$?
    
    if [ $exit_code -ne 0 ]; then
        print_colored "$RED" "✗ Installation script failed with action: $action (exit code: $exit_code)"
        return 1
    fi
    
    return 0
}

# Function for full installation
full_install() {
    local version=$1
    
    print_header
    print_colored "$GREEN" "🚀 Starting full installation process..."
    echo ""
    
    # Step 1: Check root
    check_root
    
    # Step 2: Install dependencies
    install_dependencies
    
    # Step 3: Get version if not specified
    if [ -z "$version" ]; then
        version=$(get_latest_version)
        if [ $? -ne 0 ]; then
            print_colored "$RED" "✗ Failed to get latest version"
            exit 1
        fi
        print_colored "$GREEN" "✓ Latest version detected: $version"
    else
        print_colored "$GREEN" "✓ Using specified version: $version"
    fi
    
    # Step 4: Download package
    download_package "$version"
    if [ $? -ne 0 ]; then
        exit 1
    fi
    
    # Step 5: Extract package
    extract_package
    if [ $? -ne 0 ]; then
        exit 1
    fi
    
    # Step 6: Run installation script with 'start'
    run_installation_script "start"
    if [ $? -ne 0 ]; then
        exit 1
    fi
    
    print_colored "$GREEN" "\n✅ Full installation completed successfully!"
}

# Function to forward to installation script
forward_to_install_script() {
    local action=$1
    
    print_header
    print_colored "$YELLOW" "▶ Forwarding to installation script with action: $action"
    echo ""
    
    # Check root
    check_root
    
    # Check if installation script exists
    if [ ! -f "$INSTALLATION_SCRIPT" ]; then
        print_colored "$RED" "✗ Installation script not found at: $INSTALLATION_SCRIPT"
        print_colored "$YELLOW" "ℹ Please run the script without arguments first to install ToRouter"
        exit 1
    fi
    
    # Run installation script with the action
    run_installation_script "$action"
    if [ $? -ne 0 ]; then
        exit 1
    fi
}

# Function to show usage
show_usage() {
    print_header
    echo -e "${GREEN}Usage:${NC} $0 [${YELLOW}OPTIONS${NC}] [${YELLOW}COMMAND${NC}]"
    echo ""
    echo -e "${BLUE}Commands:${NC}"
    echo -e "  ${GREEN}(no args)${NC}  - Full installation: update, install dependencies,"
    echo -e "                   download latest version, extract, and start the service"
    echo -e "  ${GREEN}start${NC}     - Forward to installation script with 'start' action"
    echo -e "  ${YELLOW}stop${NC}      - Forward to installation script with 'stop' action"
    echo -e "  ${RED}uninstall${NC}  - Forward to installation script with 'uninstall' action"
    echo ""
    echo -e "${MAGENTA}Options:${NC}"
    echo -e "  ${CYAN}--version VERSION${NC}  - Install a specific version (e.g., v0.1.0)"
    echo -e "  ${CYAN}--help${NC}              - Show this help message"
    echo ""
    echo -e "${MAGENTA}Examples:${NC}"
    echo -e "  ${YELLOW}sudo $0${NC}                           # Install latest version"
    echo -e "  ${YELLOW}sudo $0 --version v0.1.0${NC}         # Install specific version"
    echo -e "  ${YELLOW}sudo $0 start${NC}                     # Start existing installation"
    echo -e "  ${YELLOW}sudo $0 stop${NC}                      # Stop service"
    echo -e "  ${YELLOW}sudo $0 uninstall${NC}                 # Complete removal"
    echo ""
}

# Parse arguments
VERSION=""
ACTION=""

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version|-v)
            if [[ -z "$2" ]] || [[ "$2" == -* ]]; then
                print_colored "$RED" "✗ Error: --version requires a value"
                exit 1
            fi
            VERSION="$2"
            shift 2
            ;;
        --help|-h)
            show_usage
            exit 0
            ;;
        start|stop|uninstall)
            if [ -n "$ACTION" ]; then
                print_colored "$RED" "✗ Error: Multiple commands specified"
                exit 1
            fi
            ACTION="$1"
            shift
            ;;
        *)
            print_colored "$RED" "✗ Unknown argument: $1"
            echo ""
            show_usage
            exit 1
            ;;
    esac
done

# Main script logic
if [ -n "$ACTION" ]; then
    # If action is specified (start, stop, uninstall)
    forward_to_install_script "$ACTION"
else
    # Full installation with optional version
    full_install "$VERSION"
fi

exit 0
