#!/usr/bin/env bash
export DEBIAN_FRONTEND=noninteractive
set -euo pipefail

# ─── Colors ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

# ─── Paths ───────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DAEMON_DIR="$SCRIPT_DIR/daemon"
WEB_DIR="$SCRIPT_DIR/webpanel"
ASSETS_DIR="$SCRIPT_DIR/assets"
DIST_DIR="$SCRIPT_DIR/dist"
BUILD_ROOT="/root/tor-router-build"  # پوشه‌ی ساخت در root

# ─── Defaults ────────────────────────────────────────────────────────────────
BUILD_MODE="release"
BUILD_DAEMON=true
BUILD_WEB=true
CLEAN_FIRST=false
TARGET=""
VERBOSE=false

# ─── Setup Sudo ──────────────────────────────────────────────────────────────
SUDO=""
if [ "$EUID" -ne 0 ] && command -v sudo >/dev/null 2>&1; then
    SUDO="sudo"
fi

# ─── Utilities ───────────────────────────────────────────────────────────────
log_info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
log_ok()      { echo -e "${GREEN}[OK]${RESET}    $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
log_error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; }
log_step()    { echo -e "\n${BOLD}${CYAN}▶ $*${RESET}"; }
log_section() {
    echo ""
    echo -e "${BOLD}${CYAN}╔════════════════════════════════════════════╗${RESET}"
    printf "${BOLD}${CYAN}║  %-42s ║${RESET}\n" "$*"
    echo -e "${BOLD}${CYAN}╚════════════════════════════════════════════╝${RESET}"
}

usage() {
    echo -e "${BOLD}Usage:${RESET} $0 [options]"
    echo ""
    echo "  --debug           Debug build (default: release)"
    echo "  --release         Release build"
    echo "  --clean           Clean before build"
    echo "  --daemon-only     Build Rust daemon only"
    echo "  --web-only        Build web panel only"
    echo "  --target <T>      Cross-compile (e.g., x86_64-unknown-linux-musl)"
    echo "  --verbose         Verbose cargo output"
    echo "  -h, --help        Show this help message"
    exit 0
}

# ─── Parse Arguments ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug)        BUILD_MODE="debug"  ;;
        --release)      BUILD_MODE="release" ;;
        --clean)        CLEAN_FIRST=true    ;;
        --daemon-only)  BUILD_WEB=false     ;;
        --web-only)     BUILD_DAEMON=false  ;;
        --verbose)      VERBOSE=true        ;;
        --target)       TARGET="$2"; shift  ;;
        -h|--help)      usage               ;;
        *) log_error "Unknown argument: $1"; usage ;;
    esac
    shift
done

# ─── Install Dependencies Functions ──────────────────────────────────────────

install_jq() {
    log_warn "jq not found. Installing jq..."
    if command -v apt >/dev/null 2>&1 ; then
        $SUDO apt update || true
        $SUDO apt install -y jq
    elif command -v yum >/dev/null 2>&1 ; then
        $SUDO yum install -y jq
    elif command -v dnf >/dev/null 2>&1 ; then
        $SUDO dnf install -y jq
    elif command -v apk >/dev/null 2>&1 ; then
        $SUDO apk add jq
    elif command -v pacman >/dev/null 2>&1 ; then
        $SUDO pacman -S jq
    else
        log_error "Could not detect package manager. Please install jq manually."
        exit 1
    fi
    if ! command -v jq >/dev/null 2>&1 ; then exit 1; fi
    log_ok "jq installed successfully."
}

install_rsync() {
    log_warn "rsync not found. Installing rsync..."
    if command -v apt >/dev/null 2>&1 ; then
        $SUDO apt update  || true
        $SUDO apt install -y rsync 
    elif command -v yum >/dev/null 2>&1 ; then
        $SUDO yum install -y rsync 
    elif command -v dnf >/dev/null 2>&1 ; then
        $SUDO dnf install -y rsync 
    elif command -v apk >/dev/null 2>&1 ; then
        $SUDO apk add rsync 
    elif command -v pacman >/dev/null 2>&1 ; then
        $SUDO pacman -S rsync 
    else
        log_error "Could not detect package manager. Please install rsync manually."
        exit 1
    fi
    if ! command -v rsync >/dev/null 2>&1 ; then exit 1; fi
    log_ok "rsync installed successfully."
}

