#!/usr/bin/env bash
set -euo pipefail

# ── Shipyard Dev Environment Launcher ──────────────────────────
# Usage: ./scripts/dev.sh [command]
#   start   - Start all dev infrastructure + backend
#   stop    - Stop all dev infrastructure
#   db      - Start only PostgreSQL
#   reset   - Reset database (drop and recreate)
#   logs    - Tail infrastructure logs

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
COMPOSE_FILE="$PROJECT_ROOT/infra/docker-compose.dev.yml"

cd "$PROJECT_ROOT"

# Load .env if it exists
if [ -f .env ]; then
    set -a
    source .env
    set +a
fi

case "${1:-start}" in
    start)
        echo "🚀 Starting Shipyard dev environment..."

        # Ensure Docker Swarm is active — overlay networks (required for Swarm services) need it
        if ! docker info --format '{{.Swarm.LocalNodeState}}' 2>/dev/null | grep -q "active"; then
            echo "  Initializing Docker Swarm..."
            ADVERTISE_ADDR=$(hostname -I 2>/dev/null | awk '{print $1}' || echo "127.0.0.1")
            docker swarm init --advertise-addr "${ADVERTISE_ADDR}" || true
        fi

        # Ensure platform_proxy is an overlay network. If it was previously created as a
        # bridge (old docker-compose behaviour), Swarm will reject services that try to join it.
        EXISTING_DRIVER=$(docker network inspect platform_proxy --format '{{.Driver}}' 2>/dev/null || echo "")
        if [ "$EXISTING_DRIVER" = "bridge" ]; then
            echo "  Recreating platform_proxy as overlay (was bridge — Swarm services require overlay)..."
            docker compose -f "$COMPOSE_FILE" down 2>/dev/null || true
            docker network rm platform_proxy 2>/dev/null || true
        fi
        docker network create --driver overlay --attachable platform_proxy 2>/dev/null || true

        docker compose -f "$COMPOSE_FILE" up -d
        echo ""
        echo "Infrastructure running:"
        echo "  PostgreSQL: localhost:5432"
        echo "  Redis:      localhost:6379"
        echo "  MQTT:       localhost:1883 (TCP) / localhost:9001 (WebSocket)"
        echo "  Traefik:    localhost:80 (HTTP) / localhost:8080 (Dashboard)"
        echo ""
        echo "Starting backend..."
        export SHIPYARD__REDIS__URL="redis://localhost:6379"
        export SHIPYARD__DATA_DIR="$HOME/.shipyard/data"
        export SHIPYARD__TRAEFIK__DYNAMIC_CONFIG_DIR="$PROJECT_ROOT/infra/traefik/dynamic"
        mkdir -p "$SHIPYARD__DATA_DIR"
        mkdir -p "$PROJECT_ROOT/infra/traefik/dynamic"
        cd backend && cargo run --bin shipyard
        ;;
    stop)
        echo "🛑 Stopping Shipyard dev environment..."
        docker compose -f "$COMPOSE_FILE" down
        ;;
    db)
        echo "🗄️  Starting PostgreSQL..."
        docker compose -f "$COMPOSE_FILE" up -d postgres
        ;;
    redis)
        echo "🔴 Starting Redis..."
        docker compose -f "$COMPOSE_FILE" up -d redis
        ;;
    reset)
        echo "⚠️  Resetting database..."
        docker compose -f "$COMPOSE_FILE" down -v postgres
        docker compose -f "$COMPOSE_FILE" up -d postgres
        echo "Waiting for PostgreSQL to start..."
        sleep 3
        cd backend && cargo sqlx migrate run
        echo "✅ Database reset complete"
        ;;
    logs)
        docker compose -f "$COMPOSE_FILE" logs -f
        ;;
    *)
        echo "Usage: $0 {start|stop|db|redis|reset|logs}"
        exit 1
        ;;
esac
