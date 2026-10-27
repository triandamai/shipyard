# Release pipeline plan

## Today

- `docker.yml` runs on every push to `main` and pushes `latest` plus a 7-char
  SHA tag for backend, frontend, landing, edge-runtime.
- `settings/mod.rs::check_hub_update` lists Docker Hub tags, takes the newest
  7-hex tag and compares it with `SHIPYARD_GIT_SHA`.
- `install.sh` writes `BACKEND_IMAGE=repo:${TAG}` (default `latest`) to `.env`;
  `update.sh` re-pulls whatever tag is in `.env`.
- Workspace version is a constant `0.1.0`.

## Target

1. **A release is a git tag** `vMAJOR.MINOR.PATCH` (prereleases `v1.2.0-rc.1`).
   `main` pushes only run CI and publish `edge` + SHA tags (not `latest`).
2. `release.yml` on `v*` tags:
   - verifies the tag matches `backend/Cargo.toml` `[workspace.package] version`
     (the build fails otherwise, so `CARGO_PKG_VERSION` is always the release);
   - runs CI;
   - builds multi-arch images and pushes `X.Y.Z`, `X.Y`, `X` and `latest`
     (stable only; prereleases get only the exact tag) for every image;
   - creates a GitHub Release with generated notes and the install scripts
     attached.
3. **Updater uses versions, not hashes.** `/admin/version` reports `current`
   (from the build), `latest` (highest stable semver tag in the registry, or
   the GitHub latest release), `update_available` via semver comparison, and
   `release_url`. `/admin/update` rewrites `.env` image tags to the exact
   `latest` version (`repo:1.4.2`) before pulling, so installs are pinned and
   rollback is "set the previous version". `edge` channel opt-in via setting.
4. `install.sh` resolves the newest release at install time and pins it.
5. **Coverage**: each backend crate has tests; CI runs `cargo llvm-cov` per
   crate, writes a table to `$GITHUB_STEP_SUMMARY` (crate, lines %, regions %),
   uploads `lcov.info`, and fails when a crate falls below its floor
   (`backend/coverage-floors.toml`, ratcheted up over time). Frontend/landing
   get a vitest coverage line when tests exist.

## Steps

1. Pure semver helpers + tests in `common` (parse, compare, pick latest stable).
2. `check_hub_update` → version based; `/admin/update` pins tags; tests.
3. `release.yml`; change `docker.yml` to edge + sha.
4. `scripts/coverage-summary.py` + CI job + floors file.
5. Fill the crates that have zero tests with baseline unit tests.
6. Docs: release process in `docs/RELEASING.md`.
