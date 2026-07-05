#!/usr/bin/env bash
# patch-nginx-static.sh
# Adds the shipyard-nginx-static container to an existing Shipyard install.
# Run once on your VPS: sudo bash patch-nginx-static.sh
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${BLUE}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }
step()    { echo -e "\n${BOLD}── $* ──${RESET}"; }

if [[ $EUID -ne 0 ]]; then
    error "Please run as root: sudo bash patch-nginx-static.sh"
fi

INSTALL_DIR="/opt/shipyard"
DATA_DIR="${INSTALL_DIR}/data"
SITES_DIR="${DATA_DIR}/static"
COMPOSE="${INSTALL_DIR}/docker-compose.yml"
DYNAMIC="${INSTALL_DIR}/traefik/dynamic/shipyard.yml"

[[ -f "${COMPOSE}" ]] || error "Shipyard not found at ${INSTALL_DIR}. Wrong path?"

# ── 1. Directories ─────────────────────────────────────────────────────────────
step "Creating static site directories"
mkdir -p "${SITES_DIR}/conf.d" "${SITES_DIR}/uploads"
success "Directories ready: ${SITES_DIR}/conf.d"

# ── 2. docker-compose.yml ──────────────────────────────────────────────────────
step "Patching docker-compose.yml"

if grep -q "shipyard-nginx-static" "${COMPOSE}"; then
    success "nginx-static service already present — skipping compose patch"
else
    # Write a new compose file with nginx-static appended before the networks block.
    NGINX_SERVICE="
  # Serves all static sites deployed through Shipyard.
  nginx-static:
    image: nginx:alpine
    container_name: shipyard-nginx-static
    restart: unless-stopped
    volumes:
      - ${SITES_DIR}:${SITES_DIR}:ro
      - ${SITES_DIR}/conf.d:/etc/nginx/conf.d:ro
    networks:
      - platform_proxy
"
    # Insert before the 'networks:' top-level key
    awk -v svc="${NGINX_SERVICE}" '
        /^networks:/ && !done { print svc; done=1 }
        { print }
    ' "${COMPOSE}" > "${COMPOSE}.tmp"
    mv "${COMPOSE}.tmp" "${COMPOSE}"
    success "nginx-static service added to docker-compose.yml"
fi

# ── 3. Traefik dynamic config ──────────────────────────────────────────────────
step "Patching Traefik dynamic config"

if grep -q "nginx-static" "${DYNAMIC}"; then
    success "nginx-static route already present in Traefik config — skipping"
else
    # Detect whether TLS is in use by looking for certResolver in the existing config.
    if grep -q "certResolver" "${DYNAMIC}"; then
        TLS_BLOCK="      tls:
        certResolver: letsencrypt"
        ENTRYPOINTS="[websecure]"
    else
        TLS_BLOCK=""
        ENTRYPOINTS="[web]"
    fi

    # Append the nginx-static service and catch-all router.
    cat >> "${DYNAMIC}" <<PATCH

    # Catch-all for static sites — any domain not matched above is forwarded to
    # the nginx-static container which handles server_name routing per site.
    # Priority 1 ensures this never wins over explicit Shipyard app routes.
    static-sites:
      rule: "HostRegexp(\`.+\`)"
      priority: 1
      entryPoints: ${ENTRYPOINTS}
      service: nginx-static
${TLS_BLOCK}

  # (appended by patch-nginx-static.sh)
    nginx-static:
      loadBalancer:
        servers:
          - url: "http://shipyard-nginx-static:80"
PATCH

    success "static-sites route added to Traefik dynamic config"
    warn "Traefik reloads this file automatically — no restart needed."
fi

# ── 4. Start nginx-static ──────────────────────────────────────────────────────
step "Starting nginx-static container"

cd "${INSTALL_DIR}"

if docker ps --format '{{.Names}}' | grep -q "^shipyard-nginx-static$"; then
    success "shipyard-nginx-static is already running"
else
    docker compose pull nginx-static
    docker compose up -d nginx-static
    success "shipyard-nginx-static started"
fi

# ── Done ───────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}${BOLD}╔══════════════════════════════════════════════════╗${RESET}"
echo -e "${GREEN}${BOLD}║   nginx-static patch applied successfully!        ║${RESET}"
echo -e "${GREEN}${BOLD}╚══════════════════════════════════════════════════╝${RESET}"
echo ""
echo -e "  Static site directories: ${BOLD}${SITES_DIR}${RESET}"
echo -e "  nginx container:         ${BOLD}shipyard-nginx-static${RESET}"
echo ""
echo -e "  Verify with:"
echo -e "    ${BOLD}docker ps | grep nginx-static${RESET}"
echo -e "    ${BOLD}docker logs shipyard-nginx-static${RESET}"
echo ""
