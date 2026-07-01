#!/usr/bin/env bash
set -euo pipefail

# ── Colors ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${BLUE}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }
step()    { echo -e "\n${BOLD}── $* ──${RESET}"; }

read_input() {
    local prompt="$1"
    local var_name="$2"
    local default_val="${3:-}"
    
    # If already set in environment, use it
    eval "local current_val=\${${var_name}:-}"
    if [[ -n "${current_val}" ]]; then
        return 0
    fi
    
    local val=""
    if [[ -t 0 ]]; then
        read -rp "${prompt}" val
    elif [[ -c /dev/tty ]]; then
        read -rp "${prompt}" val < /dev/tty
    else
        val="${default_val}"
    fi
    
    if [[ -z "${val}" ]]; then
        val="${default_val}"
    fi
    
    eval "${var_name}=\${val}"
}

INSTALL_DIR="/opt/shipyard"
BACKEND_IMAGE="triandamai827/shipyard-backend"
FRONTEND_IMAGE="triandamai827/shipyard-frontend"

# ── Banner ────────────────────────────────────────────────────────────────────
echo -e "${BOLD}"
echo "  ███████╗██╗  ██╗██╗██████╗ ██╗   ██╗ █████╗ ██████╗ ██████╗ "
echo "  ██╔════╝██║  ██║██║██╔══██╗╚██╗ ██╔╝██╔══██╗██╔══██╗██╔══██╗"
echo "  ███████╗███████║██║██████╔╝ ╚████╔╝ ███████║██████╔╝██║  ██║"
echo "  ╚════██║██╔══██║██║██╔═══╝   ╚██╔╝  ██╔══██║██╔══██╗██║  ██║"
echo "  ███████║██║  ██║██║██║        ██║   ██║  ██║██║  ██║██████╔╝"
echo "  ╚══════╝╚═╝  ╚═╝╚═╝╚═╝        ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝ "
echo -e "${RESET}"
echo -e "${BOLD}  Self-hosted container orchestration platform${RESET}"
echo ""

# ── Root check ────────────────────────────────────────────────────────────────
if [[ $EUID -ne 0 ]]; then
    error "Please run as root: sudo bash install.sh"
fi

# ── Detect OS ─────────────────────────────────────────────────────────────────
OS_ID=""
OS_VERSION_ID=""
if [[ -f /etc/os-release ]]; then
    # shellcheck source=/dev/null
    source /etc/os-release
    OS_ID="${ID:-}"
    OS_VERSION_ID="${VERSION_ID:-}"
fi
info "OS: ${OS_ID:-unknown} ${OS_VERSION_ID}"

# ── Package installer helper ──────────────────────────────────────────────────
install_pkg() {
    case "${OS_ID}" in
        ubuntu|debian|linuxmint|pop)
            apt-get install -y -q "$@" ;;
        centos|rhel|rocky|almalinux|ol)
            dnf install -y "$@" 2>/dev/null || yum install -y "$@" ;;
        fedora)
            dnf install -y "$@" ;;
        arch|manjaro)
            pacman -Sy --noconfirm "$@" ;;
        *)
            error "Unsupported OS '${OS_ID}'. Install $* manually and re-run." ;;
    esac
}

# ── Base utilities ────────────────────────────────────────────────────────────
step "Checking base utilities"

if ! command -v curl &>/dev/null; then
    info "Installing curl..."
    install_pkg curl
fi

if ! command -v openssl &>/dev/null; then
    info "Installing openssl..."
    install_pkg openssl
fi

success "Base utilities OK"

# ── Docker Engine ─────────────────────────────────────────────────────────────
step "Checking Docker"

install_docker() {
    info "Docker not found — installing via official script..."
    case "${OS_ID}" in
        ubuntu|debian|linuxmint|pop|centos|rhel|rocky|almalinux|ol|fedora)
            curl -fsSL https://get.docker.com | sh
            ;;
        arch|manjaro)
            install_pkg docker
            ;;
        *)
            error "Cannot auto-install Docker on '${OS_ID}'. Install Docker 24+ manually: https://docs.docker.com/engine/install/"
            ;;
    esac
    systemctl enable --now docker
    success "Docker installed and started"
}

if ! command -v docker &>/dev/null; then
    install_docker
else
    DOCKER_VER=$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo "0.0.0")
    DOCKER_MAJOR=$(echo "${DOCKER_VER}" | cut -d. -f1)
    if [[ "${DOCKER_MAJOR}" -lt 24 ]]; then
        warn "Docker v${DOCKER_VER} is older than the recommended minimum (24)."
        warn "Upgrading: https://docs.docker.com/engine/install/"
    else
        success "Docker v${DOCKER_VER}"
    fi
