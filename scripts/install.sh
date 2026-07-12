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
EDGE_RUNTIME_IMAGE="triandamai827/shipyard-edge-runtime"

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
    COMPOSE_DIR="/usr/libexec/docker/cli-plugins"
    if [ ! -d "$COMPOSE_DIR" ] && [ -d "/usr/local/lib/docker/cli-plugins" ]; then
        COMPOSE_DIR="/usr/local/lib/docker/cli-plugins"
    fi
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
    BUILDX_DIR="/usr/libexec/docker/cli-plugins"
    if [ ! -d "$BUILDX_DIR" ] && [ -d "/usr/local/lib/docker/cli-plugins" ]; then
        BUILDX_DIR="/usr/local/lib/docker/cli-plugins"
    fi
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

# ── Docker Swarm ──────────────────────────────────────────────────────────────
step "Checking Docker Swarm"
if [[ "$(docker info --format '{{.Swarm.LocalNodeState}}' 2>/dev/null)" != "active" ]]; then
    info "Initializing Docker Swarm..."
    PRIMARY_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
    if [[ -z "${PRIMARY_IP}" ]]; then
        PRIMARY_IP="127.0.0.1"
    fi
    docker swarm init --advertise-addr "${PRIMARY_IP}"
    success "Docker Swarm initialized"
else
    success "Docker Swarm is already active"
fi

# Keep only 1 old (shutdown) task per slot so redeploys don't accumulate orphans.
docker swarm update --task-history-limit 1 &>/dev/null || true

success "Docker stack OK"

# ── Already installed? ────────────────────────────────────────────────────────
if [[ -f "${INSTALL_DIR}/docker-compose.yml" ]]; then
    warn "Shipyard appears to already be installed at ${INSTALL_DIR}."
    REINSTALL=""
    read_input "Re-install / overwrite? (Warning: This will delete all persistent data/volumes) [y/N] " REINSTALL "n"
    if [[ "${REINSTALL}" == "y" || "${REINSTALL}" == "Y" ]]; then
        info "Stopping running services..."
        cd "${INSTALL_DIR}" && docker compose down || true
        cd - >/dev/null
        # Remove the nginx-static container so it gets recreated with fresh mounts.
        docker rm -f shipyard-nginx-static 2>/dev/null || true
        # Remove app data volumes but preserve traefik_certs (contains ACME/TLS cert).
        # Deleting traefik_certs triggers a fresh LE cert request and burns the rate limit.
        for vol in postgres_data redis_data rmqtt_data; do
            docker volume rm "shipyard_${vol}" 2>/dev/null || true
        done
    else
        info "Aborted."
        exit 0
    fi
fi

# ── Gather config ─────────────────────────────────────────────────────────────
step "Configuration"

DOMAIN=""
read_input "  Domain name (e.g. app.example.com): " DOMAIN ""
[[ -n "${DOMAIN}" ]] || error "Domain name is required."

ACME_EMAIL=""
read_input "  Email for Let's Encrypt certificates: " ACME_EMAIL ""
[[ -n "${ACME_EMAIL}" ]] || error "Email is required for Let's Encrypt."

SCRIPTS_URL=""
read_input "  Landing/scripts base URL (e.g. https://shipyard.example.com) [leave blank to skip auto-update]: " SCRIPTS_URL ""

TAG=""
read_input "  Image tag [latest]: " TAG "latest"

BACKEND_IMAGE_FULL="${BACKEND_IMAGE}:${TAG}"
FRONTEND_IMAGE_FULL="${FRONTEND_IMAGE}:${TAG}"
EDGE_RUNTIME_IMAGE_FULL="${EDGE_RUNTIME_IMAGE}:${TAG}"

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
MQTT_PASSWORD="$(openssl rand -hex 24)"
EDGE_RUNTIME_SECRET="$(openssl rand -hex 24)"

