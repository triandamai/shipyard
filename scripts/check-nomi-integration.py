#!/usr/bin/env python3
"""Readiness check for the Nomi integration API.

Runs the whole flow Nomi depends on against a live Shipyard, using a throwaway
organization that it removes at the end: provision an org and an app, dev
database, secrets, publish (static and container), check the deployment,
roll back, restart, a failing build, environment duplicate, webhooks, and
termination. It also confirms the key can't reach orgs it doesn't own.

    SHIPYARD_URL=https://ship.example.com \\
    SHIPYARD_KEY=ship_int_...            \\
    python3 scripts/check-nomi-integration.py

Options:
    --reveal-key KEY   a key with the secrets:reveal scope (else reveal is skipped)
    --skip-container   skip the container publish/rollback/failure steps
                       (they pull node:22-alpine and build images)
    --keep             don't delete the test org afterwards
    --timeout SECONDS  how long to wait for each publish (default 300)

Exit status is 0 when every check passed or was skipped, 1 otherwise. Standard
library only.
"""

import argparse
import io
import json
import os
import secrets
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import zipfile

RESULTS = []


def record(status, name, detail=""):
    RESULTS.append((status, name, detail))
    mark = {"PASS": "ok  ", "FAIL": "FAIL", "SKIP": "skip"}[status]
    print(f"  [{mark}] {name}" + (f"  ({detail})" if detail else ""), flush=True)


def check(name, cond, detail=""):
    record("PASS" if cond else "FAIL", name, "" if cond else detail)
    return bool(cond)


def skip(name, why):
    record("SKIP", name, why)


class Client:
    def __init__(self, base, key):
        self.base = base.rstrip("/") + "/openapi/v1/integration"
        self.key = key

    def call(self, method, path, body=None, headers=None, raw=None, key=None):
        url = self.base + path
        data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
        hdrs = {"Authorization": f"Bearer {key or self.key}"}
        if body is not None:
            hdrs["Content-Type"] = "application/json"
        hdrs.update(headers or {})
        req = urllib.request.Request(url, data=data, method=method, headers=hdrs)
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                text = resp.read().decode()
                code = resp.status
        except urllib.error.HTTPError as e:
            text = e.read().decode()
            code = e.code
        except Exception as e:  # network error
            return 0, {"error": {"message": str(e)}}
        try:
            return code, json.loads(text)
        except ValueError:
            return code, {"raw": text[:300]}


def data(resp):
    return (resp[1] or {}).get("data")


def errmsg(resp):
    err = (resp[1] or {}).get("error") or {}
    return f"HTTP {resp[0]} {err.get('code', '')} {err.get('message', resp[1])}"[:240]


def multipart(fields):
    boundary = "----check" + secrets.token_hex(8)
    out = io.BytesIO()
    for name, (filename, content, ctype) in fields.items():
        out.write(f"--{boundary}\r\n".encode())
        disp = f'Content-Disposition: form-data; name="{name}"'
        if filename:
            disp += f'; filename="{filename}"'
        out.write((disp + "\r\n").encode())
        out.write(f"Content-Type: {ctype}\r\n\r\n".encode())
        out.write(content if isinstance(content, bytes) else content.encode())
        out.write(b"\r\n")
    out.write(f"--{boundary}--\r\n".encode())
    return out.getvalue(), f"multipart/form-data; boundary={boundary}"


def make_zip(files):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, content in files.items():
            z.writestr(name, content)
    return buf.getvalue()


def publish(c, org, app, env, manifest, files):
    body, ctype = multipart(
        {
            "manifest": (None, json.dumps(manifest), "application/json"),
            "source": ("source.zip", make_zip(files), "application/zip"),
        }
    )
    return c.call("POST", f"/orgs/{org}/apps/{app}/envs/{env}/publish", raw=body, headers={"Content-Type": ctype})


def wait_publish(c, org, publish_id, timeout):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        r = c.call("GET", f"/orgs/{org}/publishes/{publish_id}")
        last = data(r)
        if last and last["status"] != "running":
            return last
        time.sleep(3)
    return last


