#!/usr/bin/env bash
set -euo pipefail

# Shipyard Platform Backup Script
# Usage:   ./scripts/backup.sh [output-dir]
# Env vars: DATABASE_URL, BACKUP_DIR, BACKUP_RETAIN_DAYS

BACKUP_DIR="${1:-${BACKUP_DIR:-/opt/shipyard/backups}}"
RETAIN_DAYS="${BACKUP_RETAIN_DAYS:-7}"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
BACKUP_FILE="${BACKUP_DIR}/shipyard_${TIMESTAMP}.sql.gz"
DATABASE_URL="${DATABASE_URL:-postgres://shipyard:shipyard@localhost:5432/shipyard}"

mkdir -p "${BACKUP_DIR}"

echo "Shipyard Backup — $(date)"
echo "Target: ${BACKUP_FILE}"

pg_dump "${DATABASE_URL}" | gzip > "${BACKUP_FILE}"
echo "Database dump complete: ${BACKUP_FILE}"
echo "Size: $(du -sh "${BACKUP_FILE}" | cut -f1)"

if command -v find &>/dev/null; then
    PRUNED=$(find "${BACKUP_DIR}" -name "shipyard_*.sql.gz" -mtime +"${RETAIN_DAYS}" -print -delete 2>/dev/null | wc -l | tr -d ' ')
    echo "Pruned ${PRUNED} backup(s) older than ${RETAIN_DAYS} days"
fi

echo "Available backups:"
ls -lh "${BACKUP_DIR}"/shipyard_*.sql.gz 2>/dev/null || echo "  (none)"
