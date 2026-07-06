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

cat > "${SITES_DIR}/conf.d/_default.conf" <<NGINX_DEFAULT
# Shipyard default — serves the custom 404 page for unregistered domains.
server {
    listen 80 default_server;
    server_name _;

    error_page 404 /_errors/404.html;

    location /_errors/ {
        root ${SITES_DIR};
        internal;
    }

    location / {
        return 404;
    }
}
NGINX_DEFAULT

success "404 page and default conf written"

# ── 2. docker-compose.yml ──────────────────────────────────────────────────────
step "Patching docker-compose.yml"

if grep -q "shipyard-nginx-static" "${COMPOSE}"; then
    success "nginx-static service already present — skipping compose patch"
else
    # Write the nginx-static service block to a temp file (avoids awk -v issues with
    # backticks and multiline strings).
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
      - "traefik.http.services.static-sites.loadbalancer.server.port=80"
NGINX_SVC

    # Insert before the 'networks:' top-level key
    awk -v svcfile="${_NGINX_TMPFILE}" '
        /^networks:/ && !done {
            while ((getline line < svcfile) > 0) print line;
            done=1
        }
        { print }
    ' "${COMPOSE}" > "${COMPOSE}.tmp"
    mv "${COMPOSE}.tmp" "${COMPOSE}"
    rm -f "${_NGINX_TMPFILE}"
    success "nginx-static service added to docker-compose.yml"
fi

# ── 3. Traefik dynamic config ──────────────────────────────────────────────────
step "Patching Traefik dynamic config"

if grep -q "nginx-static" "${DYNAMIC}"; then
    success "nginx-static route already present in Traefik config — skipping"
else
    # Detect whether TLS is in use — catch-all uses self-signed fallback (tls: {}),
    # not certResolver, so it is always active regardless of cert issuance state.
    if grep -q "certResolver" "${DYNAMIC}"; then
        TLS_BLOCK="      tls: {}"
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
