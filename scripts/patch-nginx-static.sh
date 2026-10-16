#!/usr/bin/env bash
# patch-nginx-static.sh
# Idempotent patch for existing Shipyard installs.
# Run any time on your VPS: sudo bash patch-nginx-static.sh
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
ERRORS_CONFIG="${INSTALL_DIR}/traefik/dynamic/shipyard-static-errors.yml"

[[ -f "${COMPOSE}" ]] || error "Shipyard not found at ${INSTALL_DIR}. Wrong path?"

# ── 1. Directories + static assets ─────────────────────────────────────────────
step "Creating static site directories"
mkdir -p "${SITES_DIR}/conf.d" "${SITES_DIR}/uploads" "${SITES_DIR}/_errors"
success "Directories ready: ${SITES_DIR}/conf.d"

step "Writing custom 404 page and default nginx conf"

# Error pages (_errors/*.html) are written by the Shipyard backend on startup.

# _default.conf: /_errors/ has no 'internal' so Traefik errors middleware can
# also fetch /_errors/404.html directly for non-static-site requests.
cat > "${SITES_DIR}/conf.d/_default.conf" <<NGINX_DEFAULT
# Shipyard default — serves the custom 404 page for unregistered domains.
server {
    listen 80 default_server;
    server_name _;

    error_page 404 /_errors/404.html;

    location /_errors/ {
        root ${SITES_DIR};
    }

    location / {
        return 404;
    }
}
NGINX_DEFAULT

success "404 page and default conf written"

# ── 2. docker-compose.yml ──────────────────────────────────────────────────────
# Always rewrite the nginx-static service block so Docker labels are current.
# Awk removes any existing nginx-static block then inserts the new one before
# the top-level `networks:` key.
step "Updating docker-compose.yml (nginx-static service)"

_NGINX_TMPFILE=$(mktemp)
cat > "${_NGINX_TMPFILE}" <<NGINX_SVC

  # Serves all static sites deployed through Shipyard.
  nginx-static:
    image: nginx:alpine
    container_name: shipyard-nginx-static
    restart: unless-stopped
    volumes:
      - ${SITES_DIR}:${SITES_DIR}:ro
      - ${SITES_DIR}/conf.d:/etc/nginx/conf.d:ro
      - ${SITES_DIR}/logs:/var/log/nginx/shipyard:rw
    networks:
      - platform_proxy
    labels:
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
NGINX_SVC

# Remove existing nginx-static block, then insert the updated block before networks:
awk -v tmpfile="${_NGINX_TMPFILE}" '
    BEGIN { skip=0; inserted=0 }
    /^  nginx-static:/ { skip=1; next }
    skip && (/^  [a-zA-Z]/ || /^networks:/ || /^volumes:/) { skip=0 }
    skip { next }
    /^networks:/ && !inserted {
        while ((getline line < tmpfile) > 0) print line
        close(tmpfile)
        inserted=1
    }
    { print }
    END { if (!inserted) { while ((getline line < tmpfile) > 0) print line } }
' "${COMPOSE}" > "${COMPOSE}.tmp"
mv "${COMPOSE}.tmp" "${COMPOSE}"
rm -f "${_NGINX_TMPFILE}"
success "nginx-static service updated in docker-compose.yml"

# ── 3. Traefik: error-pages middleware ─────────────────────────────────────────
# Write a dedicated file so we never have to surgically modify shipyard.yml.
# Traefik watches the whole dynamic/ directory and merges all files.
# Only define the nginx-static service here if shipyard.yml doesn't already have it
# (duplicate service names across files would cause a Traefik load error).
step "Writing Traefik error-pages middleware config"

if grep -q "nginx-static" "${DYNAMIC}" 2>/dev/null; then
    # Service already declared in shipyard.yml — only write the middleware.
    cat > "${ERRORS_CONFIG}" <<ERRORS_CONF
# Written by patch-nginx-static.sh — safe to re-run (idempotent).
http:
  middlewares:
    shipyard-error-pages:
      errors:
        status:
          - "404"
          - "502-504"
        service: nginx-static
        query: "/_errors/{status}.html"
ERRORS_CONF
else
    # First-time setup — also declare the nginx-static upstream service.
    cat > "${ERRORS_CONFIG}" <<ERRORS_CONF
# Written by patch-nginx-static.sh — safe to re-run (idempotent).
http:
  routers:
    static-sites:
      rule: "HostRegexp(\`.+\`)"
      priority: 1
      entryPoints: [websecure]
      service: nginx-static
      middlewares:
        - shipyard-error-pages
      tls: {}

  services:
    nginx-static:
      loadBalancer:
        servers:
          - url: "http://shipyard-nginx-static:80"

  middlewares:
    shipyard-error-pages:
      errors:
        status:
          - "404"
          - "502-504"
        service: nginx-static
        query: "/_errors/{status}.html"
ERRORS_CONF
fi

success "shipyard-static-errors.yml written (Traefik reloads automatically)"

# ── 4. Recreate nginx-static container ─────────────────────────────────────────
# Recreate via docker compose to keep all labels and project attributes synchronized.
step "Recreating nginx-static container (applies updated Docker Compose config)"

info "Stopping and removing existing container..."
docker rm -f shipyard-nginx-static >/dev/null 2>&1 || true

info "Running docker compose up to recreate nginx-static..."
docker compose -f "${COMPOSE}" up -d --force-recreate nginx-static
success "shipyard-nginx-static recreated successfully"

# ── Done ───────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}${BOLD}╔══════════════════════════════════════════════════╗${RESET}"
echo -e "${GREEN}${BOLD}║   nginx-static patch applied successfully!        ║${RESET}"
echo -e "${GREEN}${BOLD}╚══════════════════════════════════════════════════╝${RESET}"
echo ""
echo -e "  Static site directories: ${BOLD}${SITES_DIR}${RESET}"
echo -e "  nginx container:         ${BOLD}shipyard-nginx-static${RESET}"
echo -e "  Traefik middleware file:  ${BOLD}${ERRORS_CONFIG}${RESET}"
echo ""
echo -e "  Verify with:"
echo -e "    ${BOLD}docker ps | grep nginx-static${RESET}"
echo -e "    ${BOLD}docker inspect shipyard-nginx-static | grep -A1 traefik${RESET}"
echo -e "    ${BOLD}docker logs shipyard-nginx-static${RESET}"
echo ""