install_build_essential() {
    log_warn "C compiler/linker (cc) not found. Installing build-essential..."
    if command -v apt >/dev/null 2>&1 ; then
        $SUDO apt update  || true
        $SUDO apt install -y build-essential 
    elif command -v yum >/dev/null 2>&1 ; then
        $SUDO yum groupinstall -y "Development Tools" 
    elif command -v dnf >/dev/null 2>&1 ; then
        $SUDO dnf groupinstall -y "Development Tools" 
    elif command -v apk >/dev/null 2>&1 ; then
        $SUDO apk add build-base 
    else
        log_error "Could not detect package manager."
        exit 1
    fi
    if ! command -v cc >/dev/null 2>&1 ; then exit 1; fi
    log_ok "C compiler installed successfully."
}

install_rust() {
    if command -v cargo >/dev/null 2>&1 ; then return 0; fi
    log_warn "Rust/Cargo not found. Installing Rust..."
    if command -v rustup >/dev/null 2>&1 ; then
        rustup default stable 
        rustup update 
    else
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y 
        source "$HOME/.cargo/env" 
    fi
    if ! command -v cargo >/dev/null 2>&1 ; then exit 1; fi
    log_ok "Rust/Cargo installed successfully."
}

install_nodejs() {
    log_warn "Node.js/npm not found. Installing Node.js..."
    if command -v apt >/dev/null 2>&1 ; then
        curl -fsSL https://deb.nodesource.com/setup_20.x | $SUDO bash - 
        $SUDO apt install -y nodejs 
    elif command -v yum >/dev/null 2>&1 ; then
        curl -fsSL https://rpm.nodesource.com/setup_20.x | $SUDO bash - 
        $SUDO yum install -y nodejs 
    elif command -v dnf >/dev/null 2>&1 ; then
        curl -fsSL https://rpm.nodesource.com/setup_20.x | $SUDO bash - 
        $SUDO dnf install -y nodejs 
    elif command -v apk >/dev/null 2>&1 ; then
        $SUDO apk add nodejs npm 
    else
        log_error "Could not detect package manager."
        exit 1
    fi
    if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1 ; then exit 1; fi
    log_ok "Node.js installed successfully."
}

install_tor_deps() {
    log_info "Installing dependencies for Tor binary (libevent, libssl)..."
    if command -v apt >/dev/null 2>&1 ; then
        $SUDO apt update  || true
        $SUDO apt install -y tor || true 
    elif command -v yum >/dev/null 2>&1 ; then
        $SUDO yum install -y epel-release 
        $SUDO yum install -y tor 
    elif command -v dnf >/dev/null 2>&1 ; then
        $SUDO dnf install -y tor 
    elif command -v apk >/dev/null 2>&1 ; then
        $SUDO apk add tor 
    elif command -v pacman >/dev/null 2>&1 ; then
        $SUDO pacman -S tor 
    fi
}

install_sqlite_deps() {
    log_info "Installing SQLite development dependencies (for rusqlite)..."
    if command -v apt >/dev/null 2>&1 ; then
        $SUDO apt update >/dev/null 2>&1 || true
        $SUDO apt install -y libsqlite3-dev >/dev/null 2>&1 || true
    elif command -v yum >/dev/null 2>&1 ; then
        $SUDO yum install -y sqlite-devel >/dev/null 2>&1 || true
    elif command -v dnf >/dev/null 2>&1 ; then
        $SUDO dnf install -y sqlite-devel >/dev/null 2>&1 || true
    elif command -v apk >/dev/null 2>&1 ; then
        $SUDO apk add sqlite-dev >/dev/null 2>&1 || true
    fi
}

check_tool() {
    if ! command -v "$1" >/dev/null 2>&1 ; then
        case "$1" in
            cargo|rustc) install_rust ;;
            cc|gcc)      install_build_essential ;;
            npm|node)    install_nodejs ;;
            rsync)       install_rsync ;;
            tor)         install_tor_deps ;;
            jq)          install_jq ;;
            *) log_error "Tool '$1' not found."; exit 1 ;;
        esac
    fi
}