if [[ -f "${INSTALL_DIR}/.env" ]]; then
    info "Preserving existing secrets from ${INSTALL_DIR}/.env..."
    EXISTING_JWT_SECRET=$(grep -E "^SHIPYARD__AUTH__JWT_SECRET=" "${INSTALL_DIR}/.env" | cut -d= -f2-)
    EXISTING_SECRET_KEY=$(grep -E "^SHIPYARD__AUTH__SECRET_KEY=" "${INSTALL_DIR}/.env" | cut -d= -f2-)
    EXISTING_POSTGRES_PASSWORD=$(grep -E "^POSTGRES_PASSWORD=" "${INSTALL_DIR}/.env" | cut -d= -f2-)
    EXISTING_MQTT_PASSWORD=$(grep -E "^SHIPYARD__MQTT__PASSWORD=" "${INSTALL_DIR}/.env" | cut -d= -f2-)
    EXISTING_EDGE_RUNTIME_SECRET=$(grep -E "^SHIPYARD__EDGE_FUNCTIONS__RUNTIME_SECRET=" "${INSTALL_DIR}/.env" | cut -d= -f2-)

    JWT_SECRET="${EXISTING_JWT_SECRET:-$JWT_SECRET}"
    SECRET_KEY="${EXISTING_SECRET_KEY:-$SECRET_KEY}"
    POSTGRES_PASSWORD="${EXISTING_POSTGRES_PASSWORD:-$POSTGRES_PASSWORD}"
    MQTT_PASSWORD="${EXISTING_MQTT_PASSWORD:-$MQTT_PASSWORD}"
    EDGE_RUNTIME_SECRET="${EXISTING_EDGE_RUNTIME_SECRET:-$EDGE_RUNTIME_SECRET}"
fi
success "Secrets ready"

# ── Directories ───────────────────────────────────────────────────────────────
step "Creating directories"
mkdir -p "${INSTALL_DIR}/data" "${INSTALL_DIR}/certs" "${INSTALL_DIR}/traefik/dynamic"
# Static site directories — nginx conf.d must exist before the container starts
# so the bind mount succeeds even before any static site is deployed.
mkdir -p "${INSTALL_DIR}/data/static/conf.d" "${INSTALL_DIR}/data/static/uploads" "${INSTALL_DIR}/data/static/_errors"
chmod 700 "${INSTALL_DIR}"

# Write the default nginx server conf and branded 404 page.
# nginx-static serves these for any domain not matched by a deployed site's conf.
cat > "${INSTALL_DIR}/data/static/conf.d/_default.conf" <<'NGINX_DEFAULT'
# Shipyard default — serves the custom 404 page for unregistered domains.
server {
    listen 80 default_server;
    server_name _;

    error_page 404 /_errors/404.html;

    location /_errors/ {
        root SITES_BASE_PLACEHOLDER;
        internal;
    }

    location / {
        return 404;
    }
}
NGINX_DEFAULT
# Replace the placeholder with the actual path
sed -i "s|SITES_BASE_PLACEHOLDER|${INSTALL_DIR}/data/static|g" \
    "${INSTALL_DIR}/data/static/conf.d/_default.conf"