def container_files(version, broken=False):
    run = 'RUN node -e "require(\'./does-not-exist\')"\n' if broken else ""
    return {
        "Dockerfile": f"FROM node:22-alpine\nWORKDIR /app\nCOPY server.js .\n{run}EXPOSE 3000\nCMD [\"node\", \"server.js\"]\n",
        "server.js": f"require('http').createServer((q, r) => r.end('version {version}')).listen(3000);\n",
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--url", default=os.environ.get("SHIPYARD_URL"))
    ap.add_argument("--key", default=os.environ.get("SHIPYARD_KEY"))
    ap.add_argument("--reveal-key", default=os.environ.get("SHIPYARD_REVEAL_KEY"))
    ap.add_argument("--skip-container", action="store_true")
    ap.add_argument("--keep", action="store_true")
    ap.add_argument("--timeout", type=int, default=300)
    args = ap.parse_args()
    if not args.url or not args.key:
        ap.error("set SHIPYARD_URL and SHIPYARD_KEY (or pass --url and --key)")

    c = Client(args.url, args.key)
    org = "check-" + secrets.token_hex(4)
    app = "checkapp"
    created = False

    try:
        print(f"Shipyard Nomi integration check against {args.url}\ntest org: {org}\n")

        print("Authentication")
        r = c.call("GET", "/event-types")
        if not check("integration key accepted", r[0] == 200, errmsg(r)):
            print("\nCan't continue without a working key.")
            return 1
        bad = c.call("GET", "/event-types", key="ship_int_" + "0" * 64)
        check("a wrong key is rejected", bad[0] == 401, errmsg(bad))

        print("\nOrganization")
        r = c.call("PUT", f"/orgs/{org}", {"name": "Shipyard check"})
        created = check("create org (201)", r[0] == 201 and (data(r) or {}).get("created") is True, errmsg(r))
        r = c.call("PUT", f"/orgs/{org}", {"name": "Shipyard check renamed"})
        check("create again is idempotent (200, same org)", r[0] == 200 and (data(r) or {}).get("created") is False, errmsg(r))
        r = c.call("GET", f"/orgs/{org}")
        check("read org", r[0] == 200 and data(r)["org"]["external_id"] == org, errmsg(r))
        r = c.call("GET", f"/orgs/{uuid.uuid4()}")
        check("an org this key doesn't own is 404", r[0] == 404, errmsg(r))
        if not created:
            print("\nOrg creation failed; stopping.")
            return 1

        print("\nApp and development database")
        r = c.call("PUT", f"/orgs/{org}/apps/{app}", {"name": "Check app", "dev_database": True})
        if r[0] == 400 and "not enabled" in errmsg(r):
            check("create app", False, "unexpected 400")
        elif r[0] in (200, 201):
            check("create app (with dev database)", True)
            devdb = (data(r) or {}).get("dev_database")
            check("dev database returned refs, not connection strings", bool(devdb) and "postgres://" not in json.dumps(devdb), json.dumps(devdb)[:160])
        else:
            msg = errmsg(r)
            if "dev_db_admin_url" in msg or "not enabled" in msg:
                skip("dev database", "SHIPYARD__INTEGRATION__DEV_DB_ADMIN_URL is not set")
                r = c.call("PUT", f"/orgs/{org}/apps/{app}", {"name": "Check app"})
                check("create app", r[0] in (200, 201), errmsg(r))
            else:
                check("create app (with dev database)", False, msg)
                r = c.call("PUT", f"/orgs/{org}/apps/{app}", {"name": "Check app"})
                check("create app without dev database", r[0] in (200, 201), errmsg(r))

        print("\nSecrets")
        r = c.call("PUT", f"/orgs/{org}/apps/{app}/secrets/API_TOKEN", {"value": "tok_" + secrets.token_hex(8)})
        check("set a secret (returns a ref)", r[0] == 200 and str((data(r) or {}).get("ref", "")).startswith("sec_") and "value" not in (data(r) or {}), errmsg(r))
        ref = (data(r) or {}).get("ref")
        r = c.call("PUT", f"/orgs/{org}/apps/{app}/secrets/SESSION_SECRET", {"generate": "hex32"})
        check("generate a secret", r[0] == 200, errmsg(r))
        r = c.call("GET", f"/orgs/{org}/apps/{app}/secrets")
        names = [s["name"] for s in (data(r) or [])]
        check("list secrets (names and refs, no values)", r[0] == 200 and "API_TOKEN" in names and "value" not in json.dumps(data(r)), errmsg(r))
        if args.reveal_key and ref:
            r = c.call("POST", f"/secrets/{ref}/reveal", key=args.reveal_key)
            check("reveal with the reveal key", r[0] == 200 and (data(r) or {}).get("value", "").startswith("tok_"), errmsg(r))
            r = c.call("POST", f"/secrets/{ref}/reveal")
            check("reveal is refused for the agent key", r[0] == 403 or (args.key == args.reveal_key), errmsg(r))
        else:
            skip("secret reveal", "pass --reveal-key to test it")

        print("\nWebhooks")
        r = c.call("POST", "/webhooks", {"url": "https://example.invalid/hooks/shipyard", "events": ["deployment.*", "publish.*"], "org": org})
        wid = (data(r) or {}).get("id")
        check("register an endpoint (secret returned once)", r[0] == 201 and str((data(r) or {}).get("secret", "")).startswith("whsec_"), errmsg(r))
        if wid:
            r = c.call("POST", f"/webhooks/{wid}/test")
            check("test ping runs and reports the result", r[0] == 200 and "ok" in (data(r) or {}), errmsg(r))
            r = c.call("GET", f"/webhooks/{wid}/deliveries")
            check("list deliveries", r[0] == 200, errmsg(r))
            r = c.call("DELETE", f"/webhooks/{wid}")
            check("delete the endpoint", r[0] == 200, errmsg(r))

        print("\nPublish a static site")
        manifest = {"services": [{"name": "site", "kind": "static"}]}
        r = publish(c, org, app, "production", manifest, {"index.html": "<h1>shipyard check</h1>"})
        check("publish accepted (202)", r[0] == 202, errmsg(r))
        pub = data(r) or {}
        check("response says not live yet", pub.get("live") is False, json.dumps(pub)[:160])
        done = wait_publish(c, org, pub.get("publish_id", ""), args.timeout) if pub else None
        check("publish succeeded", bool(done) and done["status"] == "succeeded", (done or {}).get("error") or "timed out")
        if done and done["status"] == "succeeded":
            dep = done["deployments"][-1]
            check("publish lists deployments with normalized status", dep.get("status") == "succeeded" and "raw_status" in dep, json.dumps(dep)[:160])
            did = dep["deployment_id"]
            r = c.call("GET", f"/deployments/{did}?logs=false")
            d = data(r) or {}
            check("check deployment by id alone", r[0] == 200 and d.get("status") == "succeeded" and d.get("urls") is not None, errmsg(r))
            r = c.call("GET", f"/orgs/{org}/deployments/{did}")
            check("check deployment by org + id", r[0] == 200 and len((data(r) or {}).get("logs", [])) > 0, errmsg(r))
            r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/production/deployments/{did}")
            check("check deployment by org + app + env + id", r[0] == 200, errmsg(r))
            r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/production/deployments?limit=5")
            check("list deployments", r[0] == 200 and len(data(r) or []) >= 1, errmsg(r))
            r = c.call("GET", f"/orgs/{org}/apps/{app}/publishes")
            check("list publishes", r[0] == 200 and len(data(r) or []) >= 1, errmsg(r))
            r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/production")
            svc = ((data(r) or {}).get("services") or [{}])[0]
            check("environment status reports the service live", r[0] == 200 and svc.get("live") is True, json.dumps(svc)[:200])
            r = c.call("GET", f"/orgs/{org}/events?type=publish.*&app={app}")
            types = [e["type"] for e in (data(r) or {}).get("events", [])]
            check("events feed has publish.started and publish.succeeded", "publish.started" in types and "publish.succeeded" in types, str(types))

        print("\nDomain")
        r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/production/domain")
        cur = data(r) or {}
        if r[0] == 200 and cur.get("subdomain"):
            check("current domain is reported", bool(cur.get("url")), json.dumps(cur)[:160])
            r = c.call("GET", f"/orgs/{org}/domains/check?subdomain={cur['subdomain']}&app={app}&env=production")
            d = data(r) or {}
            check("own name counts as available", d.get("available") is True and d.get("current") is True, json.dumps(d)[:160])
            r = c.call("GET", f"/orgs/{org}/domains/check?subdomain={cur['subdomain']}")
            d = data(r) or {}
            check("a name in use is refused with free suggestions", d.get("available") is False and d.get("reason") == "taken" and len(d.get("suggestions", [])) > 0, json.dumps(d)[:200])
            if d.get("suggestions"):
                s0 = d["suggestions"][0]["subdomain"]
                r = c.call("GET", f"/orgs/{org}/domains/check?subdomain={s0}")
                check("every suggestion is itself available", (data(r) or {}).get("available") is True, s0)
            r = c.call("GET", f"/orgs/{org}/domains/check?subdomain=Bad_Name")
            d = data(r) or {}
            check("an invalid name is explained", d.get("available") is False and d.get("reason") == "invalid", json.dumps(d)[:160])
            r = c.call("PUT", f"/orgs/{org}/apps/{app}/envs/production/domain", {"subdomain": cur["subdomain"] + "-x"})
            d = data(r) or {}
            check("change to a free name", r[0] == 200 and d.get("changed") is True, errmsg(r))
            r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/production/domain")
            check("the new name is live in the environment", (data(r) or {}).get("subdomain") == cur["subdomain"] + "-x", json.dumps(data(r))[:160])
            r = c.call("PUT", f"/orgs/{org}/apps/{app}/envs/production/domain", {"subdomain": "www"})
            check("a reserved name is refused (409 with suggestions)", r[0] == 409, errmsg(r))
        else:
            skip("domain checks", "no apps base domain configured")

        if args.skip_container:
            skip("container publish, rollback, restart, failure", "--skip-container")
        else:
            print("\nPublish a container (two versions), roll back, restart")
            manifest = {
                "services": [
                    {
                        "name": "web",
                        "port": 3000,
                        "env": {"API_TOKEN": {"secret": "API_TOKEN"}, "SESSION_SECRET": {"generate": "hex32"}, "MODE": "check"},
                    }
                ]
            }
            ids = []
            for ver in ("v1", "v2"):
                r = publish(c, org, app, "staging", manifest, container_files(ver))
                p = data(r) or {}
                done = wait_publish(c, org, p.get("publish_id", ""), args.timeout) if p else None
                ok = bool(done) and done["status"] == "succeeded"
                check(f"publish {ver} to staging", ok, (done or {}).get("error") or errmsg(r))
                if ok:
                    ids.append(done["deployments"][-1]["deployment_id"])
            if len(ids) == 2:
                r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/staging/services/web/env")
                envs = {e["name"]: e for e in (data(r) or [])}
                check("env listing shows names and secret refs, never values", r[0] == 200 and envs.get("API_TOKEN", {}).get("secret_ref") and "tok_" not in json.dumps(data(r)), errmsg(r))
                r = c.call("POST", f"/orgs/{org}/apps/{app}/envs/staging/services/web/rollback", {})
                rb = data(r) or {}
                check("rollback to the previous version (202)", r[0] == 202 and rb.get("rolled_back_to") == ids[0], errmsg(r))
                if rb:
                    end = time.time() + args.timeout
                    status = "running"
                    while time.time() < end and status == "running":
                        time.sleep(3)
                        status = (data(c.call("GET", f"/deployments/{rb['deployment_id']}?logs=false")) or {}).get("status", "running")
                    check("rollback deployment succeeded", status == "succeeded", status)
                r = c.call("POST", f"/orgs/{org}/apps/{app}/envs/staging/restart", {})
                check("restart applies current env", r[0] == 200 and "web" in (data(r) or {}).get("restarted", []), errmsg(r))
                r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/staging/services/web/logs?tail=20")
                check("runtime logs readable", r[0] == 200, errmsg(r))
                lines = (data(r) or {}).get("lines", [])
                check("runtime logs are structured", len(lines) == 0 or (isinstance(lines[0], dict) and "at" in lines[0] and "level" in lines[0] and "message" in lines[0] and "service" in lines[0]), str(lines[:1]))
                r_json = c.call("GET", f"/orgs/{org}/apps/{app}/envs/staging/services/web/logs?format=json")
                check("runtime logs format=json returns lines", r_json[0] == 200 and "lines" in (data(r_json) or {}), errmsg(r_json))
                r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/staging/usage")
                check("usage readable", r[0] == 200, errmsg(r))
                r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/staging/metrics?range=24h&step=1h")
                m = data(r) or {}
                check("metrics endpoint readable with buckets and totals", r[0] == 200 and "buckets" in m and "totals" in m and "previous_totals" in m, errmsg(r))

            print("\nA build that fails")
            r = publish(c, org, app, "staging", manifest, container_files("v3", broken=True))
            p = data(r) or {}
            done = wait_publish(c, org, p.get("publish_id", ""), args.timeout) if p else None
            check("publish reports failed", bool(done) and done["status"] == "failed", errmsg(r))
            if done and done["status"] == "failed" and done["deployments"]:
                r = c.call("GET", f"/deployments/{done['deployments'][-1]['deployment_id']}?logs=false")
                d = data(r) or {}
                f = d.get("failure") or {}
                check("failed deployment explains itself (step, phase, message, tail)", d.get("status") == "failed" and f.get("phase") == "build" and f.get("message") and f.get("tail"), json.dumps(f)[:200])
                check("failed deployment has no urls", d.get("urls") == [], str(d.get("urls")))
                skipped = [s for s in d.get("steps", []) if s["status"] == "skipped"]
                check("steps after the failure are skipped, not running", len(skipped) > 0 and not any(s["status"] == "running" for s in d["steps"]), str([(s["name"], s["status"]) for s in d.get("steps", [])]))
            r = c.call("GET", f"/orgs/{org}/apps/{app}/envs/staging")
            svc = ((data(r) or {}).get("services") or [{}])[0]
            check("the previous version keeps serving after a failed publish", svc.get("live") is True, json.dumps(svc)[:200])

        print("\nEnvironments")
        r = c.call("POST", f"/orgs/{org}/apps/{app}/envs/production/duplicate", {"to": "preview-check"})
        p = data(r) or {}
        check("duplicate production into another environment (202)", r[0] == 202, errmsg(r))
        if p:
            done = wait_publish(c, org, p["publish_id"], args.timeout)
            check("the copy deploys", bool(done) and done["status"] == "succeeded", (done or {}).get("error") or "timed out")
            r = c.call("DELETE", f"/orgs/{org}/apps/{app}/envs/preview-check")
            check("delete the copy", r[0] == 200, errmsg(r))

        print("\nTransfer to another organization")
        org2 = org + "-b"
        r = c.call("PUT", f"/orgs/{org2}", {"name": "check destination"})
        check("create a second org", r[0] in (200, 201), errmsg(r))
        try:
            r = c.call("POST", f"/orgs/{org}/apps/{app}/transfer", {"to_org": org})
            check("transfer to the same org is refused", r[0] == 400, errmsg(r))
            r = c.call("POST", f"/orgs/{org}/apps/{app}/transfer", {"to_org": org2})
            moved = data(r) or {}
            check("transfer the app", r[0] == 200 and len(moved.get("environments", [])) >= 1, errmsg(r))
            r = c.call("GET", f"/orgs/{org}/apps/{app}")
            check("the old org no longer has the app", r[0] == 404, errmsg(r))
            r = c.call("GET", f"/orgs/{org2}/apps/{app}/envs/production")
            check("the new org has the environment", r[0] == 200, errmsg(r))
            r = c.call("GET", f"/orgs/{org2}/apps/{app}/secrets")
            check("secrets moved with it", r[0] == 200 and len(data(r) or []) >= 1, errmsg(r))
            r = c.call("POST", f"/orgs/{org2}/apps/{app}/transfer", {"to_org": org})
            check("transfer back", r[0] == 200, errmsg(r))
        finally:
            c.call("DELETE", f"/orgs/{org2}")

        print("\nTermination")
        r = c.call("DELETE", f"/orgs/{org}/apps/{app}")
        check("terminate the app", r[0] == 200, errmsg(r))
        r = c.call("GET", f"/orgs/{org}/apps/{app}")
        check("the app is gone", r[0] == 404, errmsg(r))
    finally:
        if created and not args.keep:
            r = c.call("DELETE", f"/orgs/{org}")
            check("delete the test org", r[0] == 200, errmsg(r))
            r = c.call("GET", f"/orgs/{org}")
            check("the org is gone", r[0] == 404, errmsg(r))

    failed = [x for x in RESULTS if x[0] == "FAIL"]
    skipped = [x for x in RESULTS if x[0] == "SKIP"]
    print(f"\n{len(RESULTS) - len(failed) - len(skipped)} passed, {len(failed)} failed, {len(skipped)} skipped")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
