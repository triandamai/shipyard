# Re-review: corrections to the I4/I6 fix wave

**Range:** `6aae799..330cfba` (HEAD) on `worktree-editor-ui-followups`
**Commits:** `baba6dd` (Findings 1–3), `330cfba` (Finding 4)
**Verified diff:** regenerated `git diff 6aae799 HEAD` myself and byte-compared it
against `/tmp/followups-corrections-review.diff` — **identical**, so the supplied
diff is complete and nothing was reviewed second-hand.

**Build/test (run in this checkout, from a forced-cold rebuild of the three
changed crates via `cargo clean -p shipyard-api -p shipyard-docker -p
shipyard-engine`):**

```
cargo build --workspace   → Finished in 39.39s, ZERO warnings
                            (shipyard-docker, shipyard-engine, shipyard-docker-worker,
                             shipyard-openapi, shipyard-api recompiled from scratch)
cargo test  --workspace   → 144 passed, 12 ignored (29 suites)
```

Matches the expected 144/12 exactly. All three `is_container_dead` tests and both
static-template tests were confirmed present and passing by name.

**Scope:** `baba6dd` touches only `manager.rs` + `docker/src/engine.rs`;
`330cfba` touches only `templates.rs` + `sandbox_probe.rs` — exactly what each
message claims, no scope creep. The six untracked `.md` report files
(`followups-review.md`, `i1-i2-i3-fix-report.md`, `i4-fix-report.md`,
`i4-corrections-report.md`, `i5-i6-fix-report.md`, `i6-completion-report.md`)
are all still `??` in `git status` — none leaked into either commit.

---

## Strengths

**Finding 1 — the missing-`container_id` chain genuinely closes.** Traced both
ends together, not in isolation:

- `manager.rs:93-99` — the `None` arm now *constructs* `Err(AppError::NotFound(...))`
  (a real variant, not a message), so the value handed to the classifier carries
  the meaning in its type.
- `manager.rs:566-572` — `Err(AppError::NotFound(_)) => true` fires
  unconditionally, on the variant, with no message inspection. So the sentinel
  can never be misclassified again the way the string sentinel was.
- Downstream is wired correctly too: `self_healing = true` →
  `manager.rs:130-140`'s claim query `OR ($2 AND sandbox_instances.status =
  'running')` fires → the row is claimed exactly once and reprovisioned. The
  `status='running'` + `container_id IS NULL` row can now actually self-heal.

**Finding 3 — the bollard pattern is correct, verified against bollard's own
source, not assumed.** I read `bollard-0.17.1/src/docker.rs:1140-1180`: every
non-2xx response is turned into `Error::DockerResponseServerError { status_code,
message }` *inside* `process_request`, with no wrapping layer above it —
`process_into_unit` (used by `remove_container`,
`bollard/src/container.rs:1386-1401`) and `process_into_value` (used by
`inspect_container`) both propagate it verbatim. `message` is the Docker JSON
`message` field with the envelope stripped. So
`Err(BollardError::DockerResponseServerError { status_code: 404, .. })` at
`engine.rs:1118` and `engine.rs:1337` is the right pattern at the right layer, and
nothing can prevent it from matching. Moving the decision here — the one place
that sees the real HTTP status — is the correct structural fix, not a workaround.

**The claimed message preservation holds.** `AppError`'s derive is
`#[error("Not found: {0}")]` (`common/src/error.rs:6-7`), so a 404 from inspect
now renders as `Not found: Container not found: No such container: <id>`, which
still contains `No such container`. The pre-existing, untouched string check at
`containers/mod.rs:244` therefore still classifies a gone container as gone. I
also swept for every other prose matcher: the three
`Err(AppError::Docker(ref msg)) if msg.contains("404")` arms in `resources/mod.rs`
(:745, :931, :1208) are on `remove_volume`/`remove_network`, and
`containers/mod.rs:366` is on `stop_container` — none of them see either changed
function, so the variant change breaks no existing `match` arm. (One residual
gap on this, see Minor 1.)

**First-ever provision and genuine-removal both still work.**
`provision_sandbox` (`manager.rs:275-285`): for a first-ever start, Docker 404s →
`engine.rs:1118` returns `Ok(())` → `?` never fires → falls straight through to
`create_container`. For a real self-heal, the container is removed, `Ok(())`, same
path. For a genuine removal failure (409 "removal already in progress", etc.) the
`?` fires and `start_sandbox`'s error branch re-inspects by name, preserving the
I4 behavior. Removing the happy-path dependence on Docker's prose on *every single
start* is the most valuable thing in this wave.

**Finding 2 is in the right branch and the COALESCE is right.**
`manager.rs:165-169` builds `still_running` as
`.ok().filter(|d| d.state == "running")`, so `Some(_)` means *confirmed running* —
the new `UPDATE` (`manager.rs:193-202`) is inside that arm only, never the `None`
/"stopped" arm. `COALESCE(last_heartbeat_at, NOW())` sets a timestamp only when
NULL and preserves a valid one. Both columns are nullable `TIMESTAMPTZ`
(`migrations/20250101000053_sandbox_apps.sql:28-29`), and the reaper
(`reaper.rs:20-25`) requires `last_heartbeat_at IS NOT NULL`, so the row is now
reachable by the reaper — the leak this branch exists to prevent is actually
prevented. The concurrent-delete race is benign: `services` delete cascades the
row away, the `UPDATE` then matches 0 rows (no error), and it is `.ok()`-guarded
anyway.

