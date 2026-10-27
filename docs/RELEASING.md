# Releasing Shipyard

Releases are git tags. There is no `latest` image tag: installs and the
in-app updater always use an exact version, so the updater can tell whether it
is behind.

## Cut a release

```bash
scripts/release.sh 1.4.0        # bumps backend/Cargo.toml, commits, tags v1.4.0
git push origin HEAD v1.4.0
```

Pre-releases (`1.5.0-rc.1`) publish only the exact tag and are never offered
as an update to stable installs.

## What the tag does (`.github/workflows/docker.yml`)

1. Checks the tag equals the workspace version in `backend/Cargo.toml`.
2. Runs CI (tests, coverage).
3. Pushes `X.Y.Z`, `X.Y` and `X` for backend, frontend, landing and edge-runtime.
4. Creates the GitHub Release with generated notes and the install scripts.

Pushes to `main` publish `edge` and the short SHA only.

## How installs update

- `install.sh` resolves the newest stable release and writes
  `BACKEND_IMAGE=repo:X.Y.Z` (and frontend, edge runtime) to `.env`.
- Admin → Updates compares the running version with the highest stable
  `X.Y.Z` tag on the registry. Update rewrites `.env` to that exact version,
  pulls, and restarts. If no release can be found it changes nothing.
- Roll back by setting the previous version in `.env` and running update.sh.

## Coverage

CI prints a per-crate coverage table in the job summary and fails if a crate is
below its floor in `backend/coverage-floors.toml`. Raise floors when you add
tests; never lower them.
