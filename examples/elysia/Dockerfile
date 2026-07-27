# ─── Builder Stage ──────────────────────────────────────────────────────────
FROM oven/bun:1 AS builder
WORKDIR /app

# Install dependencies first (leverage layer caching)
COPY package.json ./
RUN bun install --frozen-lockfile

# Copy codebase and compile/minify to a single javascript bundle
COPY tsconfig.json ./
COPY src ./src
RUN bun run build

# ─── Runtime Stage ──────────────────────────────────────────────────────────
FROM oven/bun:1-slim AS runtime
WORKDIR /app

# Copy only the compiled single-file bundle from the builder stage
COPY --from=builder /app/dist/index.js ./index.js

# Set production defaults
ENV NODE_ENV=production
ENV PORT=3000

# Expose port
EXPOSE 3000

# Run with bun
CMD ["bun", "run", "index.js"]