fi

# ── Docker Compose plugin ─────────────────────────────────────────────────────
if ! docker compose version &>/dev/null; then
    info "Docker Compose plugin not found — installing..."
    COMPOSE_VERSION="v2.27.1"
    COMPOSE_DIR="/usr/local/lib/docker/cli-plugins"
    mkdir -p "${COMPOSE_DIR}"

    ARCH=$(uname -m)
    case "${ARCH}" in
        x86_64)  COMPOSE_ARCH="x86_64" ;;
        aarch64) COMPOSE_ARCH="aarch64" ;;
        armv7l)  COMPOSE_ARCH="armv7" ;;
        *)        error "Unsupported architecture for Docker Compose: ${ARCH}" ;;
    esac

    curl -fsSL \
        "https://github.com/docker/compose/releases/download/${COMPOSE_VERSION}/docker-compose-linux-${COMPOSE_ARCH}" \
        -o "${COMPOSE_DIR}/docker-compose"
    chmod +x "${COMPOSE_DIR}/docker-compose"
    success "Docker Compose ${COMPOSE_VERSION} installed"
else
    success "Docker Compose $(docker compose version --short 2>/dev/null || echo 'ok')"
fi

# ── Docker Buildx plugin ──────────────────────────────────────────────────────
if ! docker buildx version &>/dev/null; then
    info "Docker Buildx plugin not found — installing..."
    BUILDX_VERSION="v0.14.1"
    BUILDX_DIR="/usr/local/lib/docker/cli-plugins"
    mkdir -p "${BUILDX_DIR}"

    ARCH=$(uname -m)
    case "${ARCH}" in
        x86_64)  BUILDX_ARCH="amd64" ;;
        aarch64) BUILDX_ARCH="arm64" ;;
        armv7l)  BUILDX_ARCH="arm-v7" ;;
        *)        warn "Buildx not available for ${ARCH} — skipping"; BUILDX_ARCH="" ;;
    esac

    if [[ -n "${BUILDX_ARCH}" ]]; then
        curl -fsSL \
            "https://github.com/docker/buildx/releases/download/${BUILDX_VERSION}/buildx-${BUILDX_VERSION}.linux-${BUILDX_ARCH}" \
            -o "${BUILDX_DIR}/docker-buildx"
        chmod +x "${BUILDX_DIR}/docker-buildx"
        success "Docker Buildx ${BUILDX_VERSION} installed"
    fi
else
    success "Docker Buildx $(docker buildx version 2>/dev/null | awk '{print $2}' | head -1)"
fi

success "Docker stack OK"

# ── Already installed? ────────────────────────────────────────────────────────
if [[ -f "${INSTALL_DIR}/docker-compose.yml" ]]; then
    warn "Shipyard appears to already be installed at ${INSTALL_DIR}."
    REINSTALL=""
    read_input "Re-install / overwrite? [y/N] " REINSTALL "n"
    [[ "${REINSTALL}" == "y" || "${REINSTALL}" == "Y" ]] || { info "Aborted."; exit 0; }
fi

# ── Gather config ─────────────────────────────────────────────────────────────
step "Configuration"

DOMAIN=""
read_input "  Domain name (e.g. app.example.com): " DOMAIN ""
[[ -n "${DOMAIN}" ]] || error "Domain name is required."

ACME_EMAIL=""
read_input "  Email for Let's Encrypt certificates: " ACME_EMAIL ""
[[ -n "${ACME_EMAIL}" ]] || error "Email is required for Let's Encrypt."

TAG=""
read_input "  Image tag [latest]: " TAG "latest"

BACKEND_IMAGE_FULL="${BACKEND_IMAGE}:${TAG}"
FRONTEND_IMAGE_FULL="${FRONTEND_IMAGE}:${TAG}"

USE_HTTPS=""
read_input "  Enable HTTPS / TLS? [Y/n]: " USE_HTTPS "y"

if [[ "${USE_HTTPS}" == "y" || "${USE_HTTPS}" == "Y" || "${USE_HTTPS}" == "yes" || "${USE_HTTPS}" == "YES" || "${USE_HTTPS}" == "Yes" ]]; then
    PROTOCOL="https"
else
    PROTOCOL="http"
fi

# ── Secrets ───────────────────────────────────────────────────────────────────
step "Generating secrets"
JWT_SECRET="$(openssl rand -hex 32)"
SECRET_KEY="$(openssl rand -hex 32)"
POSTGRES_PASSWORD="$(openssl rand -hex 16)"
success "Secrets generated"