# ─── Auto-Update Tor Assets ──────────────────────────────────────────────────
update_tor_assets() {
    log_step "Updating Tor Assets (Auto-download)"
    mkdir -p "$ASSETS_DIR"
    
    # حذف فایل tor-bin قدیمی (در صورتی که از نسخه‌های قبلی در پوشه جا مانده باشد)
    rm -f "$ASSETS_DIR/tor-bin" 2>/dev/null || true
    
    check_tool jq

    local arch
    arch=$(uname -m)
    local dl_arch=""
    case "$arch" in
        x86_64) dl_arch="linux-x86_64" ;;
        aarch64|arm64) dl_arch="linux-aarch64" ;;
        i686|i386) dl_arch="linux-i686" ;;
        *) 
            log_warn "Unsupported architecture ($arch) for auto-download. Using existing assets."
            return 0 
            ;;
    esac

    log_info "Fetching latest Tor stable version..."
    local api_url="https://aus1.torproject.org/torbrowser/update_3/release/downloads.json"
    
    set +eo pipefail
    local version
    version=$(curl -sL --connect-timeout 10 "$api_url" | jq -r '.version' 2>/dev/null)
    local curl_exit=${PIPESTATUS[0]}
    set -eo pipefail

    if [[ $curl_exit -ne 0 || -z "$version" || "$version" == "null" ]]; then
        log_warn "Failed to fetch version from Tor API. Using existing assets."
        return 0
    fi

    log_info "Latest Tor stable version: $version"
    local download_url="https://dist.torproject.org/torbrowser/${version}/tor-expert-bundle-${dl_arch}-${version}.tar.gz"
    
    local temp_dir
    temp_dir=$(mktemp -d)
    local tar_file="$temp_dir/tor-bundle.tar.gz"
    
    log_info "Downloading Tor Expert Bundle..."
    set +eo pipefail
    curl -# -L --connect-timeout 20 -o "$tar_file" "$download_url"
    local dl_status=$?
    set -eo pipefail
    
    if [[ $dl_status -ne 0 ]]; then
        log_warn "Failed to download Tor bundle. Using existing assets."
        rm -rf "$temp_dir"
        return 0
    fi

    log_info "Extracting..."
    set +eo pipefail
    tar -xzf "$tar_file" -C "$temp_dir"
    local tar_status=$?
    set -eo pipefail
    
    if [[ $tar_status -ne 0 ]]; then
        log_warn "Failed to extract Tor bundle. Using existing assets."
        rm -rf "$temp_dir"
        return 0
    fi

    log_info "Locating binaries (ignoring debug folders)..."
    local tor_bin
    tor_bin=$(find "$temp_dir" -type d -name "debug" -prune -o -type f -name "tor" -print | head -n 1)
    local geoip_file
    geoip_file=$(find "$temp_dir" -type f -name "geoip" | head -n 1)
    local geoip6_file
    geoip6_file=$(find "$temp_dir" -type f -name "geoip6" | head -n 1)

    if [[ -z "$tor_bin" || -z "$geoip_file" || -z "$geoip6_file" ]]; then
        log_warn "Required files not found in the downloaded bundle. Using existing assets."
        rm -rf "$temp_dir"
        return 0
    fi

    log_info "Replacing assets in $ASSETS_DIR..."
    cp "$tor_bin" "$ASSETS_DIR/tor"
    cp "$geoip_file" "$ASSETS_DIR/geoip"
    cp "$geoip6_file" "$ASSETS_DIR/geoip6"

    chmod +x "$ASSETS_DIR/tor"
    chmod 644 "$ASSETS_DIR/geoip" "$ASSETS_DIR/geoip6"

    rm -rf "$temp_dir"
    log_ok "Tor assets updated successfully!"
}

# ─── Setup Build Root ──────────────────────────────────────────────────────
setup_build_root() {
    log_info "Setting up build root at: $BUILD_ROOT"
    $SUDO mkdir -p "$BUILD_ROOT"
    log_info "Copying source files to build root..."
    $SUDO rsync -a --exclude='target' --exclude='node_modules' --exclude='dist' --exclude='.git' "$SCRIPT_DIR/" "$BUILD_ROOT/src/"
    $SUDO chown -R $(whoami):$(whoami) "$BUILD_ROOT" 2>/dev/null || true
    log_ok "Build root ready at: $BUILD_ROOT"
}

