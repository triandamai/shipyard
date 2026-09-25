# Final whole-branch review — plan `2026-09-24-editor-ui`

**Range:** `26c89d3` → `025c546` (24 commits, 28 files, +2085/−13)
**Reviewer:** final merge gate, read-only on `/Users/triandamai/Projects/shipyard/.claude/worktrees/editor-ui`
**Method:** read the spec, the plan's task list, and the progress ledger in full; then read the whole diff in three passes (backend → frontend → the four found-bug fix commits); then verified every suspicion against the real current source (not the diff) and against already-merged sub-project-1 code. Independently re-ran the build and both test suites.

## Verification performed

| Check | Result |
|---|---|
| `cd backend && cargo build --workspace` | clean |
| `cd backend && cargo test --workspace` | **139 passed / 0 failed / 12 ignored** — matches the ledger's claimed state exactly |
| `cd frontend && npm run check` | **26 errors / 132 warnings** — matches the pre-existing baseline |
| errors/warnings in files this plan touched | **none** (the `$state` rune false-positive is in `ExecPanel.svelte:26`, pre-existing, not in `SandboxTerminal.svelte`) |
| migration ordering | `20250101000056_sandbox_seed_script.sql` is the highest; no collision; `ADD COLUMN IF NOT EXISTS ... TEXT` nullable, purely additive, backward compatible |
| all `ContainerSpec` construction sites | 3 total (`docker/src/engine.rs:642` test helper, `manager.rs:248`, `manager.rs:393`) — all updated. `ContainerSpec` has no `Default` impl and no site uses `..Default::default()`, so a missed site would be a hard compile error; the clean build is conclusive proof none was missed |

---

## Strengths