cat > "${INSTALL_DIR}/data/static/_errors/404.html" <<'HTML404'
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>404 – Not Found</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      background: #0f172a; color: #e2e8f0;
      min-height: 100vh;
      display: flex; flex-direction: column;
      align-items: center; justify-content: center;
      padding: 2rem;
    }
    .num {
      font-size: 6.5rem; font-weight: 800; line-height: 1;
      background: linear-gradient(135deg, #3b82f6 0%, #8b5cf6 100%);
      -webkit-background-clip: text; -webkit-text-fill-color: transparent;
      background-clip: text; margin-bottom: 1.25rem;
    }
    h1 { font-size: 1.4rem; font-weight: 600; color: #f1f5f9; margin-bottom: 0.65rem; }
    p { color: #94a3b8; font-size: 0.9rem; text-align: center; max-width: 380px; margin-bottom: 2.5rem; }
    .badge {
      display: inline-flex; align-items: center; gap: 0.45rem;
      font-size: 0.73rem; color: #475569;
      border: 1px solid #1e293b; border-radius: 9999px;
      padding: 0.35rem 0.9rem;
    }
    .dot { width: 6px; height: 6px; border-radius: 50%; background: #3b82f6; flex-shrink: 0; }
  </style>
</head>
<body>
  <div class="num">404</div>
  <h1>Page Not Found</h1>
  <p>The page you're looking for doesn't exist or hasn't been deployed yet.</p>
  <div class="badge"><span class="dot"></span>Powered by Shipyard</div>
</body>
</html>
HTML404

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
EDGE_RUNTIME_IMAGE=${EDGE_RUNTIME_IMAGE_FULL}
PROTOCOL=${PROTOCOL}

# Database
POSTGRES_USER=shipyard
POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
POSTGRES_DB=shipyard
SHIPYARD__DATABASE__URL=postgres://shipyard:${POSTGRES_PASSWORD}@shipyard-postgres:5432/shipyard
SHIPYARD__DATABASE__MAX_CONNECTIONS=10

# MQTT
SHIPYARD__MQTT__HOST=shipyard-mqtt
SHIPYARD__MQTT__PORT=1883
SHIPYARD__MQTT__CLIENT_ID=shipyard-api
SHIPYARD__MQTT__USERNAME=shipyard-api
SHIPYARD__MQTT__PASSWORD=${MQTT_PASSWORD}

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
SHIPYARD__DOCKER__PORT_PROXY=false

# Traefik
SHIPYARD__TRAEFIK__NETWORK=platform_proxy
SHIPYARD__TRAEFIK__ENTRYPOINT_HTTP=web
SHIPYARD__TRAEFIK__ENTRYPOINT_HTTPS=websecure
SHIPYARD__TRAEFIK__CERT_RESOLVER=letsencrypt
SHIPYARD__TRAEFIK__DYNAMIC_CONFIG_DIR=/etc/traefik/dynamic

# Data
SHIPYARD__DATA_DIR=/opt/shipyard/data

# Edge Functions (Deno-based serverless)
SHIPYARD__EDGE_FUNCTIONS__ENABLED=true
SHIPYARD__EDGE_FUNCTIONS__RUNTIME_IMAGE=${EDGE_RUNTIME_IMAGE_FULL}
SHIPYARD__EDGE_FUNCTIONS__RUNTIME_SECRET=${EDGE_RUNTIME_SECRET}

# Scripts base URL — used by update.sh to self-update from the landing site.
# Set this to the public URL of your Shipyard landing (e.g. https://shipyard.example.com).
SCRIPTS_URL=${SCRIPTS_URL}

RUST_LOG=shipyard=info,tower_http=warn
ENV
chmod 600 "${INSTALL_DIR}/.env"
success ".env written"

# ── rmqtt.toml ────────────────────────────────────────────────────────────────
info "Writing ${INSTALL_DIR}/rmqtt.toml..."
cat > "${INSTALL_DIR}/rmqtt.toml" <<RMQTT
[log]
level = "warn"

[node]
id = 1
cookie = "shipyard"
data_path = "/app/data"

[rpc]
server_addr = "0.0.0.0:5363"

[listener.tcp.external]
addr = "0.0.0.0:1883"

[listener.ws.external]
addr = "0.0.0.0:8083"

[plugins]
dir = "rmqtt-plugins/"
default_startups = [
    "rmqtt-retainer",
    "rmqtt-sys-topic",
    "rmqtt-message-storage",
    "rmqtt-session-storage",
    "rmqtt-auth-http",
    "rmqtt-acl",
    "rmqtt-counter",
    "rmqtt-http-api"
]

[plugins.rmqtt-acl]
enable = false

[plugins.rmqtt-auth-http]
enable = true

[plugins.rmqtt-http-api]
addr = "0.0.0.0:6060"

[mqtt]
max_packet_size = "10mb"
max_inflight = 16
session_expiry_interval = "2h"
allow_anonymous = false
RMQTT

# ── rmqtt-auth-http plugin config ─────────────────────────────────────────────
# This file is loaded by the rmqtt-auth-http plugin from its plugins dir.
# The [plugins.rmqtt-auth-http] section in rmqtt.toml only enables/disables it;
# the URL and rules must live here.
info "Writing ${INSTALL_DIR}/rmqtt-auth-http.toml..."
cat > "${INSTALL_DIR}/rmqtt-auth-http.toml" <<'AUTHHTTP'
http_timeout = "5s"
http_connections = 50
http_keepalive_timeout = "60s"
concurrency_limit = 10

[[rules]]
uri = "http://shipyard-backend:3001/internal/mqtt/auth"
method = "post"
params.clientid = "${clientid}"
params.username = "${username}"
params.password = "${password}"
params.ipaddr = "${ipaddr}"
AUTHHTTP
success "rmqtt-auth-http.toml written"

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
# The catch-all uses a self-signed fallback cert (tls: {}) so it is always active —
# per-domain routers handle real Let's Encrypt cert issuance.
if [[ "${PROTOCOL}" == "https" ]]; then
    STATIC_SITE_TLS="      tls: {}"
    STATIC_SITE_ENTRYPOINTS="[websecure]"
else
    STATIC_SITE_TLS=""
    STATIC_SITE_ENTRYPOINTS="[web]"
fi

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
      rule: "Host(\`api-${DOMAIN}\`) && PathPrefix(\`/openapi/v1\`)"
      entryPoints: [websecure]
      service: shipyard-backend
      tls:
        certResolver: letsencrypt

    shipyard-mqtt:
      rule: "Host(\`${DOMAIN}\`) && PathPrefix(\`/mqtt\`)"
      entryPoints: [websecure]
      service: shipyard-mqtt
      middlewares:
        - shipyard-mqtt-strip
      tls:
        certResolver: letsencrypt

    # Catch-all for static sites — any domain not matched above is forwarded to
    # the nginx-static container which handles server_name routing per-site.
    # Priority 1 ensures this never wins over explicit Shipyard app routes.
    static-sites:
      rule: "HostRegexp(\`.+\`)"
      priority: 1
      entryPoints: ${STATIC_SITE_ENTRYPOINTS}
      service: nginx-static
${STATIC_SITE_TLS}

  services:
    shipyard-frontend:
      loadBalancer:
        servers:
          - url: "http://shipyard-frontend:3000"

    shipyard-backend:
      loadBalancer:
        servers:
          - url: "http://shipyard-backend:3001"

    shipyard-mqtt:
      loadBalancer:
        servers:
          - url: "http://shipyard-mqtt:8083"

    nginx-static:
      loadBalancer:
        servers:
          - url: "http://shipyard-nginx-static:80"

  middlewares:
    shipyard-mqtt-strip:
      stripPrefix:
        prefixes:
          - "/mqtt"

    # Intercepts 404 responses from any backend and returns the Shipyard branded
    # error page served by nginx-static. Works alongside the catch-all router so
    # every possible path to a 404 shows the same styled page.
    shipyard-error-pages:
      errors:
        status:
          - "404"
        service: nginx-static
        query: "/_errors/{status}.html"
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
    # Ports are NOT published to the host — the broker is only reachable from
    # inside Docker (backend via internal network, browsers via Traefik /mqtt).
    # This prevents public internet scanners from reaching port 1883.
    volumes:
      - ${INSTALL_DIR}/rmqtt.toml:/app/rmqtt/rmqtt.toml:ro
      - ${INSTALL_DIR}/rmqtt-auth-http.toml:/app/rmqtt/rmqtt-plugins/rmqtt-auth-http.toml:ro
      - rmqtt_data:/app/data
    networks:
      - internal
      - platform_proxy

  backend:
    image: \${BACKEND_IMAGE}
    container_name: shipyard-backend
    restart: unless-stopped
    env_file: .env
    volumes:
      - ${INSTALL_DIR}:/opt/shipyard
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
      - PRIVATE_API_URL=http://shipyard-backend:3001
    depends_on:
      backend:
        condition: service_started
    networks:
      - internal
      - platform_proxy

  traefik:
    image: traefik:v3.7.6
    container_name: shipyard-traefik
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    environment:
      - DOCKER_API_VERSION=1.40
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ${INSTALL_DIR}/traefik/traefik.yml:/etc/traefik/traefik.yml:ro
      - ${INSTALL_DIR}/traefik/dynamic:/etc/traefik/dynamic:ro
      - traefik_certs:/letsencrypt
    networks:
      - platform_proxy

  # Serves all static sites deployed through Shipyard.
  # The engine automatically starts this container on first static deploy, but
  # defining it here ensures it is present immediately after install.
  nginx-static:
    image: nginx:alpine
    container_name: shipyard-nginx-static
    restart: unless-stopped
    volumes:
      - ${INSTALL_DIR}/data/static:${INSTALL_DIR}/data/static:ro
      - ${INSTALL_DIR}/data/static/conf.d:/etc/nginx/conf.d:ro
      - ${INSTALL_DIR}/data/static/logs:/var/log/nginx/shipyard:rw
    networks:
      - platform_proxy
    labels:
      # Traefik catch-all: route every domain that has no explicit Traefik router
      # to nginx-static, which then dispatches by server_name.
      # Two routers cover both HTTP (before redirect) and HTTPS.
      - "traefik.enable=true"
      - "traefik.docker.network=platform_proxy"
      - "traefik.http.routers.static-sites-http.rule=HostRegexp(\`.+\`)"
      - "traefik.http.routers.static-sites-http.priority=1"
      - "traefik.http.routers.static-sites-http.entrypoints=web"
      - "traefik.http.routers.static-sites-http.service=static-sites"
      - "traefik.http.routers.static-sites-https.rule=HostRegexp(\`.+\`)"
      - "traefik.http.routers.static-sites-https.priority=1"
      - "traefik.http.routers.static-sites-https.entrypoints=websecure"
      - "traefik.http.routers.static-sites-https.tls=true"
      - "traefik.http.routers.static-sites-https.service=static-sites"
      - "traefik.http.routers.static-sites-https.middlewares=shipyard-error-pages@file"
      - "traefik.http.services.static-sites.loadbalancer.server.port=80"

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

# ── update.sh ─────────────────────────────────────────────────────────────────
info "Writing ${INSTALL_DIR}/update.sh..."
# Try to download the canonical update.sh from the landing site so we always
# install the latest version. Fall back to the copy bundled inside install.sh.
DOWNLOADED_UPDATE=""
if [[ -n "${SCRIPTS_URL}" ]]; then
    DOWNLOADED_UPDATE=$(curl -fsSL "${SCRIPTS_URL}/update.sh" 2>/dev/null || true)
fi

if [[ -n "${DOWNLOADED_UPDATE}" ]]; then
    echo "${DOWNLOADED_UPDATE}" > "${INSTALL_DIR}/update.sh"
    info "update.sh fetched from ${SCRIPTS_URL}"
else
    # Bundled fallback — kept in sync with scripts/update.sh in the repo.
    cat > "${INSTALL_DIR}/update.sh" <<'UPDATE'
#!/usr/bin/env bash
set -euo pipefail
INSTALL_DIR="/opt/shipyard"
cd "${INSTALL_DIR}"
set -a; source "${INSTALL_DIR}/.env"; set +a

if [[ -n "${SCRIPTS_URL:-}" ]]; then
    FRESH=$(curl -fsSL "${SCRIPTS_URL}/update.sh" 2>/dev/null || true)
    if [[ -n "${FRESH}" && "${FRESH}" != "$(cat "${INSTALL_DIR}/update.sh")" ]]; then
        echo "[shipyard] Updating update.sh from ${SCRIPTS_URL}..."
        echo "${FRESH}" > "${INSTALL_DIR}/update.sh.new"
        chmod +x "${INSTALL_DIR}/update.sh.new"
        mv "${INSTALL_DIR}/update.sh.new" "${INSTALL_DIR}/update.sh"
        exec "${INSTALL_DIR}/update.sh"
    fi
fi

echo "[shipyard] Pulling latest images..."
docker compose pull

if [[ -n "${EDGE_RUNTIME_IMAGE:-}" ]]; then
    echo "[shipyard] Pulling edge runtime image: ${EDGE_RUNTIME_IMAGE}..."
    docker pull "${EDGE_RUNTIME_IMAGE}" || true
fi

echo "[shipyard] Spawning detached updater to apply new images..."
docker rm -f shipyard-updater 2>/dev/null || true
docker run --rm -d \
    --name shipyard-updater \
    -e EDGE_RUNTIME_IMAGE="${EDGE_RUNTIME_IMAGE:-}" \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -v "${INSTALL_DIR}:${INSTALL_DIR}" \
    -w "${INSTALL_DIR}" \
    docker:cli \
    sh -c 'sleep 5 \
        && docker compose up -d --remove-orphans \
        && if [ -n "${EDGE_RUNTIME_IMAGE}" ]; then \
               docker service ls --filter name=shipyard-edge- --format "{{.Name}}" \
               | while read -r svc; do \
                   [ -n "$svc" ] && docker service update \
                       --image "${EDGE_RUNTIME_IMAGE}" \
                       --with-registry-auth \
                       "${svc}" \
                       && echo "[shipyard] updated ${svc}"; \
               done; \
           fi'

echo "[shipyard] Images pulled. All services will restart with new images in ~5 seconds."
UPDATE
fi
chmod +x "${INSTALL_DIR}/update.sh"
success "update.sh written"

# ── Docker network ────────────────────────────────────────────────────────────
step "Docker network"
if docker network inspect platform_proxy >/dev/null 2>&1; then
    DRIVER=$(docker network inspect platform_proxy --format '{{.Driver}}' 2>/dev/null)
    if [[ "${DRIVER}" != "overlay" ]]; then
        info "Recreating platform_proxy network as overlay..."
        docker network rm platform_proxy
        docker network create --driver overlay --attachable platform_proxy
    fi
else
    info "Creating platform_proxy network as overlay..."
    docker network create --driver overlay --attachable platform_proxy
fi
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
echo -e "  API:         ${BOLD}${PROTOCOL}://api-${DOMAIN}${RESET}"
echo -e "  Install dir: ${INSTALL_DIR}"
echo ""
echo -e "  Useful commands:"
echo -e "    ${BOLD}cd ${INSTALL_DIR} && docker compose logs -f${RESET}"
echo -e "    ${BOLD}docker compose ps${RESET}"
echo -e "    ${BOLD}docker compose restart backend${RESET}"
echo ""
echo -e "  ${YELLOW}First run:${RESET} visit ${PROTOCOL}://${DOMAIN}/setup to create your admin account."
echo ""
echo -e "  ${BLUE}Static sites:${RESET} the nginx-static container is running and ready."
echo -e "    Deploy a static site and add a custom domain — Traefik will route it automatically."
echo ""
