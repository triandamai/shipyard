#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
#  Shipyard — Worker Node Setup
#
#  Sets up Docker on a fresh VPS and joins it to an existing Shipyard swarm.
#  Optionally configures registry credentials so the worker can pull private
#  images without extra steps.
#
#  Usage:
#    Interactive (recommended):
#      curl -fsSL https://<your-shipyard>/worker-setup.sh | sudo bash
#
#    Non-interactive (CI / automation):
#      sudo bash worker-setup.sh \
#        --token  SWMTKN-1-abc...xyz \
#        --manager 192.168.1.10:2377
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Colors ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; BOLD='\033[1m'; DIM='\033[2m'; RESET='\033[0m'

ok()   { echo -e "  ${GREEN}✔${RESET}  $*"; }
warn() { echo -e "  ${YELLOW}!${RESET}  $*"; }
err()  { echo -e "  ${RED}✖${RESET}  $*" >&2; exit 1; }
info() { echo -e "  ${CYAN}›${RESET}  $*"; }
step() { echo -e "\n${BOLD}${BLUE}── $* ──${RESET}\n"; }
dim()  { echo -e "  ${DIM}$*${RESET}"; }

# ── Read from /dev/tty so it works when piped via curl | bash ────────────────
ask() {
    local prompt="$1"
    local var_name="$2"
    local default_val="${3:-}"
    local secret="${4:-no}"

    eval "local cur=\${${var_name}:-}"
    if [[ -n "$cur" ]]; then return 0; fi

    local val=""
    local tty="/dev/tty"
    [[ ! -c "$tty" ]] && tty="/dev/stdin"

    if [[ "$secret" == "yes" ]]; then
        read -rsp "  ${YELLOW}?${RESET}  ${prompt} " val < "$tty"
        echo ""
    else
        read -rp "  ${YELLOW}?${RESET}  ${prompt} " val < "$tty"
    fi

    val="${val:-$default_val}"
    eval "${var_name}=\${val}"
}

yn() {
    local prompt="$1"
    local var_name="$2"
    local default="${3:-n}"
    ask "${prompt} [y/N]:" "$var_name" "$default"
    eval "local v=\${${var_name}:-n}"
    [[ "${v,,}" == "y" || "${v,,}" == "yes" ]]
}

# ── Argument parsing ──────────────────────────────────────────────────────────
SWARM_TOKEN=""
MANAGER_ADDR=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --token)   SWARM_TOKEN="$2"; shift 2 ;;
        --manager) MANAGER_ADDR="$2"; shift 2 ;;
        --help|-h)
            echo "Usage: $0 [--token SWMTKN-...] [--manager HOST:2377]"
            exit 0 ;;
        *) err "Unknown argument: $1" ;;
    esac
done

# ── Banner ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}  ┌──────────────────────────────────────────────────┐${RESET}"
echo -e "${BOLD}  │  ⚓  Shipyard — Worker Node Setup                 │${RESET}"
echo -e "${BOLD}  └──────────────────────────────────────────────────┘${RESET}"
echo ""
dim "This script installs Docker, configures registry credentials,"
dim "and joins this machine to your Shipyard swarm as a worker node."
echo ""

# ── Root check ────────────────────────────────────────────────────────────────
if [[ $EUID -ne 0 ]]; then
    err "Please run as root:  sudo bash $0"
fi

# ── Detect OS ─────────────────────────────────────────────────────────────────
OS_ID=""
if [[ -f /etc/os-release ]]; then
    # shellcheck source=/dev/null
    source /etc/os-release
    OS_ID="${ID:-}"
fi

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
            warn "Unknown OS — trying apt-get"
            apt-get install -y -q "$@" ;;
    esac
}

# ─────────────────────────────────────────────────────────────────────────────
step "Step 1 — Docker"

if command -v docker &>/dev/null; then
    DOCKER_VER=$(docker --version 2>/dev/null | grep -oP '\d+\.\d+' | head -1 || echo "?")
    ok "Docker ${DOCKER_VER} already installed"
else
    info "Docker not found — installing via get.docker.com..."
    install_pkg curl ca-certificates >/dev/null 2>&1 || true
    curl -fsSL https://get.docker.com | sh >/dev/null 2>&1
    systemctl enable --now docker >/dev/null 2>&1 || true
    ok "Docker installed"
fi

# Ensure current user's socket is accessible (needed if not root at runtime)
if ! docker info &>/dev/null; then
    err "Docker daemon is not running. Start it with: systemctl start docker"
fi
ok "Docker daemon is reachable"

