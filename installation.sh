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

SERVICE_NAME="ToRouter"
SERVICE_FILE="/opt/ToRouter-Multi-Location/dist/ToRouter.service"
SERVICE_DEST="/etc/systemd/system/ToRouter.service"
APP_DIR="/opt/ToRouter-Multi-Location"

# Function to print colored commands
print_commands() {
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}▶ To restart the service:${NC}"
    echo -e "  ${YELLOW}sudo systemctl restart ${SERVICE_NAME}.service${NC}"
    echo ""
    echo -e "${BLUE}▶ To check service status:${NC}"
    echo -e "  ${YELLOW}sudo systemctl status ${SERVICE_NAME}.service${NC}"
    echo ""
    echo -e "${GREEN}▶ To view real-time logs:${NC}"
    echo -e "  ${YELLOW}sudo journalctl -u ${SERVICE_NAME}.service -f${NC}"
}

# Function to print colored output
print_colored() {
    local color=$1
    local message=$2
    echo -e "${color}${message}${NC}"
}

# Function to check if service is running
is_service_active() {
    systemctl is-active ${SERVICE_NAME}.service
    return $?
}

# Function to check if service file exists
check_service_file() {
    if [ ! -f "$SERVICE_FILE" ]; then
        print_colored "$RED" "✗ Error: Service file not found at $SERVICE_FILE"
        exit 1
    fi
}

# Function to check and configure web panel port
configure_web_port() {
    local DEFAULT_PORT=3000
    local DB_PATH="/opt/ToRouter-Multi-Location/dist/ToRouter.sqlite"
    local CURRENT_PORT=$DEFAULT_PORT

    # Get current port from DB if exists
    if [ -f "$DB_PATH" ]; then
        if command -v sqlite3 >/dev/null 2>&1; then
            local db_port=$(sqlite3 "$DB_PATH" "SELECT value FROM settings WHERE key='web_panel_port';" 2>/dev/null)
            if [ -n "$db_port" ]; then
                CURRENT_PORT=$db_port
            fi
        fi
    fi

    local PORT=$CURRENT_PORT
    
    echo ""
    print_colored "$YELLOW" "🔍 Checking web panel port ($PORT)..."
    
    while true; do
        # Professional check if port is in use (more precise matching)
        if ss -tuln | awk '{print $5}' | grep -E -q ":${PORT}$"; then
            print_colored "$RED" "⚠️  Port ${PORT} is currently in use by another application."
            
            # Interactive prompt for new port
            read -p "$(echo -e "${CYAN}👉 Please enter a new free port for the web panel [1024-65535]: ${NC}")" NEW_PORT
            
            # Enhanced validation
            if [[ -z "$NEW_PORT" ]]; then
                print_colored "$RED" "❌ Port cannot be empty. Please try again."
                continue
            fi
            
            if ! [[ "$NEW_PORT" =~ ^[0-9]+$ ]]; then
                print_colored "$RED" "❌ Invalid input. Please enter a numerical value."
                continue
            fi
            
            if [ "$NEW_PORT" -lt 1 ] || [ "$NEW_PORT" -gt 65535 ]; then
                print_colored "$RED" "❌ Port out of range. Must be between 1 and 65535."
                continue
            fi
            
            if [ "$NEW_PORT" -lt 1024 ]; then
                print_colored "$YELLOW" "⚠️  Warning: Ports below 1024 are privileged."
            fi
            
            PORT=$NEW_PORT
            print_colored "$YELLOW" "🔍 Checking new port ($PORT)..."
        else
            print_colored "$GREEN" "✓ Port $PORT is free and ready to use."
            break
        fi
    done

    # If the port changed or DB doesn't exist, we must update/create it
    if [ "$PORT" != "$CURRENT_PORT" ] || [ ! -f "$DB_PATH" ]; then
        if ! command -v sqlite3 >/dev/null 2>&1; then
            print_colored "$YELLOW" "📦 Installing sqlite3 for database configuration..."
            sudo apt-get update >/dev/null 2>&1
            sudo apt-get install -y sqlite3 >/dev/null 2>&1
        fi
        
        # Create DB directory if not exists
        mkdir -p "$(dirname "$DB_PATH")"
        
        # Set port in DB
        sqlite3 "$DB_PATH" <<EOF
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT OR REPLACE INTO settings (key, value) VALUES ('web_panel_port', '$PORT');
INSERT OR REPLACE INTO settings (key, value) VALUES ('api_port', '$PORT');
EOF
        print_colored "$GREEN" "✓ Web panel port set to $PORT in database."
    fi
    
    # Configure UFW
    if command -v ufw >/dev/null 2>&1; then
        local ufw_status=$(sudo ufw status | grep -i "Status: active")
        if [ -n "$ufw_status" ]; then
            print_colored "$YELLOW" "🔐 Configuring UFW firewall for port $PORT..."
            sudo ufw allow $PORT/tcp >/dev/null 2>&1
            print_colored "$GREEN" "✓ Port $PORT allowed in UFW"
        fi
    fi
}