**Tests exercise real behavior, not tautologies.** The two updated
`is_container_dead` tests and the new
`is_container_dead_treats_a_missing_container_id_as_dead` (`manager.rs:632-644`)
each assert a distinct *branch* of the classifier: `NotFound` → dead regardless of
message text, `Docker(...)` → alive, `Ok("exited")` → dead, `Ok("running")` →
alive. The new test locks the exact invariant the caller depends on. Notably the
transient-error test would fail under a naive "any Err is dead" regression, which
is the bug it guards.

**Finding 4 — the constant really is shared.** `STATIC_DEV_CMD` is defined exactly
once (`sandbox_probe.rs:129`; a repo-wide grep finds no second definition and no
surviving copy of the literal), and both consumers reference *that symbol*:
`detect_stack` at `sandbox_probe.rs:198` and `template_runtime` at
`templates.rs:179`. Heredoc delimiter re-verified in the one shared definition:
`<<'NGINX_EOF'` — quoted, so `$uri` survives to nginx. The doc comment
proactively flags the `listen 8080` / port coupling. The claim about
`detects_static_from_index_html_only` is genuinely true, not assumed — I read
`sandbox_probe.rs:80-85`: it asserts `runtime`, `install_cmd`, `port` only, never
`dev_cmd`, so no update was needed there. The const also lives in the right crate:
`shipyard-api` already depends on `shipyard-engine` (non-dev dependency), and
`engine` does not depend on `api`, so there is no cycle and no inversion.

---

## Issues

### Critical (Must Fix)

None.

### Important (Should Fix)

**1. `backend/crates/api/src/sandbox_runtime/templates.rs:63` — the restored
`assert_eq!` only tests half the sharing; a revert of `detect_stack`'s side still
passes all 144 tests.**

```rust
assert_eq!(dev, shipyard_engine::sandbox_probe::STATIC_DEV_CMD);
```

`template_runtime` *returns* `STATIC_DEV_CMD`, so this is literally
`assert_eq!(STATIC_DEV_CMD, STATIC_DEV_CMD)`. It does catch one direction: if
someone re-inlines a diverging literal into `template_runtime`'s Static branch,
the test fails. But the test never touches `detect_stack`, so the *other*
direction — the one that actually happened, and that this whole commit exists to
fix — is still untested. I verified the consequence: reverting
`sandbox_probe.rs:198` to `dev_cmd: "nginx -g 'daemon off;'".to_string()` breaks
no test in the workspace (the only static-detection test never asserts `dev_cmd`,
and the exact-match test only looks at the template path).

Why it matters: the third review's complaint was precisely that the test had been
weakened *because* an exact match would have caught the divergence. The restored
assertion is strictly better than `contains()`, but it still cannot catch a
regression on the probe path — the more commonly taken path of the two, per this
commit's own message.