# ── Directories ───────────────────────────────────────────────────────────────
step "Creating directories"
mkdir -p "${INSTALL_DIR}/data" "${INSTALL_DIR}/certs" "${INSTALL_DIR}/traefik/dynamic"
chmod 700 "${INSTALL_DIR}"
success "Directories ready at ${INSTALL_DIR}"

# ── .env ──────────────────────────────────────────────────────────────────────
info "Writing ${INSTALL_DIR}/.env..."
cat > "${INSTALL_DIR}/.env" <<ENV
# Shipyard — generated by install.sh on $(date -u +"%Y-%m-%dT%H:%M:%SZ")
# DO NOT commit this file to version control.

DOMAIN=${DOMAIN}
ACME_EMAIL=${ACME_EMAIL}
TAG=${TAG}
BACKEND_IMAGE=${BACKEND_IMAGE_FULL}
FRONTEND_IMAGE=${FRONTEND_IMAGE_FULL}
PROTOCOL=${PROTOCOL}

# Database
POSTGRES_USER=shipyard
POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
POSTGRES_DB=shipyard
SHIPYARD__DATABASE__URL=postgres://shipyard:${POSTGRES_PASSWORD}@postgres:5432/shipyard
SHIPYARD__DATABASE__MAX_CONNECTIONS=10

# MQTT
SHIPYARD__MQTT__HOST=mqtt
SHIPYARD__MQTT__PORT=1883
SHIPYARD__MQTT__CLIENT_ID=shipyard-api

# Auth
SHIPYARD__AUTH__JWT_SECRET=${JWT_SECRET}
SHIPYARD__AUTH__ACCESS_TOKEN_EXPIRY=3600
SHIPYARD__AUTH__REFRESH_TOKEN_EXPIRY=604800
SHIPYARD__AUTH__SECRET_KEY=${SECRET_KEY}

# Server
SHIPYARD__SERVER__HOST=0.0.0.0
SHIPYARD__SERVER__PORT=3001

# Docker
SHIPYARD__DOCKER__LABEL_PREFIX=platform

# Traefik
SHIPYARD__TRAEFIK__NETWORK=platform_proxy
SHIPYARD__TRAEFIK__ENTRYPOINT_HTTP=web
SHIPYARD__TRAEFIK__ENTRYPOINT_HTTPS=websecure
SHIPYARD__TRAEFIK__CERT_RESOLVER=letsencrypt
SHIPYARD__TRAEFIK__DYNAMIC_CONFIG_DIR=${INSTALL_DIR}/traefik/dynamic

# Data
SHIPYARD__DATA_DIR=${INSTALL_DIR}/data

RUST_LOG=shipyard=info,tower_http=warn
ENV
chmod 600 "${INSTALL_DIR}/.env"
success ".env written"

# ── rmqtt.toml ────────────────────────────────────────────────────────────────
info "Writing ${INSTALL_DIR}/rmqtt.toml..."
cat > "${INSTALL_DIR}/rmqtt.toml" <<'RMQTT'
[node]
id = 1

[log]
level = "warn"

[listeners.tcp]
addr = "0.0.0.0:1883"

[listeners.websocket]
addr = "0.0.0.0:8083"

[mqtt]
max_packet_size = 1048576
RMQTT

# ── Traefik static config ─────────────────────────────────────────────────────
info "Writing Traefik config..."
cat > "${INSTALL_DIR}/traefik/traefik.yml" <<TRAEFIK
entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
  websecure:
    address: ":443"

providers:
  docker:
    endpoint: "unix:///var/run/docker.sock"
    exposedByDefault: false
    network: platform_proxy
  file:
    directory: /etc/traefik/dynamic
    watch: true

certificatesResolvers:
  letsencrypt:
    acme:
      email: ${ACME_EMAIL}
      storage: /letsencrypt/acme.json
      httpChallenge:
        entryPoint: web

api:
  dashboard: false
TRAEFIK

# ── Traefik dynamic config ────────────────────────────────────────────────────
cat > "${INSTALL_DIR}/traefik/dynamic/shipyard.yml" <<DYNAMIC
http:
  routers:
    shipyard-frontend:
      rule: "Host(\`${DOMAIN}\`)"
      entryPoints: [websecure]
      service: shipyard-frontend
      tls:
        certResolver: letsencrypt

    shipyard-backend:
      rule: "Host(\`api.${DOMAIN}\`)"
      entryPoints: [websecure]
      service: shipyard-backend
      tls:
        certResolver: letsencrypt

  services:
    shipyard-frontend:
      loadBalancer:
        servers:
          - url: "http://frontend:3000"

    shipyard-backend:
      loadBalancer:
        servers:
          - url: "http://backend:3001"