# ─────────────────────────────────────────────────────────────────────────────
step "Step 2 — Swarm Connection Details"

dim "You can find these on your Shipyard dashboard:"
dim "Settings › Infrastructure › Join Tokens"
echo ""

ask "Manager address (e.g. 192.168.1.10:2377):" MANAGER_ADDR
[[ -z "$MANAGER_ADDR" ]] && err "Manager address is required"

ask "Worker join token (SWMTKN-1-...):" SWARM_TOKEN
[[ -z "$SWARM_TOKEN" ]] && err "Join token is required"

# ─────────────────────────────────────────────────────────────────────────────
step "Step 3 — Registry Credentials"

dim "Workers pull images directly from registries. Configure credentials"
dim "here so private images work without extra steps after joining."
echo ""

# ── Docker Hub ────────────────────────────────────────────────────────────────
DO_DOCKERHUB=""
if yn "Configure Docker Hub (docker.io)?" DO_DOCKERHUB; then
    ask "Docker Hub username:" DH_USER
    ask "Docker Hub password / access token:" DH_PASS "yes"
    if echo "$DH_PASS" | docker login registry-1.docker.io -u "$DH_USER" --password-stdin 2>/dev/null; then
        ok "Logged in to Docker Hub"
    else
        warn "Docker Hub login failed — skipping (you can retry later with: docker login)"
    fi
fi

# ── GitHub Container Registry ─────────────────────────────────────────────────
DO_GHCR=""
if yn "Configure GitHub Container Registry (ghcr.io)?" DO_GHCR; then
    dim "  Use a GitHub Personal Access Token (PAT) with 'read:packages' scope."
    dim "  Create one at: github.com/settings/tokens"
    echo ""
    ask "GitHub username:" GH_USER
    ask "GitHub PAT (ghp_...):" GH_TOKEN "yes"
    if echo "$GH_TOKEN" | docker login ghcr.io -u "$GH_USER" --password-stdin 2>/dev/null; then
        ok "Logged in to ghcr.io"
    else
        warn "ghcr.io login failed — skipping (you can retry later with: docker login ghcr.io)"
    fi
fi

# ── Custom Registry ───────────────────────────────────────────────────────────
DO_CUSTOM=""
if yn "Configure a custom/self-hosted registry?" DO_CUSTOM; then
    ask "Registry URL (e.g. registry.example.com):" CUSTOM_REG_URL
    ask "Username:" CUSTOM_REG_USER
    ask "Password / token:" CUSTOM_REG_PASS "yes"
    if echo "$CUSTOM_REG_PASS" | docker login "$CUSTOM_REG_URL" -u "$CUSTOM_REG_USER" --password-stdin 2>/dev/null; then
        ok "Logged in to $CUSTOM_REG_URL"
    else
        warn "Login to $CUSTOM_REG_URL failed — skipping"
    fi
fi

# ─────────────────────────────────────────────────────────────────────────────
step "Step 4 — Joining the Swarm"

# Leave existing swarm if any
SWARM_STATE=$(docker info --format '{{.Swarm.LocalNodeState}}' 2>/dev/null || echo "inactive")
if [[ "$SWARM_STATE" == "active" ]]; then
    warn "This node is already in a swarm — leaving first..."
    docker swarm leave --force >/dev/null 2>&1
    ok "Left previous swarm"
fi

info "Connecting to manager at ${MANAGER_ADDR}..."
if docker swarm join --token "$SWARM_TOKEN" "$MANAGER_ADDR"; then
    ok "Joined swarm successfully"
else
    err "Failed to join swarm. Check that:
      • The manager address is reachable from this node
      • Port 2377 (TCP) is open on the manager
      • The join token is correct and has not expired"
fi

# ─────────────────────────────────────────────────────────────────────────────
step "Step 5 — Verification"

NODE_ID=$(docker info --format '{{.Swarm.NodeID}}' 2>/dev/null || echo "")
NODE_ADDR=$(docker info --format '{{.Swarm.NodeAddr}}' 2>/dev/null || echo "")

if [[ -n "$NODE_ID" ]]; then
    ok "Node ID   : ${NODE_ID}"
    ok "Node addr : ${NODE_ADDR}"
    ok "Status    : active worker"
else
    warn "Could not read node info — run 'docker info' to verify"
fi

# ─────────────────────────────────────────────────────────────────────────────
echo ""
echo -e "  ${BOLD}${GREEN}Worker node is ready!${RESET}"
echo ""
dim "  The manager will start scheduling containers here automatically."
dim "  Check this node in Shipyard:  Settings › Infrastructure › Swarm Nodes"
echo ""
