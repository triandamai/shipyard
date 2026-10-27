# Observability plan

Goal: Shipyard's own stack (Traefik, Postgres, core backend) and every tenant
app get tracing, durable logs (up to 90 days), traffic and resource history,
with sensitive data masked. Platform staff see everything in Admin → Logs;
organizations see only their own projects and services. Nomi's Monitor sheet
reads the same data through one integration endpoint.

## What exists today

- Container logs are streamed live from Docker (`docker/src/engine.rs`
  `container_logs`) and never stored. When a replica is replaced, its output
  is gone.
- `usage` is a live snapshot; nothing is kept.
- Traefik runs without an access log; the backend uses `tracing` to stdout.
- No request id crosses Traefik → backend → app.

## Principles

1. Capture at the edge of the host, not on demand: a collector follows
   containers continuously and writes to storage, so a crashed or replaced
   replica is still readable.
2. Mask before storing. Nothing unmasked is ever written to the log store.
3. One store, two scopes. Every row carries `org_id` (NULL for platform
   components); org endpoints filter by it, admin endpoints do not.
4. A metric Shipyard cannot measure is `null`, never `0`.
5. No new infrastructure for small installs: Postgres only. The storage trait
   keeps ClickHouse/Loki possible later.

## New crates

| Crate | Responsibility |
|---|---|
| `observability` (core) | Types, `LogStore`/`MetricStore` traits, level detection, query model, retention. No Docker or HTTP. |
| `observability-mask` (module in core, own tests) | Rule set + masker: bearer/API keys (`ship_int_`, `whsec_`, `ship_`), JWTs, `password=`/`secret=`/`token=` pairs, connection-string credentials, AWS/GCP key shapes, card-like numbers, optional emails. Org-defined extra patterns later. |
| `collector` | Docker log follower, Traefik access-log tailer, stats sampler. Depends on `docker`, `observability`. |

`api` only wires routes and permissions.

## Data model (Postgres, new migration)

- `log_entries` — partitioned by day (`ts`). Columns: `ts`, `org_id`,
  `project_id`, `service_id`, `deployment_id`, `source` (`app|traefik|postgres|backend|...`),
  `container_id`, `replica` (task slot), `stream` (`stdout|stderr`), `level`,
  `trace_id`, `span_id`, `message` (masked), `fields jsonb`.
  Indexes: `(service_id, ts desc)`, `(org_id, ts desc)`, `(trace_id)`, GIN
  trigram on `message` for keyword search.
- `request_rollups_1m` — per env per minute: `requests`, `s2xx..s5xx`, latency
  histogram (fixed log-scale buckets, mergeable → p50/p95 for any larger
  bucket), `bytes`.
- `visitor_hashes` — `(env_id, bucket, hash)` where
  `hash = sha256(daily_salt || ip || user_agent)` truncated; salt rotates at
  00:00 UTC and old salts are deleted, so hashes cannot be reversed or linked
  across days. Kept 8 days, pruned with the partitions.
- `resource_samples_1m` — per container per minute: cpu %, mem bytes, mem
  limit, plus 5 min and 1 h rollups (kept ≥ 7 days; raw minutes 48 h).
- `log_counts_1h` — per source/level for the "how many errors" tiles over 90
  days without scanning raw rows.
- Retention job (daily): drop partitions older than `observability.retention_days`
  (default 90, per-org override capped by plan), delete expired rollups.

## Collection

- **Docker logs**: subscribe to the Docker event stream; for every container
  with Shipyard labels attach `logs(follow, since=last_seen)`. Resume cursor in
  memory + `log_cursors` table so a backend restart loses at most seconds.
  Containers of the platform stack (traefik, postgres, backend, frontend)
  are tagged `org_id = NULL`.
- **Level**: parse JSON lines (`level`/`severity`/`lvl`, pino/zap/slog/tracing
  numbers); otherwise logfmt `level=`; otherwise stream (`stderr` → at least
  `warn`); never guess from words.