DYNAMIC
success "Traefik config written"

# ── docker-compose.yml ────────────────────────────────────────────────────────
info "Writing ${INSTALL_DIR}/docker-compose.yml..."
cat > "${INSTALL_DIR}/docker-compose.yml" <<COMPOSE
services:

  postgres:
    image: postgres:16-alpine
    container_name: shipyard-postgres
    restart: unless-stopped
    env_file: .env
    environment:
      - POSTGRES_USER=\${POSTGRES_USER}
      - POSTGRES_PASSWORD=\${POSTGRES_PASSWORD}
      - POSTGRES_DB=\${POSTGRES_DB}
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U shipyard"]
      interval: 5s
      timeout: 5s
      retries: 5
    networks:
      - internal

  redis:
    image: redis:7-alpine
    container_name: shipyard-redis
    restart: unless-stopped
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 5
    networks:
      - internal

  mqtt:
    image: rmqtt/rmqtt:latest
    container_name: shipyard-mqtt
    restart: unless-stopped
    ports:
      - "1883:1883"
      - "8083:8083"
    volumes:
      - ${INSTALL_DIR}/rmqtt.toml:/app/rmqtt.toml:ro
      - rmqtt_data:/app/data
    networks:
      - internal

  backend:
    image: \${BACKEND_IMAGE}
    container_name: shipyard-backend
    restart: unless-stopped
    env_file: .env
    volumes:
      - ${INSTALL_DIR}/data:/opt/shipyard/data
      - ${INSTALL_DIR}/traefik/dynamic:/etc/traefik/dynamic
      - /var/run/docker.sock:/var/run/docker.sock
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
      mqtt:
        condition: service_started
    networks:
      - internal
      - platform_proxy

  frontend:
    image: \${FRONTEND_IMAGE}
    container_name: shipyard-frontend
    restart: unless-stopped
    ports:
      - "3000:3000"
    environment:
      - ORIGIN=${PROTOCOL}://${DOMAIN}
      - PRIVATE_API_URL=http://backend:3001
    depends_on:
      backend:
        condition: service_started
    networks:
      - internal
      - platform_proxy

  traefik:
    image: traefik:v3.1
    container_name: shipyard-traefik
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ${INSTALL_DIR}/traefik/traefik.yml:/etc/traefik/traefik.yml:ro
      - ${INSTALL_DIR}/traefik/dynamic:/etc/traefik/dynamic:ro
      - traefik_certs:/letsencrypt
    networks:
      - platform_proxy

networks:
  internal:
    name: shipyard_internal
  platform_proxy:
    name: platform_proxy
    external: true

volumes:
  postgres_data:
  redis_data:
  rmqtt_data:
  traefik_certs:
COMPOSE
success "docker-compose.yml written"

# ── Docker network ────────────────────────────────────────────────────────────
step "Docker network"
docker network inspect platform_proxy >/dev/null 2>&1 \
    || docker network create platform_proxy
success "platform_proxy network ready"

# ── Pull images ───────────────────────────────────────────────────────────────
step "Pulling images"
info "This may take a few minutes on first install..."
cd "${INSTALL_DIR}"
docker compose pull

# ── Start ─────────────────────────────────────────────────────────────────────
step "Starting Shipyard"
docker compose up -d
success "All services started"

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}${BOLD}╔══════════════════════════════════════════════╗${RESET}"
echo -e "${GREEN}${BOLD}║       Shipyard installed successfully!        ║${RESET}"
echo -e "${GREEN}${BOLD}╚══════════════════════════════════════════════╝${RESET}"
echo ""
echo -e "  Dashboard:   ${BOLD}${PROTOCOL}://${DOMAIN}${RESET}"
echo -e "  API:         ${BOLD}${PROTOCOL}://api.${DOMAIN}${RESET}"
echo -e "  Install dir: ${INSTALL_DIR}"
echo ""
echo -e "  Useful commands:"
echo -e "    ${BOLD}cd ${INSTALL_DIR} && docker compose logs -f${RESET}"
echo -e "    ${BOLD}docker compose ps${RESET}"
echo -e "    ${BOLD}docker compose restart backend${RESET}"
echo ""
echo -e "  ${YELLOW}First run:${RESET} visit ${PROTOCOL}://${DOMAIN}/setup to create your admin account."
echo ""
