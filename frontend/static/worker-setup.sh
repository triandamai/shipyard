#!/usr/bin/env bash
# Shipyard Worker Node Setup
# Installs Docker and joins this VPS to your Shipyard swarm as a worker node.
# Usage: curl -fsSL https://shipyard.trian.space/worker-setup.sh | sudo bash
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

banner() {
  echo ""
  echo -e "${BOLD}${CYAN}╔═══════════════════════════════════════╗${NC}"
  echo -e "${BOLD}${CYAN}║     Shipyard Worker Node Setup        ║${NC}"
  echo -e "${BOLD}${CYAN}╚═══════════════════════════════════════╝${NC}"
  echo ""
}

ok()   { echo -e "  ${GREEN}✓${NC} $*"; }
warn() { echo -e "  ${YELLOW}!${NC} $*"; }
err()  { echo -e "  ${RED}✗${NC} $*"; }
step() { echo -e "\n${BOLD}$*${NC}"; }

banner

# ── Root check ────────────────────────────────────────────────────────────────
if [ "$EUID" -ne 0 ]; then
  err "This script must be run as root (use sudo)."
  exit 1
fi

# ── OS detection ──────────────────────────────────────────────────────────────
if [ -f /etc/os-release ]; then
  . /etc/os-release
  OS_NAME="${NAME:-Linux}"
else
  OS_NAME="Linux"
fi
ok "OS: ${OS_NAME}"

# ── Docker installation ───────────────────────────────────────────────────────
step "1. Docker"
if command -v docker &>/dev/null; then
  ok "Docker already installed: $(docker --version)"
else
  warn "Docker not found — installing via get.docker.com..."
  curl -fsSL https://get.docker.com | sh
  systemctl enable docker
  systemctl start docker
  ok "Docker installed: $(docker --version)"
fi

# Ensure Docker daemon is running
if ! systemctl is-active --quiet docker; then
  warn "Docker daemon is not running — starting..."
  systemctl start docker
fi
ok "Docker daemon is running."

# ── Collect join details ──────────────────────────────────────────────────────
step "2. Swarm join details"
echo ""
echo "  Open your Shipyard dashboard → Settings → Infra → Join Tokens"
echo "  and copy the worker token and manager address shown there."
echo ""

# Support non-interactive mode by accepting env vars
if [ -z "${SWARM_TOKEN:-}" ]; then
  read -r -p "  Worker token: " SWARM_TOKEN
fi

if [ -z "${SWARM_TOKEN}" ]; then
  err "Token cannot be empty."
  exit 1
fi

if [ -z "${SWARM_MANAGER:-}" ]; then
  read -r -p "  Manager address (e.g. 1.2.3.4:2377): " SWARM_MANAGER
fi

if [ -z "${SWARM_MANAGER}" ]; then
  err "Manager address cannot be empty."
  exit 1
fi

# Append default Swarm port if none given
if [[ "$SWARM_MANAGER" != *":"* ]]; then
  SWARM_MANAGER="${SWARM_MANAGER}:2377"
fi

# ── Join the swarm ────────────────────────────────────────────────────────────
step "3. Joining swarm"
warn "Connecting to manager at ${SWARM_MANAGER}..."

if docker swarm join --token "$SWARM_TOKEN" "$SWARM_MANAGER"; then
  echo ""
  ok "${BOLD}This node has joined the Shipyard swarm!${NC}"
  echo ""
  echo "  It will appear in your Infra settings within a few seconds."
  echo "  You can now deploy services to this node from the Shipyard dashboard."
  echo ""
else
  err "Failed to join the swarm. Check that:"
  echo "    • The manager address and port are correct (default port: 2377)"
  echo "    • Port 2377/tcp is open between this node and the manager"
  echo "    • The worker token has not expired (regenerate from the dashboard)"
  exit 1
fi
