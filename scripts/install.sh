#!/usr/bin/env bash
set -euo pipefail

# Shipyard PaaS Installer
# Usage: curl -sSL https://get.shipyard.dev | bash
#        or: sudo bash scripts/install.sh
#
# Requirements: Docker 24+, PostgreSQL 15+, Mosquitto 2+, openssl

SHIPYARD_VERSION="${SHIPYARD_VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-/opt/shipyard}"
DATA_DIR="${DATA_DIR:-/opt/shipyard/data}"
CONFIG_DIR="${CONFIG_DIR:-/opt/shipyard/config}"
BINARY_RELEASE_URL="https://github.com/shipyard/shipyard/releases/${SHIPYARD_VERSION}/download"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

echo ""
echo "  Shipyard PaaS Installer"
echo "  ========================"
echo ""

# ── Privilege check ──────────────────────────────────────────────────────────
if [[ $EUID -ne 0 ]]; then
    error "This installer must be run as root. Try: sudo bash install.sh"
fi

# ── Detect OS ────────────────────────────────────────────────────────────────
OS=""
if [[ -f /etc/os-release ]]; then
    # shellcheck source=/dev/null
    source /etc/os-release
    OS="${ID:-unknown}"
fi
info "OS: ${OS:-unknown}"

# ── Check prerequisites ──────────────────────────────────────────────────────
info "Checking prerequisites..."
command -v docker  &>/dev/null || error "Docker is not installed. Install Docker 24+ first."
command -v openssl &>/dev/null || error "openssl is required for secret generation."
command -v psql    &>/dev/null || warn  "psql not found — PostgreSQL setup will be skipped."
command -v redis-cli &>/dev/null && HAS_REDIS=true || HAS_REDIS=false

DOCKER_VERSION=$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo "unknown")
info "Docker version: ${DOCKER_VERSION}"

# ── Initialize Docker Swarm if needed ────────────────────────────────────────
if ! docker info --format '{{.Swarm.LocalNodeState}}' 2>/dev/null | grep -q "active"; then
    warn "Docker Swarm is not active — initializing..."
    ADVERTISE_ADDR=$(hostname -I 2>/dev/null | awk '{print $1}' || echo "127.0.0.1")
    docker swarm init --advertise-addr "${ADVERTISE_ADDR}" \
        || error "Failed to initialize Docker Swarm."
    success "Docker Swarm initialized (manager: ${ADVERTISE_ADDR})"
else
    success "Docker Swarm is active"
fi

# ── Create directories ───────────────────────────────────────────────────────
info "Creating directories..."
mkdir -p "${INSTALL_DIR}/bin" "${DATA_DIR}" "${CONFIG_DIR}" "${INSTALL_DIR}/backups"
chmod 750 "${INSTALL_DIR}" "${CONFIG_DIR}"

# ── Generate secrets ─────────────────────────────────────────────────────────
info "Generating secrets..."
JWT_SECRET=$(openssl rand -hex 32)
SECRET_KEY=$(openssl rand -hex 32)
DB_PASSWORD=$(openssl rand -hex 16)

# ── Write config ─────────────────────────────────────────────────────────────
info "Writing ${CONFIG_DIR}/config.toml ..."
cat > "${CONFIG_DIR}/config.toml" <<TOML
[server]
host = "0.0.0.0"
port = 3001

[database]
url = "postgres://shipyard:${DB_PASSWORD}@localhost:5432/shipyard"
max_connections = 10

[auth]
jwt_secret = "${JWT_SECRET}"
access_token_expiry = 3600
refresh_token_expiry = 604800
secret_key = "${SECRET_KEY}"

[mqtt]
host = "localhost"
port = 1883
client_id = "shipyard-api"

[docker]
label_prefix = "platform"

[traefik]
network = "platform_proxy"
entrypoint_http = "web"
entrypoint_https = "websecure"
cert_resolver = "letsencrypt"

[smtp]
enabled = false
host = "smtp.example.com"
port = 587
username = ""
password = ""
from_address = "noreply@example.com"
from_name = "Shipyard"

[tls]
enabled = false
cert_path = "/etc/shipyard/tls/cert.pem"
key_path  = "/etc/shipyard/tls/key.pem"

[redis]
url = "redis://localhost:6379"

data_dir = "${DATA_DIR}"
TOML
chmod 600 "${CONFIG_DIR}/config.toml"
success "Config written"

# ── Set up PostgreSQL ────────────────────────────────────────────────────────
if command -v psql &>/dev/null; then
    info "Setting up PostgreSQL..."
    sudo -u postgres psql -c "CREATE USER shipyard WITH PASSWORD '${DB_PASSWORD}';" 2>/dev/null \
        || warn "User 'shipyard' may already exist"
    sudo -u postgres psql -c "CREATE DATABASE shipyard OWNER shipyard;" 2>/dev/null \
        || warn "Database 'shipyard' may already exist"
    sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE shipyard TO shipyard;" 2>/dev/null \
        || true
    success "PostgreSQL ready (password saved in config)"
