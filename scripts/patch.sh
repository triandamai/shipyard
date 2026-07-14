#!/usr/bin/env bash
# Shipyard patch script — safe to run on a live installation.
# Updates update.sh and fills in any missing .env vars without touching
# running containers, volumes, or compose configuration.
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${BLUE}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }

INSTALL_DIR="/opt/shipyard"
ENV_FILE="${INSTALL_DIR}/.env"
UPDATE_SCRIPT="${INSTALL_DIR}/update.sh"

[[ $EUID -ne 0 ]] && error "Please run as root: sudo bash patch.sh"
[[ -f "${ENV_FILE}" ]] || error "No .env found at ${ENV_FILE} — is Shipyard installed?"

# ── Source existing .env ───────────────────────────────────────────────────────
set -a; source "${ENV_FILE}"; set +a

# ── Interactive prompt helper ──────────────────────────────────────────────────
# Usage: prompt_input "Prompt text: " VAR_NAME "default_value"
# Reads from /dev/tty so it works even when the script is piped via curl | bash.
prompt_input() {
    local prompt="$1"
    local var_name="$2"
    local default="$3"
    local input=""
    if [[ -t 0 ]]; then
        read -rp "${prompt}" input
    elif [[ -c /dev/tty ]]; then
        read -rp "${prompt}" input < /dev/tty
    fi
    [[ -z "${input}" ]] && input="${default}"
    printf -v "${var_name}" '%s' "${input}"
}

# ── Append missing env vars ────────────────────────────────────────────────────
info "Checking .env for missing vars..."

append_if_missing() {
    local key="$1"
    local value="$2"
    if ! grep -qE "^${key}=" "${ENV_FILE}"; then
        echo "${key}=${value}" >> "${ENV_FILE}"
        success "Added ${key} to .env"
    else
        info "${key} already set — skipping"
    fi
}

# Derive DOCKERHUB_USERNAME from existing BACKEND_IMAGE if available.
EXISTING_BACKEND_IMAGE="${BACKEND_IMAGE:-}"
DOCKERHUB_USER=""
if [[ -n "${EXISTING_BACKEND_IMAGE}" ]]; then
    DOCKERHUB_USER=$(echo "${EXISTING_BACKEND_IMAGE}" | cut -d/ -f1)
fi
DOCKERHUB_USER="${DOCKERHUB_USER:-triandamai827}"

TAG_VALUE="${TAG:-latest}"

append_if_missing "SHIPYARD__TRAEFIK__DYNAMIC_CONFIG_DIR" "/etc/traefik/dynamic"
append_if_missing "EDGE_RUNTIME_IMAGE" "${DOCKERHUB_USER}/shipyard-edge-runtime:${TAG_VALUE}"
# SHIPYARD__EDGE_FUNCTIONS__RUNTIME_IMAGE is what the backend reads for create_runtime_service.
# EDGE_RUNTIME_IMAGE (no prefix) is only used by update.sh for docker pull / swarm update.
append_if_missing "SHIPYARD__EDGE_FUNCTIONS__RUNTIME_IMAGE" "${DOCKERHUB_USER}/shipyard-edge-runtime:${TAG_VALUE}"
append_if_missing "SHIPYARD__EDGE_FUNCTIONS__RUNTIME_SECRET" "$(openssl rand -hex 24)"
append_if_missing "SCRIPTS_URL" ""

# API URL used by edge runtime Swarm services on worker nodes to reach the backend.
# Worker nodes can't resolve the container name 'shipyard-backend' — must use Traefik.
DETECTED_PROTOCOL="${PROTOCOL:-https}"
DETECTED_DOMAIN="${DOMAIN:-}"

# ── New vars introduced in registry refactor ───────────────────────────────────
# Prompt for API and registry domains if not already set.
# Skip the prompt if the var is already present in .env (idempotent re-runs).

NEEDS_API_DOMAIN=false
NEEDS_REGISTRY_DOMAIN=false
grep -qE "^SHIPYARD__API_DOMAIN=" "${ENV_FILE}"       || NEEDS_API_DOMAIN=true
grep -qE "^SHIPYARD__REGISTRY__HOSTNAME=" "${ENV_FILE}" || NEEDS_REGISTRY_DOMAIN=true