Fix (3 lines, in the same test, and it makes the test's name honest):

```rust
let probe = shipyard_engine::sandbox_probe::ProbeResult {
    has_index_html: true, ..Default::default()
};
let detected = shipyard_engine::sandbox_probe::detect_stack(&probe).unwrap();
assert_eq!(dev, detected.dev_cmd, "template and probe paths must not diverge");
```

That compares the two *paths* rather than a constant to itself, and fails on a
change to either one. (`ProbeResult` and `detect_stack` are both `pub`, so no
visibility change is needed.)

### Minor (Nice to Have)

**2. `backend/crates/docker/src/engine.rs:1337-1339` — the new message drops the
`404` token that bollard's `Display` used to carry, narrowing the pre-existing
caller's fallback from two substrings to one.**

Old rendered text: `inspect_container failed: Docker responded with status code
404: No such container: x` — contained *both* `No such container` **and** `404`.
New text: `Not found: Container not found: No such container: x` — contains only
the first. `containers/mod.rs:244` is written as
`msg.contains("No such container") || msg.contains("404")`, i.e. the `404` arm was
the belt to the braces. If a 404 ever arrives with an empty or differently-phrased
body (a proxy in front of the socket, a non-JSON error page — bollard falls back
to the raw body text at `docker.rs:1167-1171`, and to an empty string for an empty
body), that handler now takes the `else` branch and *refuses* the delete with
"stop it before deleting the record", where it previously allowed it. Low
probability, but it is a behavior narrowing introduced by this commit, not a
pre-existing one. Cheapest fix — keep the token in the text:
`AppError::NotFound(format!("Container not found (404): {message}"))`. Better
still (separate cleanup): have `containers/mod.rs:240-247` match
`Err(AppError::NotFound(_))` by type now that the type exists, which is the whole
point of this commit.

**3. `backend/crates/api/src/sandbox_runtime/manager.rs:193-202` — the new
heartbeat `UPDATE` has no status guard, unlike the real heartbeat path.**

`heartbeat()` at `manager.rs:431` is deliberately written
`... WHERE service_id = $1 AND status = 'running'`. The new query is
`WHERE service_id = $1` only. If a concurrent `stop_sandbox` lands between the
`upsert_instance_status(..., "running", ...)` at :176 and this `UPDATE`, the result
is a `status='stopped'` row with a non-NULL `last_heartbeat_at`/`started_at` —
which `stop_sandbox` (`manager.rs:418`) explicitly NULLs on purpose. It is not a
leak (the reaper only looks at `status='running'`), just stale state that the
`SandboxInstanceRow` serializes to the UI. Adding `AND status = 'running'` costs
nothing and matches the existing convention. (The larger race — the `"running"`
upsert itself overwriting a concurrent stop — is pre-existing and out of scope.)

**4. `backend/crates/api/src/sandbox_runtime/manager.rs:275-285` — the new comment
asserts more than the code can know, and the blanket `Conflict` mapping is now
more visible.**

> "any `Err` reaching here is a genuine failure to remove a container that does
> exist"

That is true for 409/500 from the daemon, but false for a transport failure
(daemon down, socket timeout, TLS error) — in which case no container may exist at
all, yet the caller reports HTTP **409 Conflict** with "Could not clean up existing
sandbox container before restart". The blanket mapping is pre-existing behavior, so
this is not a regression, but the comment now claims a precision the code does not
have — and the fix wave's whole theme is not over-trusting one signal. Consider
propagating the engine error unchanged (`?` with no `map_err`) and letting
`AppError::Docker` → 500 stand for transport failures, or narrowing the `Conflict`
wrap to `AppError::Docker(msg)` cases the daemon actually answered. At minimum,
soften the comment.

**5. No migration for `sandbox_app_configs.dev_cmd` rows already written with the
broken static command.** `manager.rs:253-268` inserts the config once, on first
start (`if config.is_none()`), and a repo-wide grep finds **no** `UPDATE
sandbox_app_configs` anywhere — so a static sandbox provisioned before `330cfba`
keeps `nginx -g 'daemon off;'` in the DB forever and stays broken even after this
deploy. Almost certainly fine (the whole sandbox runtime is gated behind
`state.config.sandbox.enabled` and looks pre-release), but it should be a
deliberate call rather than an oversight. A one-line data migration would close it:
`UPDATE sandbox_app_configs SET dev_cmd = <new literal> WHERE runtime = 'static'
AND dev_cmd = 'nginx -g ''daemon off;''';`

**6. Two cosmetic leftovers in `engine.rs`.** (a) `get_container_ip`
(`engine.rs:1426-1431`) makes the same `inspect_container` call and still maps a
404 to `AppError::Docker`, so the crate now has two inspect paths with two error
types for the same condition — harmless today only because the function has zero
callers (dead code). (b) `use bollard::errors::Error as BollardError;`
(`engine.rs:20`) is appended after the alphabetically-ordered `bollard::*` import
block instead of into it.

**7. Informational — public API status code change.** `GET
/services/:id/containers/:id/inspect` (`containers/mod.rs:303-334`) propagates the
engine error directly, so a gone container now yields HTTP **404 / `NOT_FOUND`**
instead of **500 / `DOCKER_ERROR`**. That is an improvement, and
`inspectContainer` in `frontend/src/lib/api/client.ts:496` has no call sites, so
nothing breaks — just worth knowing it is an observable contract change that
neither commit message mentions.

---

## On the two deliberately deferred items

Both deferrals look reasonable to me.

- **argv ceiling (~128KiB → ~96KiB saveable file size).** Correct call. Every real
  fix is a product decision (lower the advertised cap / chunk / stream via stdin),
  and bolting one onto a regression-correction wave is how waves grow. It should
  get a tracked follow-up, since a silent failure at ~96KiB is a worse UX than a
  documented cap.
- **8-hex container-name collision in the by-name re-inspect.** Reasonable to
  defer, but I'd rate it slightly above "theoretical": the `Some(detail)` branch
  writes `detail.id` into `sandbox_instances.container_id` (`manager.rs:170-181`),
  so a collision does not merely mis-detect liveness — it records *another
  service's* container id on this row, which the next `start_sandbox` will then
  inspect and, on a self-heal, remove. Labels are indeed the right fix and are
  properly out of scope here; the follow-up is worth filing with that consequence
  spelled out rather than as a pure naming nit.

---

## Assessment

**Ready to merge?** With fixes

**Reasoning:** Findings 1–3 are fixed correctly and structurally — I verified the
bollard variant against bollard's own source, the full `None`-`container_id` chain
end to end, the `Display` text the untouched string-matching caller still depends
on, and that the new heartbeat `UPDATE` sits in the confirmed-running branch; the
build is warning-free from cold and tests are 144/12 as predicted. The one thing
worth holding for is the 3-line test strengthening in Issue 1: as written, the
restored `assert_eq!` compares a constant to itself and would not fail if someone
re-broke `detect_stack`'s static branch — the exact regression this round exists to
prevent. Everything else is Minor and could ship as follow-ups.
