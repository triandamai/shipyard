#!/usr/bin/env bash
# Check that a tenant org's edge network can't reach targets it shouldn't.
#
# Usage: check-tenant-isolation.sh <org-id> <host:port> [<host:port>...]
#
# Starts a throwaway container on the org's edge network
# (${LABEL_PREFIX:-platform}-edge-<org id without dashes>) and tries a TCP
# connect to each target. Every target is expected to be unreachable: pass
# other orgs' services and platform services (shipyard-mqtt:1883,
# shipyard-postgres:5432, ...). Exits 1 if any target answers.
set -euo pipefail

if [ $# -lt 2 ]; then
  echo "usage: $0 <org-id> <host:port> [<host:port>...]" >&2
  exit 2
fi

org="${1//-/}"
shift
network="${LABEL_PREFIX:-platform}-edge-${org}"

if ! docker network inspect "$network" >/dev/null 2>&1; then
  echo "network $network not found (tenant isolation on, and has the org deployed yet?)" >&2
  exit 2
fi

failed=0
for target in "$@"; do
  host="${target%:*}"
  port="${target##*:}"
  if docker run --rm --network "$network" busybox:1.36 \
      nc -z -w 3 "$host" "$port" >/dev/null 2>&1; then
    echo "FAIL  $target is reachable from $network"
    failed=1
  else
    echo "ok    $target is not reachable"
  fi
done

exit "$failed"