if [[ "${NEEDS_API_DOMAIN}" == "true" || "${NEEDS_REGISTRY_DOMAIN}" == "true" ]]; then
    echo ""
    info "New domains are required for this patch."
    echo -e "  Current app domain: ${BOLD}${DETECTED_DOMAIN:-<unknown>}${RESET}"
    echo ""
fi

if [[ "${NEEDS_API_DOMAIN}" == "true" ]]; then
    PATCH_API_DOMAIN=""
    prompt_input "  API domain (e.g. api.${DETECTED_DOMAIN:-example.com}) [api-${DETECTED_DOMAIN:-example.com}]: " \
        PATCH_API_DOMAIN "api-${DETECTED_DOMAIN:-example.com}"
    [[ -n "${PATCH_API_DOMAIN}" ]] || error "API domain is required."
    echo "SHIPYARD__API_DOMAIN=${PATCH_API_DOMAIN}" >> "${ENV_FILE}"
    success "Added SHIPYARD__API_DOMAIN=${PATCH_API_DOMAIN}"
else
    PATCH_API_DOMAIN="${SHIPYARD__API_DOMAIN:-}"
    info "SHIPYARD__API_DOMAIN already set to '${PATCH_API_DOMAIN}' — skipping"
fi

if [[ "${NEEDS_REGISTRY_DOMAIN}" == "true" ]]; then
    PATCH_REGISTRY_DOMAIN=""
    prompt_input "  Registry domain (e.g. registry.${DETECTED_DOMAIN:-example.com}) [registry.${DETECTED_DOMAIN:-example.com}]: " \
        PATCH_REGISTRY_DOMAIN "registry.${DETECTED_DOMAIN:-example.com}"
    [[ -n "${PATCH_REGISTRY_DOMAIN}" ]] || error "Registry domain is required."
    echo "SHIPYARD__REGISTRY__HOSTNAME=${PATCH_REGISTRY_DOMAIN}" >> "${ENV_FILE}"
    success "Added SHIPYARD__REGISTRY__HOSTNAME=${PATCH_REGISTRY_DOMAIN}"
else
    PATCH_REGISTRY_DOMAIN="${SHIPYARD__REGISTRY__HOSTNAME:-}"
    info "SHIPYARD__REGISTRY__HOSTNAME already set to '${PATCH_REGISTRY_DOMAIN}' — skipping"
fi

# SHIPYARD__REGISTRY__STORAGE: storage backend for the artifact registry.
append_if_missing "SHIPYARD__REGISTRY__STORAGE" "local"

# SHIPYARD__EDGE_FUNCTIONS__RUNTIME_API_URL: use the prompted API domain.
if [[ -n "${PATCH_API_DOMAIN}" ]]; then
    append_if_missing "SHIPYARD__EDGE_FUNCTIONS__RUNTIME_API_URL" "${DETECTED_PROTOCOL}://${PATCH_API_DOMAIN}"
else
    warn "API domain unknown — skipping SHIPYARD__EDGE_FUNCTIONS__RUNTIME_API_URL"
    warn "Add it manually: SHIPYARD__EDGE_FUNCTIONS__RUNTIME_API_URL=https://<api-domain>"
fi

# Re-source so new vars are available below.
set -a; source "${ENV_FILE}"; set +a

# ── Write new update.sh ────────────────────────────────────────────────────────
info "Writing new ${UPDATE_SCRIPT}..."

# If SCRIPTS_URL is set, try fetching the canonical copy first.
FETCHED=""
if [[ -n "${SCRIPTS_URL:-}" ]]; then
    FETCHED=$(curl -fsSL "${SCRIPTS_URL}/update.sh" 2>/dev/null || true)
fi

if [[ -n "${FETCHED}" ]]; then
    echo "${FETCHED}" > "${UPDATE_SCRIPT}"
    success "update.sh fetched from ${SCRIPTS_URL}"
else
    cat > "${UPDATE_SCRIPT}" <<'UPDATE'
#!/usr/bin/env bash
set -euo pipefail
INSTALL_DIR="/opt/shipyard"
cd "${INSTALL_DIR}"
set -a; source "${INSTALL_DIR}/.env"; set +a

# Self-update from landing site if SCRIPTS_URL is configured.
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
    success "update.sh written from bundled copy"
