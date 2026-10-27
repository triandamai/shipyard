#!/usr/bin/env bash
# Cut a release: bump the workspace version, commit, and tag vX.Y.Z.
# Pushing the tag builds and publishes X.Y.Z / X.Y / X images and
# creates the GitHub Release (see .github/workflows/docker.yml).
#
#   scripts/release.sh 1.4.0          # stable
#   scripts/release.sh 1.5.0-rc.1     # pre-release (exact tag only, no `latest`)
set -euo pipefail

v="${1:-}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "usage: $0 MAJOR.MINOR.PATCH[-pre]" >&2; exit 1; }
cd "$(dirname "$0")/.."
[[ -z "$(git status --porcelain)" ]] || { echo "working tree not clean" >&2; exit 1; }
git rev-parse "v$v" >/dev/null 2>&1 && { echo "tag v$v already exists" >&2; exit 1; }

sed -i.bak -e '/^\[workspace.package\]/,/^\[/{s/^version *= *".*"/version = "'"$v"'"/;}' backend/Cargo.toml && rm -f backend/Cargo.toml.bak
(cd backend && cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null)
git add backend/Cargo.toml backend/Cargo.lock
git commit -m "release: v$v"
git tag -a "v$v" -m "Shipyard $v"
echo "Created v$v. Push with: git push origin HEAD v$v"
