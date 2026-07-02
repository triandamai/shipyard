# Shipyard

A self-hosted PaaS for deploying and managing containerized applications on your own infrastructure using Docker Swarm and Traefik.

![License](https://img.shields.io/badge/license-MIT-blue.svg)

## Overview

Shipyard gives you a Heroku-like deployment experience on infrastructure you control. Connect your Git repositories, define your services, and Shipyard handles building images, routing traffic, and monitoring deployments — all from a visual topology canvas.

**Key features:**

- Visual service topology with real-time status updates (via MQTT WebSocket)
- Git-triggered deployments with parallel execution control
- Automatic Traefik routing with wildcard domain generation (nip.io / traefik.me)
- Docker Swarm orchestration with rolling deployments
- Role-based access control (org / project level)
- Deployment queuing when parallel limit is reached
- Dark/light theme

## Architecture

```
┌─────────────┐    ┌──────────────┐    ┌──────────────┐
│   Browser   │───▶│  SvelteKit   │───▶│  Rust/Axum   │
│             │    │  (Frontend)  │    │   (Backend)  │
│             │◀───│              │◀───│              │
│   MQTT WS   │    └──────────────┘    └──────┬───────┘
└─────────────┘                               │
        │                             ┌───────┼───────┐
        │                             │       │       │
        ▼                             ▼       ▼       ▼
  ┌──────────┐                  ┌─────────┐ ┌───┐ ┌────────┐
  │  rmqtt   │                  │Postgres │ │Git│ │Docker  │
  │  broker  │                  │   DB    │ │   │ │Swarm   │
  └──────────┘                  └─────────┘ └───┘ └────────┘
                                                       │
                                                  ┌────▼────┐
                                                  │ Traefik │
                                                  └─────────┘
```

**Backend crates** (`backend/crates/`):
| Crate | Purpose |
|---|---|
| `api` | Axum HTTP server, REST endpoints, deployment scheduler |
| `engine` | Deployment execution, Docker image build/push |
| `db` | SQLx migrations and query helpers |
| `docker` | Docker API client wrapper |
| `docker_worker` | Worker for Docker Swarm task polling |
| `git` | Git clone/pull operations |
| `traefik` | Traefik dynamic config generation |
| `mqtt_broker` | MQTT client for publishing events |
| `common` | Shared types, errors, config |

## Requirements

- Linux VPS (x86_64) with Docker installed
- Docker Swarm initialized (`docker swarm init`)
- A domain or public IP (nip.io wildcard DNS works out of the box)

## Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/triandamai/shipyard/main/scripts/install.sh | bash
```

The installer will:
1. Prompt for your domain, email (TLS), and admin credentials
2. Generate secure secrets (JWT, DB password, MQTT password)
3. Deploy the stack via `docker stack deploy`
4. Configure Traefik with automatic HTTPS

### Re-running the installer

The installer is idempotent — re-running it updates the deployment while preserving existing secrets.

## Configuration

All configuration lives in `/opt/shipyard/.env` after installation.

| Variable | Description |
|---|---|
| `DOMAIN` | Your base domain (e.g. `example.com`) |
| `ACME_EMAIL` | Email for Let's Encrypt TLS certificates |
| `SHIPYARD__AUTH__JWT_SECRET` | JWT signing secret |
| `SHIPYARD__DATABASE__URL` | PostgreSQL connection string |
| `SHIPYARD__MQTT__USERNAME` | MQTT broker username (for backend) |
| `SHIPYARD__MQTT__PASSWORD` | MQTT broker password (for backend) |

## Domain Generation

When adding a domain to a service, Shipyard auto-generates a subdomain:

- **VPS (public IP)**: `<name>.<public-ip>.nip.io` — resolves via [nip.io](https://nip.io) wildcard DNS, no DNS config needed
- **Localhost**: `<name>.traefik.me` — resolves to 127.0.0.1 via [traefik.me](https://traefik.me)

## MQTT Authentication

Real-time events are delivered over MQTT WebSocket. The broker (`rmqtt`) is configured to require authentication:

- **Backend services**: authenticate with a static username/password from environment
- **Browser clients**: authenticate using the user's JWT access token as the password
- **Port 1883** is not published to the host — MQTT is only accessible within the Docker network

## Docker Images

| Image | Description |
|---|---|
| `triandamai827/shipyard-backend:latest` | Rust API server |
| `triandamai827/shipyard-frontend:latest` | SvelteKit frontend |

## Development

### Prerequisites

- Rust 1.78+
- Node.js 20+
- Docker with Swarm mode
- PostgreSQL 15+

### Backend

```bash
cd backend
cp .env.example .env   # fill in DATABASE_URL etc.
cargo run -p api
```

### Frontend

```bash
cd frontend
cp .env.example .env
npm install
npm run dev
```

### Database migrations

```bash
cd backend
cargo run -p db --bin migrate
```

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you'd like to change.

1. Fork the repository
2. Create a feature branch (`git checkout -b feat/my-feature`)
3. Commit your changes
4. Push and open a pull request

## License

[MIT](LICENSE) — Copyright (c) 2025 Trian