fi

chmod +x "${UPDATE_SCRIPT}"

# ── Remove old per-domain edge Traefik config files ───────────────────────────
# Edge function domain configs migrated from per-domain files (edge-domain-*.yml)
# to per-service files ({slug}.yml). Old files must be removed or Traefik will
# keep routing on the deleted domains.
OLD_EDGE_FILES=("${INSTALL_DIR}"/traefik/dynamic/edge-domain-*.yml)
if [[ -e "${OLD_EDGE_FILES[0]}" ]]; then
    rm -f "${INSTALL_DIR}"/traefik/dynamic/edge-domain-*.yml
    success "Removed old per-domain edge Traefik config files"
else
    info "No old edge-domain-*.yml files found — skipping"
fi

# ── Rewrite shipyard.yml with updated Traefik config ─────────────────────────
# Changes in this release:
#   • OCI registry is now nested at /registry in the backend — Traefik must prepend
#     /registry with addPrefix middleware before forwarding to shipyard-backend.
#   • A dedicated shipyard-registry router is added for the registry hostname.
#   • shipyard-backend router is renamed to shipyard-api and covers all of api-domain
#     (removes the old PathPrefix(/openapi/v1) restriction so the REST API is reachable).
# We re-source .env first so the new vars set above are available.
set -a; source "${ENV_FILE}"; set +a

TRAEFIK_DYNAMIC="${INSTALL_DIR}/traefik/dynamic/shipyard.yml"

# Gather the values we need — fall back to derived defaults if not set.
T_DOMAIN="${DOMAIN:-}"
T_API_DOMAIN="${PATCH_API_DOMAIN:-${SHIPYARD__API_DOMAIN:-}}"
[[ -z "${T_API_DOMAIN}" && -n "${T_DOMAIN}" ]] && T_API_DOMAIN="api-${T_DOMAIN}"
T_REGISTRY_DOMAIN="${PATCH_REGISTRY_DOMAIN:-${SHIPYARD__REGISTRY__HOSTNAME:-}}"
[[ -z "${T_REGISTRY_DOMAIN}" && -n "${T_DOMAIN}" ]] && T_REGISTRY_DOMAIN="registry.${T_DOMAIN}"
T_ENTRYPOINT_HTTPS="${SHIPYARD__TRAEFIK__ENTRYPOINT_HTTPS:-websecure}"
T_ENTRYPOINT_HTTP="${SHIPYARD__TRAEFIK__ENTRYPOINT_HTTP:-web}"
T_CERT_RESOLVER="${SHIPYARD__TRAEFIK__CERT_RESOLVER:-letsencrypt}"
T_PROTOCOL="${PROTOCOL:-https}"
T_STATIC_EP="${T_ENTRYPOINT_HTTPS}"
[[ "${T_PROTOCOL}" != "https" ]] && T_STATIC_EP="${T_ENTRYPOINT_HTTP}"
T_STATIC_TLS=""
[[ "${T_PROTOCOL}" == "https" ]] && T_STATIC_TLS="      tls: {}"

if [[ -z "${T_DOMAIN}" ]]; then
    warn "DOMAIN not set — skipping shipyard.yml rewrite (Traefik config unchanged)"
elif [[ -z "${T_API_DOMAIN}" || -z "${T_REGISTRY_DOMAIN}" ]]; then
    warn "Could not determine API/registry domain — skipping shipyard.yml rewrite"
else
    info "Rewriting ${TRAEFIK_DYNAMIC} with new registry + API routers..."
    # Back up the existing file before overwriting.
    cp "${TRAEFIK_DYNAMIC}" "${TRAEFIK_DYNAMIC}.bak.$(date +%Y%m%d%H%M%S)" 2>/dev/null || true

    cat > "${TRAEFIK_DYNAMIC}" <<TRAEFIK_CONF