# ─── Start ───────────────────────────────────────────────────────────────────
log_section "Tor Router — Build System"
log_info "Mode : ${BOLD}$BUILD_MODE${RESET}  |  Root : $SCRIPT_DIR"
[[ -n "$TARGET" ]] && log_info "Target : $TARGET"
log_info "Build Root : $BUILD_ROOT"

# ─── Update Assets First ──────────────────────────────────────────────────────
update_tor_assets

# ─── Setup Build Environment ──────────────────────────────────────────────────
setup_build_root

# ─── Cleanup ─────────────────────────────────────────────────────────────────
if $CLEAN_FIRST; then
    log_step "Cleaning..."
    rm -rf "$DIST_DIR"
    $SUDO rm -rf "$BUILD_ROOT"
    log_ok "Cleaned."
    setup_build_root
fi
mkdir -p "$DIST_DIR"

# ══════════════════════════════════════════════════════════════════════════════
#  Phase 1 — Build Daemon (Rust)
# ══════════════════════════════════════════════════════════════════════════════
if $BUILD_DAEMON; then
    log_section "Phase 1 — Daemon (Rust)"
    
    check_tool cc
    check_tool cargo
    install_sqlite_deps
    check_tool tor
    
    export PATH="$HOME/.cargo/bin:$PATH"

    DAEMON_BUILD_DIR="$BUILD_ROOT/src/daemon"
    [[ ! -d "$DAEMON_BUILD_DIR" ]]        && log_error "Daemon directory not found: $DAEMON_BUILD_DIR"     && exit 1
    [[ ! -f "$DAEMON_BUILD_DIR/Cargo.toml" ]] && log_error "Cargo.toml not found."              && exit 1

    log_step "Checking assets in build root..."
    ASSETS_BUILD_DIR="$BUILD_ROOT/src/assets"
    MISSING=()
    [[ ! -f "$ASSETS_BUILD_DIR/tor" ]] && MISSING+=("assets/tor")
    [[ ! -f "$ASSETS_BUILD_DIR/geoip"   ]] && MISSING+=("assets/geoip")
    [[ ! -f "$ASSETS_BUILD_DIR/geoip6"  ]] && MISSING+=("assets/geoip6")

    if [[ ${#MISSING[@]} -gt 0 ]]; then
        log_error "The following files are missing:"
        for f in "${MISSING[@]}"; do echo -e "   ${RED}✗${RESET}  $BUILD_ROOT/src/$f"; done
        exit 1
    fi
    log_ok "All assets present."

    log_step "Compiling Rust (${BUILD_MODE})..."
    CARGO_ARGS=("build")
    [[ "$BUILD_MODE" == "release" ]] && CARGO_ARGS+=("--release")
    [[ -n "$TARGET" ]]               && CARGO_ARGS+=("--target" "$TARGET")
    $VERBOSE                         && CARGO_ARGS+=("--verbose")
    
    export RUSTFLAGS="-C target-cpu=generic"
    log_info "Using RUSTFLAGS: $RUSTFLAGS"
    
    if [[ -n "$TARGET" ]] && ! rustup target list --installed  | grep "$TARGET"; then
        log_warn "Target '$TARGET' is not installed — installing..."
        rustup target add "$TARGET" 
    fi
    
    T0=$(date +%s)
    (cd "$DAEMON_BUILD_DIR" && cargo "${CARGO_ARGS[@]}")
    log_ok "Compiled in $(($(date +%s) - T0))s."

    BIN_NAME=$(grep -m1 '^name' "$DAEMON_BUILD_DIR/Cargo.toml" | sed 's/.*= *"\(.*\)"/\1/')
    BIN_NAME="${BIN_NAME:-tor-router}"
    [[ "$TARGET" == *"windows"* ]] && BIN_NAME="${BIN_NAME}.exe"

    OUT_NAME="ToRouter"
    [[ "$TARGET" == *"windows"* ]] && OUT_NAME="${OUT_NAME}.exe"

    if [[ -n "$TARGET" ]]; then
        CARGO_OUT="$DAEMON_BUILD_DIR/target/$TARGET/$BUILD_MODE"
    else
        CARGO_OUT="$DAEMON_BUILD_DIR/target/$BUILD_MODE"
    fi

    [[ ! -f "$CARGO_OUT/$BIN_NAME" ]] && log_error "Binary not found: $CARGO_OUT/$BIN_NAME" && exit 1

    cp "$CARGO_OUT/$BIN_NAME" "$DIST_DIR/$OUT_NAME"
    chmod +x "$DIST_DIR/$OUT_NAME"
    
    log_ok "→ dist/$OUT_NAME  ($(du -sh "$DIST_DIR/$OUT_NAME" | cut -f1))"
fi

# ══════════════════════════════════════════════════════════════════════════════
#  Phase 2 — Build Web Panel
# ══════════════════════════════════════════════════════════════════════════════
if $BUILD_WEB; then
    WEB_BUILD_DIR="$BUILD_ROOT/src/webpanel"
    if [[ ! -d "$WEB_BUILD_DIR" || ! -f "$WEB_BUILD_DIR/index.html" ]]; then
        log_info "webpanel/ directory or index.html not found — skipped."
    else
        log_section "Phase 2 — Web Panel"
        log_step "Building Tailwind CSS..."
        check_tool npm
        
        if [[ -f "$WEB_BUILD_DIR/package.json" ]]; then
            (cd "$WEB_BUILD_DIR" && { npm ci || npm install; })
            (cd "$WEB_BUILD_DIR" && npm run build)
            log_ok "Vite built the web panel."
        else
            log_warn "webpanel/package.json not found — skipping build."
        fi

        log_step "Copying web panel files..."
        check_tool rsync
        rm -rf "$DIST_DIR/web"
        mkdir -p "$DIST_DIR/web"
        rsync -a "$WEB_BUILD_DIR/dist/" "$DIST_DIR/web/" 
        log_ok "→ dist/web/"
    fi
fi

# ══════════════════════════════════════════════════════════════════════════════
#  Phase 3 — Helper Files
# ══════════════════════════════════════════════════════════════════════════════
log_section "Phase 3 — Helper Files"

log_step "Copying assets..."
rm -rf "$DIST_DIR/assets"
mkdir -p "$DIST_DIR/assets"

# فقط و فقط ۳ فایلی که نام بردیم اجازه انتقال به پوشه نهایی را دارند
cp "$BUILD_ROOT/src/assets/tor" "$DIST_DIR/assets/tor"
cp "$BUILD_ROOT/src/assets/geoip" "$DIST_DIR/assets/geoip"
cp "$BUILD_ROOT/src/assets/geoip6" "$DIST_DIR/assets/geoip6"
log_ok "→ dist/assets/ (only tor, geoip, geoip6 copied)"

BIN_FINAL="ToRouter"
[[ "$TARGET" == *"windows"* ]] && BIN_FINAL="${BIN_FINAL}.exe"

# ─── run.sh ──────────────────────────────────────────────────────────────────
cat > "$DIST_DIR/run.sh" << RUNEOF
#!/usr/bin/env bash
export DEBIAN_FRONTEND=noninteractive
# Run Tor Router
DIR="\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
BIN="\$DIR/${BIN_FINAL}"

if [[ -d "\$DIR/web" ]]; then
    exec "\$BIN" --web-dir "\$DIR/web"
else
    exec "\$BIN" --run
fi
RUNEOF
chmod +x "$DIST_DIR/run.sh"

[[ -f "$SCRIPT_DIR/README.md" ]] && cp "$SCRIPT_DIR/README.md" "$DIST_DIR/"

# ─── Summary ─────────────────────────────────────────────────────────────────
log_section "Build Result"
echo -e "${GREEN}✅ Build successful!${RESET}\n"
echo -e "📦 Contents of dist/:"
ls -lh "$DIST_DIR"
echo ""
echo -e "${BOLD}${CYAN}Run:${RESET}"
echo -e "  ${CYAN}cd $DIST_DIR && ./run.sh${RESET}"
echo ""