else
    warn "Create the database manually:"
    warn "  CREATE USER shipyard WITH PASSWORD '${DB_PASSWORD}';"
    warn "  CREATE DATABASE shipyard OWNER shipyard;"
fi

# ── Install / verify Redis ───────────────────────────────────────────────────
info "Setting up Redis..."
if $HAS_REDIS; then
    success "Redis is already installed ($(redis-cli --version))"
else
    case "${OS}" in
        ubuntu|debian)
            apt-get install -y redis-server
            systemctl enable redis-server
            systemctl start  redis-server
            success "Redis installed via apt"
            ;;
        centos|rhel|fedora|rocky|almalinux)
            dnf install -y redis 2>/dev/null || yum install -y redis
            systemctl enable redis
            systemctl start  redis
            success "Redis installed via dnf/yum"
            ;;
        *)
            warn "Could not install Redis automatically on OS '${OS}'."
            warn "Install Redis manually and ensure it listens on localhost:6379."
            warn "Remove the [redis] section from config.toml to disable caching."
            ;;
    esac
fi

# ── Docker overlay network ───────────────────────────────────────────────────
info "Creating Docker overlay network 'platform_proxy'..."
docker network create --driver overlay --attachable platform_proxy 2>/dev/null \
    || info "Network 'platform_proxy' already exists"

# ── Install binary ───────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" 2>/dev/null && pwd || pwd)"
WORKSPACE_CARGO="${SCRIPT_DIR}/../backend/Cargo.toml"

if command -v cargo &>/dev/null && [[ -f "${WORKSPACE_CARGO}" ]]; then
    info "Building from source (this may take several minutes)..."
    cargo build --release --manifest-path "${WORKSPACE_CARGO}" 2>&1 | tail -5
    cp "$(dirname "${WORKSPACE_CARGO}")/target/release/shipyard" "${INSTALL_DIR}/bin/shipyard"
    chmod +x "${INSTALL_DIR}/bin/shipyard"
    success "Binary built and installed"
else
    info "Downloading pre-built binary (${SHIPYARD_VERSION})..."
    ARCH=$(uname -m)
    case "${ARCH}" in
        x86_64)  RUST_TARGET="x86_64-unknown-linux-gnu" ;;
        aarch64) RUST_TARGET="aarch64-unknown-linux-gnu" ;;
        *)        error "Unsupported architecture: ${ARCH}" ;;
    esac
    curl -sSfL "${BINARY_RELEASE_URL}/shipyard-${RUST_TARGET}" \
        -o "${INSTALL_DIR}/bin/shipyard" \
        || error "Failed to download binary"
    chmod +x "${INSTALL_DIR}/bin/shipyard"
    success "Binary downloaded"
fi

# ── systemd service ──────────────────────────────────────────────────────────
if command -v systemctl &>/dev/null; then
    info "Installing systemd service..."
    cat > /etc/systemd/system/shipyard.service <<UNIT
[Unit]
Description=Shipyard PaaS Platform
After=network.target postgresql.service mosquitto.service redis.service redis-server.service
Wants=postgresql.service redis.service redis-server.service

[Service]
Type=simple
User=root
WorkingDirectory=${CONFIG_DIR}
ExecStart=${INSTALL_DIR}/bin/shipyard
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=shipyard
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
UNIT
    systemctl daemon-reload
    systemctl enable shipyard
    success "systemd service installed and enabled"
fi

# ── Backup script and daily cron ─────────────────────────────────────────────
if [[ -f "${SCRIPT_DIR}/backup.sh" ]]; then
    cp "${SCRIPT_DIR}/backup.sh" "${INSTALL_DIR}/bin/shipyard-backup"
    chmod +x "${INSTALL_DIR}/bin/shipyard-backup"

    if [[ -d /etc/cron.d ]]; then
        printf '0 2 * * * root DATABASE_URL=postgres://shipyard:%s@localhost:5432/shipyard %s/bin/shipyard-backup >> /var/log/shipyard-backup.log 2>&1\n' \
            "${DB_PASSWORD}" "${INSTALL_DIR}" > /etc/cron.d/shipyard-backup
        success "Daily backup cron installed (02:00 UTC)"
    fi
fi

# ── Done ─────────────────────────────────────────────────────────────────────
LISTEN_IP=$(hostname -I 2>/dev/null | awk '{print $1}' || echo "localhost")
echo ""
success "Installation complete!"
echo ""
echo "  Next steps:"
echo "  1. Start:         systemctl start shipyard"
echo "  2. Setup wizard:  http://${LISTEN_IP}:3001"
echo "  3. Create your admin account and first organization"
echo ""
echo "  Config:  ${CONFIG_DIR}/config.toml"
echo "  Data:    ${DATA_DIR}"
echo "  Logs:    journalctl -u shipyard -f"
echo "  Backup:  ${INSTALL_DIR}/bin/shipyard-backup"
echo ""