http:
  routers:
    shipyard-frontend:
      rule: "Host(\`${T_DOMAIN}\`)"
      entryPoints: [${T_ENTRYPOINT_HTTPS}]
      service: shipyard-frontend
      tls:
        certResolver: ${T_CERT_RESOLVER}

    shipyard-api:
      rule: "Host(\`${T_API_DOMAIN}\`)"
      entryPoints: [${T_ENTRYPOINT_HTTPS}]
      service: shipyard-backend
      tls:
        certResolver: ${T_CERT_RESOLVER}

    shipyard-registry:
      rule: "Host(\`${T_REGISTRY_DOMAIN}\`)"
      entryPoints: [${T_ENTRYPOINT_HTTPS}]
      service: shipyard-backend
      middlewares:
        - shipyard-registry-prefix
      tls:
        certResolver: ${T_CERT_RESOLVER}

    shipyard-fn-invoke:
      rule: "Host(\`${T_DOMAIN}\`) && PathPrefix(\`/fn/\`)"
      entryPoints: [${T_ENTRYPOINT_HTTPS}]
      service: shipyard-backend
      tls:
        certResolver: ${T_CERT_RESOLVER}

    shipyard-mqtt:
      rule: "Host(\`${T_DOMAIN}\`) && PathPrefix(\`/mqtt\`)"
      entryPoints: [${T_ENTRYPOINT_HTTPS}]
      service: shipyard-mqtt
      middlewares:
        - shipyard-mqtt-strip
      tls:
        certResolver: ${T_CERT_RESOLVER}

    static-sites:
      rule: "HostRegexp(\`.+\`)"
      priority: 1
      entryPoints: [${T_STATIC_EP}]
      service: nginx-static
${T_STATIC_TLS}

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
    shipyard-registry-prefix:
      addPrefix:
        prefix: "/registry"

    shipyard-mqtt-strip:
      stripPrefix:
        prefixes:
          - "/mqtt"

    shipyard-error-pages:
      errors:
        status:
          - "404"
        service: nginx-static
        query: "/_errors/{status}.html"
TRAEFIK_CONF

    success "shipyard.yml rewritten — Traefik will hot-reload automatically"
    info "  API domain:      ${T_API_DOMAIN}"
    info "  Registry domain: ${T_REGISTRY_DOMAIN}"
    info "  Backup saved to: ${TRAEFIK_DYNAMIC}.bak.*"
fi

# ── Pull edge runtime image now so it's ready for first deploy ─────────────────
if [[ -n "${EDGE_RUNTIME_IMAGE:-}" ]]; then
    info "Pulling edge runtime image: ${EDGE_RUNTIME_IMAGE}..."
    docker pull "${EDGE_RUNTIME_IMAGE}" || warn "Pull failed — will retry on next update"
fi

# ── Done ──────────────────────────────────────────────────────────────────────
# Re-source one final time so summary reflects all newly written vars.
set -a; source "${ENV_FILE}"; set +a

echo ""
echo -e "${GREEN}${BOLD}Patch applied successfully.${RESET}"
echo ""
echo -e "  ${BOLD}API domain${RESET}           = ${SHIPYARD__API_DOMAIN:-<not set>}"
echo -e "  ${BOLD}Registry domain${RESET}      = ${SHIPYARD__REGISTRY__HOSTNAME:-<not set>}"
echo -e "  ${BOLD}Registry storage${RESET}     = ${SHIPYARD__REGISTRY__STORAGE:-local}"
echo -e "  ${BOLD}Edge runtime image${RESET}   = ${EDGE_RUNTIME_IMAGE:-<not set>}"
echo -e "  ${BOLD}Runtime API URL${RESET}      = ${SHIPYARD__EDGE_FUNCTIONS__RUNTIME_API_URL:-<not set>}"
echo -e "  ${BOLD}SCRIPTS_URL${RESET}          = ${SCRIPTS_URL:-<not set>}"
echo ""
if [[ -z "${SCRIPTS_URL:-}" ]]; then
    warn "SCRIPTS_URL is empty — update.sh will not self-update automatically."
    warn "Set it in ${ENV_FILE} to enable auto-updates:"
    warn "  SCRIPTS_URL=https://<your-landing-domain>"
fi
echo ""
echo -e "  ${BOLD}Next steps:${RESET}"
echo -e "    1. Point DNS for ${SHIPYARD__REGISTRY__HOSTNAME:-registry.<domain>} → this server"
echo -e "    2. Point DNS for ${SHIPYARD__API_DOMAIN:-api-<domain>} → this server (if not already)"
echo -e "    3. Run ${BOLD}${INSTALL_DIR}/update.sh${RESET} to restart containers with new images"
echo ""