- **The WorkingDir fix is correct, minimal and structurally right.** `working_dir: None` on the probe container (`manager.rs:393`) is genuinely correct, not merely "left alone": `SANDBOX_PROBE_SCRIPT` (`engine/src/sandbox_probe.rs:100`) opens with an explicit `cd /app`, so it has never depended on the container's cwd. Choosing to add the field rather than prefixing every `install_cmd` with `cd /app &&` matches sub-project 1's implicit design contract and is the fix a careful engineer would have made without time pressure. Both new tests are meaningful (one asserts the field is mapped, one asserts `None` omits it), not tautological.
- **The heredoc-delimiter randomisation (`files.rs:48-68`) is a genuine correctness proof, not a probability argument.** The `loop` re-draws a fresh UUID until no line of `content` equals the candidate, which matches POSIX heredoc semantics exactly (the body ends at a line that is *exactly* the delimiter). The regression test feeds content containing the literal old delimiter and asserts the chosen one differs. This is better engineering than the plan asked for.
- **`validate_sandbox_path` (`files.rs:~260`) is in a correct final state and nothing later in the branch regressed it.** I re-derived the reasoning independently rather than trusting the ledger: every one of the six handlers calls it before touching a path, every handler then interpolates only into a *single-quoted* string prefixed with `/app/`, and in POSIX `sh` a literal `'` is the only byte that can terminate a single-quoted string (`$`, backtick, `\`, `;`, `|`, newline are all inert inside one). Rejecting `'`, `\0`, `\n`, `\r` and any `..` component therefore closes the injection surface completely for the current call sites. Four of the five destructive handlers additionally pass `--` before the path, so a leading `-` can't be read as an option (and can't arise anyway, since the `/app/` prefix comes first). The "`/etc/passwd` is indistinguishable from `src/index.js` at this layer" test is the right test and its comment documents the real contract.
- **The `create_app` authorization gate is in the right final state** (`routes.rs:41-48`): `require_project_permission(..., "service:write")`, matching the sibling `create_service`. I confirmed the coarser `require_project_access` appears nowhere in the new code. Getting this raised from org-membership to `service:write` matters more than it looks — this endpoint is a path to container shell access.
- **`exec_container_oneshot` (`docker/src/engine.rs:1519`) correctly separates stdout from stderr** by `LogOutput` variant, with `tty: Some(false)` so no PTY translation mangles bytes. The `Console` variant is folded into stdout, which is right for a non-TTY exec that may still emit it. Splitting the fields (rather than shipping the plan's own merged-field code) was the right call and the downstream consumers use the right field each time (content from `stdout`, diagnostics from `stderr`).
- **The Vite WS-proxy entry is correctly scoped and does not conflict.** `'^/api/apps/[^/]+/exec'` (`vite.config.ts:25`) cannot shadow the pre-existing `'^/api/projects/[^/]+/services/[^/]+/exec'` key (disjoint prefixes, order-independent), and cannot match the file API (`/api/apps/:id/files/...` has no `/exec` segment). It *does* also match `/api/apps/:id/exec/token`, but the byte-identical `bypass(req) { if (!req.headers.upgrade) return req.url; }` returns that POST to SvelteKit, so token minting is unaffected. This is exactly the right shape.
- **The atomic start claim is genuinely sound.** I traced `start_sandbox`'s `INSERT ... ON CONFLICT DO UPDATE ... WHERE status NOT IN ('starting','running') OR ($2 AND status='running') RETURNING` myself. Postgres re-evaluates that `WHERE` against the latest committed row version after taking the row lock, so *two concurrent self-heal callers cannot both claim*: the loser re-reads `'starting'` and both branches evaluate false. The ledger's parked finding is not this race (see Important #4).
- **Debug-quality comments throughout.** The BusyBox-vs-GNU `find` note (`files.rs:~90`), the `bollard::container::NetworkingConfig` vs `bollard::models::NetworkingConfig` note, and the heredoc-collision rationale are the kind of comments that save the next reader an hour. This is above the bar for the codebase.
- **Process quality was high.** Three real bugs found by actually running the thing, each root-caused empirically (isolated `docker run` repro for WorkingDir; `docker exec cat` to confirm on-disk truncation for the save race; a raw Node WebSocket client bypassing the dev server to prove the backend was innocent for the proxy bug) rather than guessed at. The `is_container_dead` "wrong field" theory being investigated and *discarded* after reading bollard-stubs' generated `Display` impl is exactly right.

---

## Issues

### Critical (Must Fix)

#### C1. `CodeEditor` never reacts to a changed `value`, so switching files silently overwrites the newly-opened file with the previously-opened file's contents

`frontend/src/lib/components/CodeEditor.svelte:93` + `frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/apps/[serviceId]/editor/+page.svelte:114-118`

`CodeEditor` reads `value` exactly once, inside `onMount`, to seed `EditorState.create({ doc: value, extensions })`. There is no `$effect` syncing the prop into the view; the only way to change the document is the exported imperative `setValue()`. Likewise `language` is only read once, via `languageCompartment.of(languageExtension(language))`; the only way to change it is the exported `setLanguage()`.

The editor route calls **neither**. It renders:

```svelte
{#if openPath}
  <CodeEditor value={fileContent} language={languageForPath(openPath)} onChange={saveFile} height="100%" />
```

with no `bind:this` and no `{#key openPath}`. Because `openPath` goes from one truthy string to another, the `{#if}` block is not torn down and the `CodeEditor` instance is **not** recreated. So:

1. Open `index.html` → editor shows `index.html`. Correct.
2. Click `package.json` in the file tree → `openFile()` sets `openPath = 'package.json'` and `fileContent = <package.json>`. The tab bar updates to `package.json`. **The editor keeps displaying `index.html`.**
3. Type one character → CodeMirror's `updateListener` fires `onChange(<index.html's doc>)` → `saveFile` reads `openPath`, which is now `package.json` → `api.writeSandboxFile(serviceId, 'package.json', <index.html content>)`.

`package.json` is destroyed, replaced by the contents of `index.html`, with a "Saving…" indicator and a 200 OK. The user's only signal is that the file they thought they were editing looks wrong, by which time the original is gone. Syntax highlighting is also stuck on the first file's language for the same reason.

This is the single most basic multi-file interaction in an IDE and it loses data. It was not caught by Task 15 because the live walkthrough only ever opened one file (`index.html`).

**Fix** (either is fine, both are small):

- In the editor route, add `bind:this={editorRef}` and in `openFile`, after setting state, call `editorRef?.setValue(content)` and `editorRef?.setLanguage(languageForPath(path))`. This uses the API `CodeEditor` already exports and matches how `setValue` was designed to be used. Guard against the resulting programmatic `docChanged` re-triggering `saveFile` (either suppress the next `onChange`, or accept it — a write of the just-read content is harmless but wasteful).
- Or wrap the component in `{#key openPath}` so it remounts per file. Simpler, but throws away undo history and scroll position on every file switch, and remounting CodeMirror per click is heavier.

The `bind:this` route is preferable. Either way, add a manual check: open file A, open file B, confirm B's content is displayed and that editing B writes to B.

#### C2. The "Static" template creates an app that can never start — and its starter files are never even written

`backend/crates/api/src/sandbox_runtime/templates.rs:131` (with `manager.rs:232-235`)

`template_runtime(Template::Static)` returns `install_cmd` as the **empty string** `""`. `create_app` (`routes.rs:~78`) binds that as a `&str`, so the column is `''`, not `NULL`, and `fetch_config` yields `install_cmd: Some("")`. `provision_sandbox` then does:

```rust
let install_and_dev = match &config.install_cmd {
    Some(install) => format!("{install} && {}", config.dev_cmd.as_deref().unwrap_or("")),
    None => config.dev_cmd.clone().unwrap_or_default(),
};
```

producing the container command `" && nginx -g 'daemon off;'"`. I verified against a real shell: this is a hard **syntax error, exit 2** (`syntax error near unexpected token '&&'`). Because `sh -c` parses the entire script before executing any of it, the `if [ -z "$(ls -A /app ...)" ]` seed step that `build_startup_command` prepends **also never runs** — so the volume isn't even populated, and every subsequent start fails identically. The Static template is a permanent brick, not a transient failure.

This is a contract violation the file's own doc comment asserts is not possible: *"matches the exact values `shipyard_engine::sandbox_probe::detect_stack` would infer for these stacks."* I checked `detect_stack` directly — for a static app it returns `install_cmd: None` (`engine/src/sandbox_probe.rs`, `has_index_html` branch), and the already-merged test `detects_static_from_index_html_only` asserts `stack.install_cmd == None`. The template path diverges from the detection path precisely where it claimed not to.

Worse, `templates.rs:58` (`assert_eq!(install, "")`) *codifies* the wrong value, which is why the full green suite gave no signal.

**Fix:** change `template_runtime`'s third element to `Option<&'static str>` (`None` for Static, `Some(..)` for Node/Python) and bind that, so the DB stores `NULL` and `provision_sandbox` takes its `None` branch. Update the test to assert `None`. A narrower alternative — normalising empty-to-`NULL` at the `create_app` bind site — works too, but leaves `template_runtime` still misdescribing itself. Whichever you pick, also harden `provision_sandbox:232` to treat `Some("")` like `None`, since a `shipyard.json` manifest with `"install": ""` can reach the same line through the already-merged path.

#### C3. The sandbox exec WebSocket performs no per-service authorization after decoding the token

`backend/crates/api/src/sandbox_runtime/exec.rs:75-97`

`exec_ws` decodes the JWT and extracts `user_id`, then upgrades. `handle_exec_socket` looks up the instance, checks `status == "running"`, checks a `container_id` exists, and calls `state.docker.exec_container(...)` — it **never calls `require_service_access`** (or any RBAC function) for the `service_id` in the path.

The token minted by `exec_token` is a **general-purpose access token** (`create_access_token(..., 300, vec![])`) — it carries no service or scope binding at all. So the signature check proves only "some valid Shipyard user"; it proves nothing about *this* service. Any authenticated user who knows (or leaks) a sandbox's `service_id` UUID can open `GET /api/apps/{other_tenants_service_id}/exec?token=<their own access token>` and receive an interactive `/bin/sh` inside another tenant's gVisor sandbox container — read and write the whole volume, exfiltrate whatever is in it. A user's ordinary long-lived access token works just as well as a minted one, so the 5-minute TTL provides no protection either.

Two honest qualifications, which I want to state plainly rather than bury:

- **This faithfully mirrors a pre-existing hole.** I read `services::exec_ws` / `handle_exec_socket` (`services/mod.rs:1767-1900`): it decodes the token and then checks only Swarm node locality and the `containers` table — no `rbac::` call anywhere in the socket handler. So this branch did not *introduce* the pattern; Task 5's brief said "mirror `services::exec_ws`" and the implementer did exactly that. The per-task review noted (correctly) an *improvement* over the mirrored pattern — `container_id` is derived server-side instead of trusted from the client — and missed that the authorization gap came along with it.
- Reaching it still requires an authenticated account plus a service UUID.

I am nonetheless calling it Critical rather than Important, for three reasons. The impact is cross-tenant root shell in a container — about the worst outcome a control plane has. UUIDs are not a security boundary: they appear in topology API responses, editor URLs, `svc_<id>` canvas node ids, and access logs. And the fix is one line in code this branch is adding, which makes "inherited, so not ours" a weak argument at a merge gate:

```rust
// in handle_exec_socket, before exec_container:
if require_service_access(&state.db, user_id, service_id).await.is_err() {
    // send the existing {"type":"error"} frame and close, same as the other guards
}
```

`require_service_access` is already imported in this exact file for `exec_token`, so this costs nothing. Please also fix `services::exec_ws` the same way — separately if you prefer, but track it; leaving it is not acceptable now that it's known. Secondarily, consider binding the minted exec token to the service (a claim, checked on connect) so the URL-borne token stops being a full API credential sitting in browser history and proxy logs.

---

### Important (Should Fix)

#### I1. Every file save appends an extra trailing newline, and it compounds once per editing session

`backend/crates/api/src/sandbox_runtime/files.rs:68`

```rust
format!("{mkdir_part}cat > '{full}' <<'{delimiter}'\n{content}\n{delimiter}\n")
```

A `<<` heredoc body is the lines up to (not including) the delimiter line, each terminated by a newline. The format string unconditionally inserts `\n` between `{content}` and the delimiter, so the body is always `content + "\n"`. I verified against a real shell (`od -c`): content `"abc\n"` lands on disk as `abc\n\n`.

Since essentially every well-formed text file ends in a newline, essentially every save adds a blank line. It compounds across sessions: save → disk has `…\n\n`; reload the editor → `cat` returns `…\n\n` → edit and save → `…\n\n\n`. The seeded `index.html` will visibly grow a blank line per open-edit-save cycle. Symmetrically, a file *without* a trailing newline can never be saved as such.

This directly contradicts the guarantee `exec_container_oneshot`'s own doc comment makes ("required for reading/writing file content without corruption") and undercuts the point of using a non-TTY exec at all. It is not caught by `write_command_uses_heredoc_with_validated_path_and_content`, whose `assert!(cmd_str.contains(content))` holds either way.

**Minimal fix:** only add the separator when needed —

```rust
let sep = if content.ends_with('\n') { "" } else { "\n" };
format!("{mkdir_part}cat > '{full}' <<'{delimiter}'\n{content}{sep}{delimiter}\n")
```

This makes the round-trip stable, though a heredoc still cannot express "no trailing newline".

**Better fix, which I'd recommend given `base64` is now already a dependency of this crate:** drop the heredoc entirely and write `echo '<base64(content)>' | base64 -d > '/app/{path}'`. That is byte-exact for *any* content, needs no delimiter search loop, and is immune to delimiter collision by construction — so it retires both this bug and the machinery added to work around the previous one. Keep the existing collision test as a regression guard on whichever path survives. (Watch `ARG_MAX` for a 1 MB file: base64 of 1 MB is ~1.37 MB, comfortably under the typical 2 MB limit but not by a wide margin — if you're uneasy, chunk it or keep the heredoc with the `sep` fix.)

#### I2. `read_file` buffers unbounded output before enforcing its 1 MB cap

`backend/crates/api/src/sandbox_runtime/files.rs:165-166`

The size check runs on `output.stdout` *after* `exec_in_sandbox` has already accumulated the entire `cat` output into a `String`. A user who creates a 2 GB file in `/app` (trivial — they have a terminal: `dd`, `yes >`, a runaway log) and then opens it in the editor makes the API process allocate 2 GB. `exec_container_oneshot` has no output ceiling of its own, and there is no timeout either. One authenticated user can OOM the control plane for every tenant on the node.

The spec's wording is *"read one file via `cat`, capped at 1 MB"* — the cap was meant to be part of the read, not a post-hoc rejection.

**Fix:** cap in the command, e.g. `head -c {MAX_FILE_BYTES + 1} -- '/app/{path}'` (BusyBox and GNU `head` both support `-c`), and keep the Rust-side check to turn the `MAX+1` case into the existing clear error. Consider also giving `exec_in_sandbox` a blanket output ceiling and a `tokio::time::timeout`, so no future handler can reintroduce this. `tree` is already bounded by `head -n`, which is the right shape — this is just the same idea applied to `read_file`.

#### I3. Non-UTF-8 file contents are silently mangled on read, and then destroyed on save

`backend/crates/docker/src/engine.rs:1551` + `files.rs:168`

`exec_container_oneshot` accumulates with `String::from_utf8_lossy`, so any non-UTF-8 byte becomes U+FFFD *before* `read_file` ever sees it. The binary guard (`output.stdout.as_bytes().contains(&0)`) then only catches content with NUL bytes — which is not the same thing:

- A Latin-1/CP-1252 text file, or a small binary with no NULs, passes the guard and is returned as mojibake.
- The editor displays the mojibake. The next keystroke saves it back — writing U+FFFD over the original bytes. The original is unrecoverable.

The spec asked for binary rejection *"rather than corrupting the editor"*; the lossy conversion defeats it for a real subset of inputs.

**Fix:** have the read path detect non-UTF-8 before conversion — e.g. return `Vec<u8>` (or a `bytes` variant) from the one-shot exec for this call site and `String::from_utf8(..)` fallibly, rejecting with the existing "File appears to be binary" error on failure. Cheaper interim mitigation: also reject if the decoded string contains U+FFFD, which is a decent proxy and a two-line change.

#### I4. My independent judgment on the parked concurrency finding: the severity call is right, the diagnosis is probably wrong, and the proposed fix would not have fixed it

`backend/crates/api/src/sandbox_runtime/manager.rs:498` (with `:92-104` and `:221`)

The ledger parks Task 15's third finding as "most likely two near-simultaneous `start_sandbox` calls racing inside `provision_sandbox`'s self-heal path … a proper fix (per-service async lock) deserves careful design." I traced this myself and reached a different conclusion.

**The concurrency race described is already closed.** The atomic claim at `:121-134` re-evaluates its `WHERE` against the latest committed row under the row lock. Two concurrent self-healers both compute `self_healing = true`, but after the first commits, the row reads `'starting'`: the loser's `status NOT IN ('starting','running')` is false *and* its `$2 AND status = 'running'` is false, so it gets no `RETURNING` row and takes the `claimed.is_none()` early return. A per-service async lock would add nothing.

**The much more likely cause is `is_container_dead`:**

```rust
fn is_container_dead(inspect_result: &Result<String, String>) -> bool {
    match inspect_result {
        Err(_) => true,
        Ok(state) => state != "running",
    }
}
```

*Every* `inspect_container` error is read as "the container is dead" — not just "no such container". Any transient Docker API failure (socket timeout, daemon momentarily busy under a gVisor start, connection reset) on a perfectly healthy container therefore sends `start_sandbox` down the self-heal path. That path then does `remove_container(&container_name, true).await.ok()` — **best-effort, error swallowed** — followed by `create_container`. This matches the observed symptom exactly, including the parts the race theory doesn't explain: `docker ps` and the DB agreed the container was running and healthy, and the failure was a 409 name conflict (i.e. the remove didn't take effect, then the create collided) rather than anything claim-shaped.

The consequences are worse than a failed page load:

- A healthy running sandbox is destroyed and recreated because of one flaky inspect, killing the user's dev server and any in-container state.
- When the remove fails and the create 409s, `provision_sandbox` errors, `start_sandbox` resets the row to `'stopped'` (`:152`) — while the real container is still running. Docker and the DB now disagree in the direction that the idle reaper (which looks for stale *running* rows) cannot correct, so the container leaks indefinitely, still holding CPU/memory against the node.

**Verdict on "minor, deferred, not blocking":** I agree it should not block *this* branch, for the specific reason that every line involved is already-merged sub-project-1 code — this branch touches `manager.rs` only to add `working_dir: Some("/app")` and the seed wrapper. Blocking a merge on a pre-existing defect in code already on `main` is the wrong gate. **But the ledger's reasoning for deferring is wrong, and that matters**, because it concluded "isn't a realistic path for a compiled production app" from the Vite-HMR trigger. If the cause is a transient inspect error, the trigger is ordinary infrastructure flakiness and it *will* recur in production — more often now, because this branch's editor route calls `POST /sandbox/start` on every mount and is the first caller this code path has ever had.

**Recommended follow-up (a separate ticket, not a merge blocker):** distinguish "container genuinely absent" from "inspect failed" — e.g. have `inspect_container` surface a typed not-found, and treat only that (plus a non-`running` state) as dead, while a transient error either retries or returns the existing row untouched. Separately, stop swallowing the `remove_container` result on the self-heal path: if the remove fails, don't attempt the create, and don't reset the row to `'stopped'` while a container is still up. Please carry this into the next sub-project's plan explicitly, with the corrected diagnosis, so it isn't re-derived from the wrong theory.

#### I5. The Python template creates an app that installs Django and then immediately exits

`backend/crates/api/src/sandbox_runtime/templates.rs:100-107` (`PYTHON_SEED_SCRIPT`)

The seeded `manage.py` is `print(...)` then `sys.exit(0)`, while `dev_cmd` is `python manage.py runserver 0.0.0.0:8000`. So the container runs `pip install -r requirements.txt && python manage.py runserver …`, the placeholder exits 0, and the container stops seconds after starting. The DB row says `running`; the preview is dead; the editor's file tree and terminal (which both require a running container) stop working shortly after boot.

The ledger notes this as Minor/deferred from Task 2's review. I'm raising it to Important because of the aggregate: with C2, **two of the three advertised templates do not produce a working sandbox**, and the third (Node) is the only one Task 15 exercised. The spec's entire justification for making a template mandatory is that it *"resolves the bootstrap ordering problem"* — a template whose container exits immediately doesn't resolve it. A user clicking "Python" gets exactly the stuck state the design was written to prevent.

**Fix:** make the Python starter actually serve. The smallest honest option is to drop the Django pretence — seed an `app.py` with a stdlib `http.server` bound to `0.0.0.0:$PORT` and an empty-ish `requirements.txt`, with `dev_cmd = "python app.py"` (which `detect_stack` already produces for `has_requirements_txt && !has_manage_py`, so the template stays consistent with the detection path). If Django is wanted, seed a real minimal project (`manage.py` + a settings module with `ALLOWED_HOSTS = ["*"]`) rather than a placeholder that exits.

#### I6. The Static template's nginx serves its default page, not the user's files

`backend/crates/api/src/sandbox_runtime/templates.rs:131`

Independent of C2's syntax error: the sandbox volume is mounted at `/app`, `working_dir` is now `/app`, and the command is `nginx -g 'daemon off;'`. `nginx:alpine`'s bundled config serves `/usr/share/nginx/html`, which is untouched by the volume. So even after C2 is fixed, a Static app's live preview shows the nginx welcome page while the editor shows and saves `/app/index.html` — edits never appear in the preview, with no error anywhere to explain why.

This is shared with the already-merged probe-detected static path (`detect_stack`'s `has_index_html` branch has the same `dev_cmd`), so it is not introduced here — but this branch is what puts a "Static" button in front of users, and the spec's manual-verification loop explicitly requires *"edit a file, confirm the live preview updates."* That step was never exercised for any template (the ledger correctly records it as an environment gap), so this went unnoticed.

**Fix:** have the Static template's seed script also write an nginx config rooted at `/app` (or seed a config into `/etc/nginx/conf.d/` — note that only `/app` is on the volume, so it must be written by the startup command each boot, not once) and make `dev_cmd` point nginx at it. Verify by loading the preview URL and confirming the seeded `<h1>` renders.

---

### Minor (Nice to Have)

1. **`tree` ignores the exec exit code** — `files.rs:~100`. Unlike the other five handlers, `tree` never checks `output.exit_code`. If `find` fails outright (missing `/app`, permission error), the handler returns `{entries: [], truncated: false}` and the editor renders an empty file tree with no explanation. Note the `| head -n N` pipeline makes the exit code the *pipeline's* (i.e. `head`'s), so this needs `set -o pipefail` or a `${PIPESTATUS}`-style check to be meaningful — worth doing, or at least worth a comment saying why it's deliberately unchecked.

2. **Three file-API endpoints have no UI consumer.** `mkdir`, `delete_entry`, `rename` and their client methods (`mkdirSandboxFile`, `deleteSandboxFile`, `renameSandboxFile`) are fully built and tested but `FileTree.svelte` offers no create/delete/rename affordance. Faithful to the spec (which specifies them backend-side and describes only a read-only tree frontend-side), so this is a plan-scope observation, not an implementation defect — but half the file API is currently dead code. Either add a context menu or note the deferral in the next sub-project's plan.

3. **"Tabbed editor" is specified but not built.** The spec calls for *"a tabbed CodeMirror editor in the center"*; the implementation has a single-file tab bar showing one path label. Reasonable scope trimming, but it should be recorded as a deviation rather than left implicit — and note that C1 is exactly the bug a real tab model would have forced someone to confront.

4. **No error surfaced when the file tree or a file read fails.** Editor route `:44-50` — if `getSandboxFileTree` fails, `bootState` still becomes `'ready'` with an empty tree. In `openFile` (`:52`), a failed read (e.g. the binary rejection from I3) leaves `openPath` unchanged and shows nothing at all, so clicking a binary file appears to do nothing. Both deserve a visible message; the boot error path itself is well done and this is the same pattern applied one level down.

5. **Terminal panel overlays rather than participates in the layout.** `.terminal-pane` is `position: fixed; bottom: 32px; height: 240px`, so it covers the bottom of the editor and preview instead of shrinking them; the grid already has a second row for the toggle, so it's close to being placeable properly. Cosmetic.

6. **`SandboxAppDetailPanel` declares an unused `onDeleted?: () => void` prop** (`:14`, not destructured, no delete control exists). Dead interface — drop it or implement delete.

7. **`create_app`'s two INSERTs are not wrapped in a transaction** (`routes.rs:~60-88`). A failure between them leaves an orphaned `services` row with no `sandbox_app_configs`, which then renders as a canvas node whose editor can't start. Already ledgered as deferred; a `state.db.begin()` here is ~4 lines. Also no validation that `name`/`slug` are non-empty or that `slug` is unique within the project (the DB constraint, if any, will surface as a raw `AppError::Database` string rather than a clean 400), and `service_id` uses `Uuid::new_v4()` where the codebase elsewhere prefers `now_v7()`.

8. **`preview_url` iframe has no reload affordance** (editor route `:130`). A template app's container is seconds old when the iframe first loads, so the initial render is often a connection error with no way to retry short of a full page reload.

9. **Symlink escape from `/app` remains possible** — pre-existing, correctly scoped out by the spec (which asks the validator to reject *"symlink escape attempts"*, which it does not do; `..`, absolute-path and quote rejection are all there). A symlink inside the volume pointing at `/etc` would let `cat`/`cat >` follow it out. The blast radius is one gVisor-isolated container the user already has a shell in, so this is close to theoretical — but the spec claimed the protection, so either implement it (`realpath`-then-prefix-check inside the container) or amend the spec to say it's out of scope.

10. **No teardown when a `sandbox_app` service is deleted** — pre-existing, and already flagged by an in-code `NOTE (follow-up)` at `manager.rs:164`. Deleting the service cascades the DB rows but leaves the container running and the volume on disk. Carrying it forward since this branch is what makes sandbox apps creatable from the UI, so it's now reachable by ordinary users rather than only by hand.

11. **Production WS path unverified.** The Vite fix is dev-only by construction. The pre-existing comment says SvelteKit's Node adapter cannot handle upgrades, which implies production must route `/api` to the backend at the edge rather than through `hooks.server.ts`. I could not confirm the production topology from this repo (`infra/docker-compose.dev.yml` doesn't define the api/frontend services). Since `ExecPanel`'s terminal presumably works in production today over the same `/api` path, the sandbox terminal should too — but this is an assumption, not something verified, and it's worth one deliberate check before anyone relies on the embedded terminal in a deployed environment.

---

## Cross-task interface consistency (spot-check of the predicted risk areas)

The pre-flight scan called out four shared-file risks. I checked each file's **final** state rather than each task's diff in isolation:

- **`sandbox_runtime/routes.rs`** (Tasks 4/5/6/10) — coherent. `routes()` registers the four `/sandbox/*` routes and `.merge()`s `files::routes()` and `exec::routes()`; `project_routes()` is cleanly separated for the `/projects/:project_id/apps` POST and is wired once in `api_router()` (`routes.rs:129`). No duplicate path registrations (axum would panic at startup on an exact duplicate, and the server builds and the suite runs). No route accidentally shadows another: `/apps/:service_id/files/*`, `/apps/:service_id/exec[/token]` and `/apps/:service_id/sandbox/*` are disjoint prefixes.
- **`sandbox_runtime/models.rs`** (Tasks 7 + 10) — coherent. Task 7's `seed_script_b64` landed on `SandboxAppConfigRow`, Task 10's `Serialize` derive and `default_stopped()` on `SandboxInstanceRow`. No field collision. Worth noting: `SandboxInstanceRow` now derives `Serialize` and is returned verbatim by the `status` route (`serde_json::json!(instance)`), so **`container_id` is exposed to any user with service read access**. Harmless today (the frontend's `SandboxInstance` type mirrors it, and C3 means the container id isn't the weak link anyway), but it's an internal identifier leaking into a public response — consider `#[serde(skip)]` on it once C3 is fixed.
- **`topology/mod.rs`** (stack-card bounded task + Task 10) — coherent. The domain-link fields (`first_domain`/`first_domain_tls`) live only in the final `else` branch; the new `sandbox_app` branch sits between `static` and `else` and is correctly ordered (no earlier branch can swallow `sandbox_app`). The `WHERE` clause change from `NOT IN ('edge_functions','sandbox_app')` to `!= 'edge_functions'` is right, and the comment was updated to match rather than left stale.
- **`api/client.ts` + `types.ts`** (Task 8 consumed by 9/11/12/13/14) — consistent. Every method name and shape the later tasks call exists with the signature they assume. `ServiceType` and `TopologyNode['type']` both gained `'sandbox_app'`, and `nodeTypes` in the project page registers `sandbox_app: SandboxAppNode`, so the canvas renderer, the backend `node_type` string and the click handler's `node.type` check all agree on one spelling. `putRaw`'s `text/plain` + `body: String` extractor pairing matches the backend's `write_file` signature.

One genuine cross-task gap, which is C1: the `CodeEditor` generalization (Task 9) exported `setValue`/`setLanguage` as the mechanism for changing content and language, and the editor route (Task 14) consumed the component via props only. Each task's diff was individually defensible; the seam between them is where the bug lives. The pre-flight scan checked the `language` prop *union* against `languageForPath`'s return type (and recorded "Matches exactly. Clean.") but not the *reactivity contract* — a type-level check where a behavioural one was needed. Worth remembering as a scan-methodology lesson: matching signatures is not the same as matching semantics.

---

## Recommendations

1. **Fix C1, C2 and C3 before merge.** All three are small (C1 ≈ 5 lines, C2 ≈ 6 lines plus a test, C3 ≈ 4 lines). C1 and C2 both destroy user data or functionality on ordinary first use; C3 is a cross-tenant container-shell hole.
2. **Then re-run Task 15's walkthrough against the two templates it never covered.** The three bugs Task 15 found are exactly the bugs live testing finds, and its Node-only coverage is precisely why C1, C2, I5 and I6 survived. Specifically: create a Static app and a Python app and confirm each boots; in the editor, open two different files in sequence and confirm the second one's content displays and saves to the right path; save a file twice and `docker exec … od -c` it to confirm no newline accretion. That's three checks, all cheap, and they cover C1, C2, I1, I5 and I6.
3. **Do I1 with the base64-write approach** rather than the `sep` patch, if you have the appetite. It retires the delimiter-collision machinery entirely and makes the write path byte-exact by construction rather than by careful reasoning — which is the stronger guarantee for the core save path.
4. **Open follow-up tickets, with the corrected diagnosis, for:** `is_container_dead`'s transient-error conflation and the swallowed `remove_container` (I4 — the highest-value of these; it affects already-merged code that this branch makes hot), `services::exec_ws`'s matching authorization gap (C3's sibling), and the service-delete teardown hook (Minor #10). Please carry I4's corrected analysis forward explicitly — the current ledger entry would send the next engineer after a lock that isn't the problem.
5. **Plan/spec feedback, not implementation feedback:** the spec's testing section commits only to unit tests for the path validator and command construction, and to a single manual walkthrough for everything else. That was a reasonable call for a UI-heavy sub-project with no frontend test precedent — but it left the *template matrix* with no automated coverage at all, and C2 is the direct result: a wrong constant that a two-line test (`assert_eq!(install_cmd, None)` for Static, cross-checked against `detect_stack`) would have caught instantly. If `template_runtime` claims in a doc comment to match `detect_stack`, a test should assert that against `detect_stack` itself rather than against hand-copied literals. Cheap, high-yield, and worth adding to the next plan.

---

## Assessment

**Ready to merge?** **With fixes** — C1, C2 and C3 must be fixed and re-verified first.

**Reasoning:** The backend is well-built and the security-sensitive surfaces the review process focused on (path validation, heredoc collision, the `create_app` permission gate, the WorkingDir fix) are all in a correct, non-regressed final state, with a genuinely green 139/12 suite and a clean type-check baseline. But three defects survived because they sit precisely where per-task review and a single-file, Node-only manual walkthrough can't see: a prop-reactivity seam between two individually-correct components that silently overwrites files on any second file open (C1), an empty-string-vs-NULL mismatch that bricks the Static template and is locked in by its own test (C2), and an authorization check omitted from the exec WebSocket because the pre-existing pattern it was told to mirror omits it too (C3). None is hard to fix; all three are the kind of thing that must not reach `main`.
