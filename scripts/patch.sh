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

append_if_missing "EDGE_RUNTIME_IMAGE" "${DOCKERHUB_USER}/shipyard-edge-runtime:${TAG_VALUE}"
append_if_missing "SCRIPTS_URL" ""

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

# ── Pull edge runtime image now so it's ready for first deploy ─────────────────
if [[ -n "${EDGE_RUNTIME_IMAGE:-}" ]]; then
    info "Pulling edge runtime image: ${EDGE_RUNTIME_IMAGE}..."
    docker pull "${EDGE_RUNTIME_IMAGE}" || warn "Pull failed — will retry on next update"
fi

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}${BOLD}Patch applied successfully.${RESET}"
echo ""
echo -e "  ${BOLD}EDGE_RUNTIME_IMAGE${RESET} = ${EDGE_RUNTIME_IMAGE:-<not set>}"
echo -e "  ${BOLD}SCRIPTS_URL${RESET}         = ${SCRIPTS_URL:-<not set>}"
echo ""
if [[ -z "${SCRIPTS_URL:-}" ]]; then
    warn "SCRIPTS_URL is empty — update.sh will not self-update automatically."
    warn "Set it in ${ENV_FILE} to enable auto-updates:"
    warn "  SCRIPTS_URL=https://<your-landing-domain>"
fi
echo ""
echo -e "  Run ${BOLD}${INSTALL_DIR}/update.sh${RESET} to deploy the new images."
echo ""