# Function to start the service
start_service() {
    clear
    print_colored "$CYAN" "╔═══════════════════════════════════════════════════════════════╗"
    print_colored "$CYAN" "║          🚀 Starting ToRouter Service                       ║"
    print_colored "$CYAN" "╚═══════════════════════════════════════════════════════════════╝"
    echo ""
    
    # Configure Web Port
    configure_web_port
    
    # Install required packages
    print_colored "$YELLOW" "📦 Installing required packages (libssl-dev, libevent-dev)..."
    sudo apt-get update
    sudo apt-get install -y libssl-dev libevent-dev
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Error: Failed to install required packages"
        exit 1
    fi
    print_colored "$GREEN" "✓ Required packages installed successfully"
    echo ""
    
    # Check if service file exists
    check_service_file
    
    # Copy service file
    print_colored "$YELLOW" "📁 Copying service file to /etc/systemd/system/..."
    sudo cp "$SERVICE_FILE" "$SERVICE_DEST"
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Error: Failed to copy service file"
        exit 1
    fi
    print_colored "$GREEN" "✓ Service file copied successfully"
    
    # Reload systemd
    print_colored "$YELLOW" "🔄 Reloading systemd..."
    sudo systemctl daemon-reload
    print_colored "$GREEN" "✓ Systemd reloaded successfully"
    
    # Enable service
    print_colored "$YELLOW" "🔗 Enabling service (auto-start on boot)..."
    sudo systemctl enable ${SERVICE_NAME}.service
    print_colored "$GREEN" "✓ Service enabled successfully"
    
    # Start service
    print_colored "$YELLOW" "▶ Starting service..."
    sudo systemctl start ${SERVICE_NAME}.service
    if [ $? -ne 0 ]; then
        print_colored "$RED" "✗ Error: Failed to start service"
        print_colored "$YELLOW" "ℹ Check logs for more details: sudo journalctl -u ${SERVICE_NAME}.service -n 20"
        exit 1
    fi
    print_colored "$GREEN" "✓ Service started successfully"
    
    # Show status
    echo ""
    print_colored "$CYAN" "╔═══════════════════════════════════════════════════════════════╗"
    print_colored "$CYAN" "║          📊 Service Status                                  ║"
    print_colored "$CYAN" "╚═══════════════════════════════════════════════════════════════╝"
    sudo systemctl status ${SERVICE_NAME}.service --no-pager
    
    # Create symlink for CLI tool
    echo ""
    print_colored "$YELLOW" "🔗 Creating symlink for CLI tool..."
    local SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
    local BINARY_PATH="${SCRIPT_DIR}/dist/ToRouter"
    
    if [ -f "$BINARY_PATH" ]; then
        sudo ln -sf "$BINARY_PATH" /usr/local/bin/tor-p
        print_colored "$GREEN" "✓ Symlink created successfully: tor-p -> $BINARY_PATH"
    else
        print_colored "$YELLOW" "⚠️  Binary not found at $BINARY_PATH. CLI symlink not created."
    fi
    
    # Show useful commands
    print_commands
    
    print_colored "$GREEN" "\n✅ Service installation and startup completed successfully!"
}

# Function to show usage
show_usage() {
    echo -e "${CYAN}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║          📦 ToRouter Installation Manager                    ║${NC}"
    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${GREEN}Usage:${NC} $0 {${YELLOW}start${NC}}"
    echo ""
    echo -e "${BLUE}Commands:${NC}"
    echo -e "  ${GREEN}start${NC}     - Install and start the ToRouter service"
    echo ""
    echo -e "${MAGENTA}Example:${NC}"
    echo -e "  ${YELLOW}sudo $0 start${NC}"
    echo ""
}

# Main script logic
case "$1" in
    start)
        start_service
        ;;
    *)
        show_usage
        exit 1
        ;;
esac

exit 0