- **Trace id**: Traefik emits W3C `traceparent` (OTLP tracing) and logs it in
  the access log; the backend uses `tracing-opentelemetry` and
  `tower-http` request spans so every backend log line carries `trace_id`.
  Tenant apps that echo `traceparent` in JSON logs get the same id parsed from
  `trace_id`/`traceId` fields. Admin UI links a trace id to "all lines with
  this trace id" across stacks (Kibana-style).
- **Traefik access log**: JSON, tailed by the collector. Router/service name →
  env via the existing label scheme. Produces `request_rollups_1m`,
  `visitor_hashes`, and error log lines for 5xx.
- **Resources**: sampler reuses the `usage` code path every 60 s.
- **Backpressure**: bounded channel, batch insert (COPY / multi-row, 500 rows
  or 1 s), drop-oldest with a counted `dropped` metric when the DB is slow.

## APIs

Admin (`shipyard:admin:logs:read`): `GET /api/admin/logs` with `source`, `level`,
`q` (keyword), `from`, `to` (≤ 90 d), `trace_id`, cursor pagination;
`GET /api/admin/logs/stats` (counts by level/source per hour/day).
Org (`shipyard:<org>:logs:read`): same shape under `/api/orgs/:org/logs`, plus
`/projects/:p/logs` and `/services/:s/logs?replica=&deployment=&previous=true`
for crashed replicas. Server forces `org_id`; the client can never widen it.

Integration (manage key):
- `GET …/envs/{env}/metrics?range=1h|24h|7d&step=` — shape in the Nomi to-do.
  Bucket sets are fixed: 1h/1m, 24h/1h, 7d/1d for traffic; 1h/1m, 24h/5m,
  7d/1h for containers. `previous_totals` = the equal-length range before.
  Unmeasured fields are `null`.
- `GET …/services/{svc}/logs?format=json` → `{at, level, message, service}`;
  text format stays the default.
- Webhook `service.error_rate_alert` (5xx > threshold % over a window, default
  5 % / 5 min, debounced like `service.resource_alert`).
- `scripts/check-nomi-integration.py` and the Postman collection get the new
  calls; `docs/nomi.md` gets a "Metrics and logs" section.

## UI

- Admin → **Logs**: stack tabs (Traefik, Database, Backend, Frontend, All),
  level chips with live counts, range picker up to 90 d, keyword box, trace
  id chip, histogram of errors over time, virtualized list, line detail drawer
  with `fields` and "same trace".
- Org → project/service → **Logs** and **Metrics**: same components, scoped;
  replica switcher with "previous replicas" so crashed instances stay
  readable; deployment filter.
- Mask indicator: masked spans render as `••• [token]`.

## Delivery phases

1. `observability` crate: types, masker, level parser, query builder, tests
   (≥ 90 % line coverage on the crate; masker table-driven).
2. Migration + Postgres store + retention job.
3. Collector: Docker logs, backend `tracing` layer with trace ids, Traefik
   OTLP + access log config (bootstrap, `install.sh`, patch script).
4. Admin API + page.
5. Org/project/service API + pages (previous replicas).
6. Rollups, `metrics` integration endpoint, structured logs, error-rate alert,
   docs/Postman/readiness script.
7. Load test: 2 k lines/s sustained, search over 90 d p95 < 1.5 s on a
   30 GB dataset; tune indexes/partitions.

## Risks / open questions

- Postgres size at 90 d: budget ~1 KB/line; an install doing 500 lines/s is
  ~3.7 TB for 90 d, which is too much for Postgres. Mitigations: per-org line
  budget and sampling of `info` under pressure, retention caps per plan, and
  the store trait for a ClickHouse backend. Default install sets
  `max_gb` and drops oldest partitions first.
- Traefik access log field for the traceparent header must be verified on the
  pinned Traefik version.
- Docker log driver `json-file` rotation must be large enough that the
  collector never lags behind rotation; the collector follows by stream, not
  file, so this only matters while the backend is down.
- Privacy: visitor hash is not personal data only if the salt is destroyed
  daily; test covers that.
