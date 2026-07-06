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

cat > "${SITES_DIR}/_errors/404.html" <<'HTML404'
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
        service: nginx-static
        query: "/_errors/{status}.html"
ERRORS_CONF
fi

success "shipyard-static-errors.yml written (Traefik reloads automatically)"

# ── 4. Recreate nginx-static container ─────────────────────────────────────────
# Docker labels are baked at creation time — a running container never picks up
# new labels. Always recreate so Traefik sees the latest catch-all labels.
step "Recreating nginx-static container (applies updated Docker labels)"

info "Stopping and removing existing container..."
docker stop shipyard-nginx-static >/dev/null 2>&1 || true
docker rm   shipyard-nginx-static >/dev/null 2>&1 || true

# Verify the container is actually gone before we try to create it.
if docker ps -a --format '{{.Names}}' | grep -q '^shipyard-nginx-static$'; then
    error "Could not remove shipyard-nginx-static. Run manually: docker rm -f shipyard-nginx-static"
fi

docker pull nginx:alpine
docker run -d \
    --name    shipyard-nginx-static \
    --restart unless-stopped \
    --network platform_proxy \
    -v "${SITES_DIR}:${SITES_DIR}:ro" \
    -v "${SITES_DIR}/conf.d:/etc/nginx/conf.d:ro" \
    --label "traefik.enable=true" \
    --label "traefik.docker.network=platform_proxy" \
    --label 'traefik.http.routers.static-sites-http.rule=HostRegexp(`.+`)' \
    --label "traefik.http.routers.static-sites-http.priority=1" \
    --label "traefik.http.routers.static-sites-http.entrypoints=web" \
    --label "traefik.http.routers.static-sites-http.service=static-sites" \
    --label 'traefik.http.routers.static-sites-https.rule=HostRegexp(`.+`)' \
    --label "traefik.http.routers.static-sites-https.priority=1" \
    --label "traefik.http.routers.static-sites-https.entrypoints=websecure" \
    --label "traefik.http.routers.static-sites-https.tls=true" \
    --label "traefik.http.routers.static-sites-https.service=static-sites" \
    --label "traefik.http.routers.static-sites-https.middlewares=shipyard-error-pages@file" \
    --label "traefik.http.services.static-sites.loadbalancer.server.port=80" \
    nginx:alpine
success "shipyard-nginx-static recreated with updated labels"

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
