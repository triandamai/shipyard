# Follow-up Wave Review — I1–I6

**Range:** `fce4a57..HEAD` (`310525c`, `9826889`, `6aae799`) on `worktree-editor-ui-followups`
**Files touched:** 5 (`files.rs`, `manager.rs`, `templates.rs`, `docker/engine.rs`, `docker/types.rs`) — no scope creep; each commit message accurately describes its own diff.
**Build/tests:** `cargo build --workspace` clean; `cargo test --workspace` → **143 passed / 12 ignored**, matching the expected count exactly.

## Verification performed

Beyond reading the diff, I reproduced the runtime behaviour against real containers (Docker 28.0.4):

| Claim | Result |
|---|---|
| base64 write round-trips byte-exactly (`echo '<b64>' \| base64 -d > f`) | ✅ 4 bytes for `"abc\n"`, 2 for `"xy"`, **0 for empty content** — identical on BusyBox (`alpine`) and coreutils (`python:3.12-slim`) |
| `head -c N -- file` portability | ✅ works on BusyBox; missing file → exit 1; directory → exit 1 (so `read_file`'s 404 path still fires) |
| `pip install -r <empty requirements.txt>` exits 0 | ✅ `PIP_EXIT=0` on `python:3.12-slim` (pip 25.0.1) — the `&& python app.py` chain is not broken by the now-empty requirements file |
| Python template container stays up | ✅ still `running` after 10 s, `GET /` → 200 custom page, `GET /nope.txt` → 404 via `super().do_GET()` |
| Static nginx config validity + serving `/app` | ✅ `nginx -t` "syntax is ok"; generated config exact; `GET /` serves the **seeded** `/app/index.html` (not nginx's default); `GET /missing.html` → 404; `$uri` survived unexpanded |
| Docker 404 phrasing (I4's string match) | ✅ Docker 28 returns `No such container: <name>` for **both** inspect and remove; bollard 0.17.1 Display is `Docker responded with status code {status_code}: {message}`, and `AppError::Docker` is `Docker error: {0}` — so `.to_lowercase().contains("no such container")` matches through the whole chain |
| `sh -c <script>` single-argv limit | ⚠️ **fails at ~131 072 bytes** (`exec /bin/sh: argument list too long`) — see Important #5 |

## Strengths

- **I1 is the right fix, not a patch.** Replacing the heredoc with base64 doesn't just remove the spurious `\n`; it deletes an entire bug class (delimiter collision) and the ~15 lines of randomization/collision-loop machinery that existed to paper over it. `general_purpose::STANDARD` is confirmed the correct engine choice — `A–Za–z0–9+/=` contains no shell metacharacter and, unlike `URL_SAFE`, no `-` that `echo` could mistake for an option flag. The new tests assert the round trip by *decoding* the emitted blob rather than string-matching the command shape, which is a materially stronger assertion than what they replaced.
- **I2 is correctly bounded and the boundary arithmetic is right.** `head -c {MAX_FILE_BYTES + 1}` caps the exec payload at the source, and `stdout_bytes.len() > MAX_FILE_BYTES` distinguishes exactly-at-cap (1 048 576 → accepted) from over-cap (1 048 577 → rejected) with no off-by-one.
- **I3's additive design is genuinely non-invasive and I confirmed it.** `exec_container_oneshot` sets `tty: Some(false)` (engine.rs:1867/1876), so the stream is properly demuxed and carries no CRLF translation — `stdout_bytes` really is byte-exact. `ExecOutput` derives only `Debug, Clone, PartialEq` (types.rs:248) with **no `Default` impl**, and there is exactly one production construction site (engine.rs:1911) plus one `impl DockerEngine for` (engine.rs:783), so nothing could have silently escaped the field addition. Every other `files.rs` handler (`mkdir`, `delete_entry`, `rename`, `write_file`) touches only `exit_code`/`stderr`; `tree` is the only other `stdout` consumer and did not need the strict-decode treatment.
- **I4's ordering is correct.** `provision_sandbox` bails out *before* `create_container` rather than after, so the 409-then-clobber sequence can't start; and the failure handler re-inspects by name rather than by the DB's just-nulled `container_id` (the claim query at manager.rs:125 sets `container_id = NULL`), which is the only id that is actually available at that point. The bail-out does **not** regress first-ever provisioning: `remove_container` on a nonexistent name returns a genuine `No such container` 404, which falls through as intended (verified against the live daemon).
- **I5/I6 both actually work**, verified end to end through the real `build_startup_command` wrapper, not just in unit tests. The multi-line static `dev_cmd` composes correctly with `build_startup_command`'s `…fi; {install_and_dev}` and with `format!("{install} && {dev}")` because `dev_cmd` is always appended last in both shapes, and `|` binds tighter than `&&` in POSIX sh so `mkdir -p '…' && echo … | base64 -d > …` parses as intended.
- `template_runtime` has exactly **one** non-test caller (`routes.rs:81`), so the plain-string → multi-line change has no other blast radius. `dev_cmd` reaches the frontend only as a type declaration (`frontend/src/lib/api/types.ts:516`) with no component rendering it.
- `<<'NGINX_EOF'` is quoted at both its occurrences in the single source string (templates.rs:180), and the new regression test locks that quoting down explicitly.

## Issues

### Critical (Must Fix)

None.

### Important (Should Fix)

**1. `manager.rs:93-99` — I4 introduced a new "permanently stuck running" state for a row with no `container_id`.**

```rust
let inspect_result = match &existing.container_id {
    Some(id) => state.docker.inspect_container(id).await.map(|d| d.state).map_err(|e| e.to_string()),
    None => Err("no container_id recorded".to_string()),   // <-- no longer "dead"
};
if !is_container_dead(&inspect_result) { return Ok(existing); }
```

Under the old `Err(_) => true`, a `status = 'running'` row with `container_id = NULL` was classified dead and self-healed. Now `"no container_id recorded"` does not contain `"no such container"`, so it is classified **alive** and `start_sandbox` returns the stale row. Nothing else can recover it: `exec_in_sandbox` (files.rs:39-40) then fails forever with `Internal("sandbox_instances row is 'running' with no container_id")`, and the reaper will at best call `stop_sandbox`, which no-ops on the container. This is a synthetic sentinel, not a Docker error, so it should never have been routed through the prose match. Fix: classify the `None` arm as dead directly, e.g.

```rust
let dead = match &existing.container_id {
    None => true,
    Some(id) => is_container_dead(&state.docker.inspect_container(id).await.map(|d| d.state).map_err(|e| e.to_string())),
};
```

**2. `manager.rs:167-182` — the restored `"running"` row is invisible to the idle reaper, which is the exact leak the fix's own comment claims to prevent.**

`upsert_instance_status(..., "running", Some(&detail.id), Some(&container_name))` writes status/ids but not `last_heartbeat_at` or `started_at`. `find_stale_sandboxes` (reaper.rs:20-26) requires:

```sql
status = 'running' AND last_heartbeat_at IS NOT NULL AND last_heartbeat_at < NOW() - …
```

So whenever `last_heartbeat_at` is NULL at that moment the container leaks **forever** — the `'starting'` branch no longer applies either, since the row is now `'running'`. Two reachable sequences:

- First-ever start: the claim `INSERT` creates the row with `last_heartbeat_at` NULL; `create_container` + `start_container` succeed, then `ensure_preview_domain` (manager.rs:323) fails on a DB error → handler sees a running container → writes `'running'` with NULL heartbeat.
- Start after a stop: `stop_sandbox` explicitly sets `last_heartbeat_at = NULL` (manager.rs:402), and the claim query doesn't restore it — same outcome on the next failed provision.

Fix: set the heartbeat when restoring, e.g. `SET …, last_heartbeat_at = COALESCE(last_heartbeat_at, NOW()), started_at = COALESCE(started_at, NOW())`, so the reaper has a clock to age the row against.

**3. `manager.rs:257` and `manager.rs:555` — both new code paths depend on a prose match against a message Docker does not contract, and one of them is on the happy path of every single sandbox start.**

`remove_container` **always** returns an error on a first-ever provision (the API returns 404 for `DELETE /containers/{name}?force=1`; confirmed against Docker 28). So `msg.to_lowercase().contains("no such container")` at manager.rs:257 is the only thing keeping normal app creation working. If Docker/bollard ever rephrases that string, or the daemon sits behind a proxy or localized error path, *every* sandbox start begins failing with `Conflict("Could not clean up existing sandbox container before restart: …")`. The symmetric failure on `is_container_dead` is a service stuck showing `"running"` that never self-heals — which is also the explicit answer to the review question "what if the container genuinely IS dead but the message doesn't contain that phrase": it never self-heals, and there is no reconciliation loop that would notice.

This is a fragile contract for two safety-critical branches. Fix it structurally in the engine layer where the HTTP status is actually visible — either map bollard's `DockerResponseServerError { status_code: 404, .. }` to `AppError::NotFound` in `inspect_container`/`remove_container` (engine.rs:1107-1113, 1313-1321), or simply make `remove_container` return `Ok(())` on a 404 (an idempotent delete is what all three call sites want: manager.rs:255, 367, 479). Callers then key off a type, not a sentence.

**4. `templates.rs:180` vs `engine/src/sandbox_probe.rs:182` — I6 fixed only half the paths, and the test was weakened rather than the invariant upheld.**

`detect_stack`'s static branch still returns the bare `dev_cmd: "nginx -g 'daemon off;'"`. Only apps created through `create_app` (which uses `template_runtime`) get the fixed config; any app whose stack is resolved by the probe (`provision_sandbox` → `probe_and_detect` → `detect_stack`, taken whenever `sandbox_app_configs` has no row, i.e. every non-template app with an `index.html`) still gets nginx's stock `/usr/share/nginx/html` root and still shows the default page. The bug report's own framing ("the Static template's nginx served its own bundled default page") is satisfied, but the underlying defect is not.

Compounding this: `template_runtime`'s doc comment (templates.rs:168-171) asserts it "matches the exact values `shipyard_engine::sandbox_probe::detect_stack` would infer for these stacks, so a template-created app behaves identically to a probe-detected one" — now false for `Static`. And `template_runtime_matches_stack_detector_conventions` was changed from `assert_eq!(dev, "nginx -g 'daemon off;'")` to two `contains` checks (templates.rs:63-64), which is precisely what stops the test from failing on the divergence it was named to protect. Fix: move the config-generating `dev_cmd` into `detect_stack`'s static branch (or a shared constant both call), restore `assert_eq!`, and delete or correct the doc comment.

**5. `files.rs:56` — base64 inflation shrinks the largest file that can actually be *saved* from ~128 KiB to ~96 KiB, against an advertised 1 MiB limit.**

The whole write script is passed as one argv element to `sh -c` inside the container, and Linux enforces `MAX_ARG_STRLEN` = 128 KiB per argument. Measured through `docker exec`: 131 000 bytes → `rc=0`; 140 000 bytes → `exec /bin/sh: argument list too long`. Because base64 is 4/3 expansion, the ceiling on file content drops from ~131 000 to ~98 200 bytes. Meanwhile `MAX_FILE_BYTES` is 1 MiB, `write_file` (files.rs:179) accepts up to 1 MiB, and `read_file` will happily *open* a 500 KB file — which then cannot be saved. The failure is loud, not silent (`exit_code != 0` → `Internal("Failed to write '…': exec /bin/sh: argument list too long")`), but the message is inscrutable and the asymmetry (open works, save doesn't) is a bad editor experience.

This is a pre-existing class made 25 % worse, so it need not block the merge, but it shouldn't be left undocumented. Options, cheapest first: (a) lower `MAX_FILE_BYTES` to something the write path can honour (~64 KiB) so the contract stops lying; (b) chunk the blob into successive `>>` appends under the limit; (c) the real fix — stream content via the exec's stdin (`attach_stdin`) instead of embedding it in argv, which removes the ceiling entirely.

**6. `manager.rs:160-181` — the by-name re-inspect is only as trustworthy as `sandbox_container_name`, which is a 32-bit truncation of the service UUID.**

`sandbox_container_name` is `format!("shipyard-sandbox-{}", &service_id.to_string()[..8])` (manager.rs:10-12) — 8 hex chars, so ~1 % collision probability by ~9 300 services and ~50 % by ~77 000. That truncation is a deliberate, documented convention across the file (manager.rs:18-33), so it's pre-existing. But the *consequence* is new: before this change, `container_id` in `sandbox_instances` always came from `create_container`'s own return value and therefore always belonged to this service. The failure handler now derives it from a name lookup, so on a collision it writes **another service's** `container_id` into this service's row — after which `exec_in_sandbox` (files.rs:42) runs the file API's read/write/delete commands inside that other service's container. Mitigation: `ContainerDetail` already exposes `labels` and `name`, so gate the restore on an identity check; the durable fix is to add a `shipyard.sandbox.service_id` label to `ContainerSpec` (types.rs:26-46 currently has no labels field) and require it to match before trusting the inspect result.

### Minor (Nice to Have)

**7. `manager.rs:541-552` — the stale doc paragraph was left above the new one, so the contract now states both positions.**

The pre-existing text ("Any inspect failure … is treated as dead rather than trusting the stale DB row") is still there, immediately followed by the new text saying the opposite. On a function whose whole point is this distinction, the first paragraph is the one someone will act on. Delete lines 541-545.

**8. `engine.rs:1895` — `stdout` is assembled from *per-chunk* lossy decodes, so valid UTF-8 split across a Docker frame boundary is mangled even now.**

`stdout.push_str(&String::from_utf8_lossy(&message))` runs once per frame; a multi-byte character straddling two frames becomes two U+FFFDs. `read_file` is now immune (it uses `stdout_bytes`), but `tree` still reads `output.stdout.lines()` (files.rs:115), so a non-ASCII filename could in principle come back corrupted. Now that `stdout_bytes` exists, the clean fix is one line: build `stdout` once at the end from the complete buffer (`String::from_utf8_lossy(&stdout_bytes).into_owned()`), which also drops the duplicated accumulation.

**9. `templates.rs:180` — `listen 8080;` is hardcoded rather than derived from the port element of the same tuple it sits in.** Changing the tuple's `8080` would leave nginx listening on the old port; the guard is only partial (`assert_eq!(port, 8080)` would fail, but `assert!(dev.contains("listen 8080;"))` would still pass). A `format!`/`const PORT` shared by both, or at least a comment tying them together.

**10. `templates.rs:119-134` — two small rough edges in the seeded `app.py`, both confirmed live.** `do_HEAD` is not overridden, so `HEAD /` bypasses the custom page and returns `SimpleHTTPRequestHandler`'s directory-listing headers (`Content-Length: 281`, `charset=utf-8`) while `GET /` returns the custom 181-byte page. And the `super().do_GET()` fallback serves everything under `/app`, including `app.py` itself (`GET /app.py` → 200) — the same exposure class as the Static template's nginx `root /app`, on a publicly-routed preview hostname, so worth being a deliberate decision rather than an accident. Also `TCPServer` (not `ThreadingTCPServer`) serialises requests; harmless here only because `SimpleHTTPRequestHandler` defaults to HTTP/1.0 with no keep-alive.

**11. No migration for `sandbox_app_configs` rows created before this wave.** Existing Python-template apps keep `dev_cmd = 'python manage.py runserver 0.0.0.0:8000'` next to a `manage.py` that still `sys.exit(0)`s, and existing Static apps keep the bare nginx command — `provision_sandbox` only inserts a config when none exists (manager.rs:228), so nothing re-derives them. Fine if nothing is live yet; otherwise an `UPDATE` migration is needed.

**12. `files.rs:56` — the write is still non-atomic.** `> '{full}'` truncates before `base64 -d` produces a byte, so an interrupted or failed write leaves a truncated file rather than the previous content. Same as the old heredoc, but worth noting in a wave explicitly about save corruption; `… > '{full}.tmp' && mv '{full}.tmp' '{full}'` would make saves atomic.

**13. Three untracked `i1-i2-i3-fix-report.md` / `i4-fix-report.md` / `i5-i6-fix-report.md` files sit in the worktree.** Not committed, so harmless — just make sure they don't get swept into the merge.

## Assessment

**Ready to merge?** With fixes

**Reasoning:** All six bugs are genuinely fixed and I verified I1, I2, I5 and I6 working against real containers, with test counts and commit hygiene exactly as claimed. But I4 traded one self-heal failure mode for two others — a `container_id`-less `"running"` row that can never recover (#1), and a restored `"running"` row with a NULL heartbeat that the reaper can never reap, reintroducing the very leak the fix's comment cites (#2) — and both are a few lines to fix. I6 is also only half done: `detect_stack` still hands probe-detected static apps the broken nginx command, and the test that should have caught that was relaxed from `assert_eq!` to `contains` (#4).
