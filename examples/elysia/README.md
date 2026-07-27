# Elysia.js Shipyard Example

This is a high-performance backend API example built with [Elysia.js](https://elysiajs.com/) and [Bun](https://bun.sh/). It includes a sleek dashboard UI, Swagger/OpenAPI documentation, schema validation, CORS support, and is fully containerized.

## Features

- **Blazing Fast**: Powered by Bun's JavaScript engine and Elysia.js.
- **Visual Dashboard**: A beautiful dark-themed, glassmorphic UI served directly at `/` to monitor system stats and interactively trigger API endpoints.
- **Self-Documenting**: OpenAPI/Swagger UI automatically generated at `/swagger`.
- **Validation**: Schema safety enforced on the POST `/api/todos` endpoint using Elysia's type compiler.
- **Dockerized**: Optimized multi-stage Docker build producing an ultra-lean runtime container.
- **CI/CD Ready**: Pre-configured GitHub Actions workflow to build and push the Docker image to GitHub Container Registry (GHCR) or Docker Hub.

## Getting Started

### Prerequisites

You will need [Bun](https://bun.sh/) installed locally.

```bash
curl -fsSL https://bun.sh/install | bash
```

### Installation

Install dependencies:

```bash
bun install
```

### Running Locally

To start the development server with hot-reloading:

```bash
bun run dev
```

The server will be available at `http://localhost:3000`.

- Open [http://localhost:3000](http://localhost:3000) to view the interactive dashboard.
- Open [http://localhost:3000/swagger](http://localhost:3000/swagger) to explore the API docs.

### Building for Production

Compile and minify the TypeScript code into a single executable bundle:

```bash
bun run build
```

The compiled output will be generated at `./dist/index.js`.

---

## Containerization

### Build the Docker Image

You can build the Docker container locally using:

```bash
docker build -t elysia-example .
```

### Run the Docker Container

Run the built container and map it to port `3000` on your host:

```bash
docker run -p 3000:3000 elysia-example
```

---

## GitHub Actions Workflows

The repository includes a GitHub Actions workflow in [.github/workflows/elysia-docker.yml](file:///Users/triandamai/Projects/shipyard/.github/workflows/elysia-docker.yml) that automates building and pushing this Docker image.

### Pushing to GitHub Container Registry (GHCR)

By default, the workflow is configured to compile the project, build the Docker container, and push it to GHCR. This requires no extra registry credentials configuration as it utilizes the repository's built-in `GITHUB_TOKEN`.

Images are published under the namespace:
`ghcr.io/${{ github.repository_owner }}/elysia-example`

### Pushing to Docker Hub (Alternative)

If you prefer to push to Docker Hub, configure the following secrets in your repository:
- `DOCKERHUB_USERNAME`: Your Docker Hub username.
- `DOCKERHUB_TOKEN`: A Personal Access Token (PAT) for Docker Hub.

And update the `images` field in the workflow metadata to point to your Docker Hub repository.
