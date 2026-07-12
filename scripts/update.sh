#!/usr/bin/env bash
# Shipyard self-update script — run by the backend when a user triggers an update.
#
# This script runs inside the shipyard-backend container (Docker socket is mounted).
# Calling "docker compose up -d" directly would stop this container mid-execution.
# Instead, we spawn a detached docker:cli container that runs the restart after we exit.
# docker:cli is a tiny (~10 MB) official image that has docker + compose — unlike the
# backend image which is just a compiled Rust binary with no shell or docker CLI.
set -euo pipefail
INSTALL_DIR="/opt/shipyard"
cd "${INSTALL_DIR}"

# Source .env so we can read EDGE_RUNTIME_IMAGE and SCRIPTS_URL.
set -a; source "${INSTALL_DIR}/.env"; set +a

# ── Self-update ────────────────────────────────────────────────────────────────
# If SCRIPTS_URL is set, pull the latest update.sh from the landing site and
# replace this script before doing anything else, then re-exec.
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

# ── Pull compose images ────────────────────────────────────────────────────────
echo "[shipyard] Pulling latest images..."
docker compose pull

# ── Pull edge runtime image ────────────────────────────────────────────────────
if [[ -n "${EDGE_RUNTIME_IMAGE:-}" ]]; then
    echo "[shipyard] Pulling edge runtime image: ${EDGE_RUNTIME_IMAGE}..."
    docker pull "${EDGE_RUNTIME_IMAGE}" || true
fi

# ── Spawn detached updater ─────────────────────────────────────────────────────
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
