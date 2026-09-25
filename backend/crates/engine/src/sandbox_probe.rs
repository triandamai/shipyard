use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct ProbeResult {
    pub has_package_json: bool,
    pub package_json: Option<String>,
    pub has_requirements_txt: bool,
    pub has_manage_py: bool,
    pub has_index_html: bool,
    pub shipyard_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetectionSource {
    Detected,
    Manifest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedStack {
    pub runtime: String,
    pub base_image: String,
    pub install_cmd: Option<String>,
    pub dev_cmd: String,
    pub port: u16,
    pub source: DetectionSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_overrides_detection_when_present() {
        let probe = ProbeResult {
            has_package_json: true,
            package_json: Some(r#"{"scripts":{"dev":"vite"}}"#.to_string()),
            shipyard_json: Some(r#"{"app":{"runtime":"node","install":"pnpm install","dev":"pnpm dev","port":4000}}"#.to_string()),
            ..Default::default()
        };
        let stack = detect_stack(&probe).expect("manifest should resolve successfully");
        assert_eq!(stack.source, DetectionSource::Manifest);
        assert_eq!(stack.install_cmd.as_deref(), Some("pnpm install"));
        assert_eq!(stack.dev_cmd, "pnpm dev");
        assert_eq!(stack.port, 4000);
    }

    #[test]
    fn detects_node_from_package_json_dev_script() {
        let probe = ProbeResult {
            has_package_json: true,
            package_json: Some(r#"{"scripts":{"dev":"vite --host 0.0.0.0"}}"#.to_string()),
            ..Default::default()
        };
        let stack = detect_stack(&probe).expect("node should be detected");
        assert_eq!(stack.source, DetectionSource::Detected);
        assert_eq!(stack.runtime, "node");
        assert_eq!(stack.base_image, "node:20-alpine");
        assert_eq!(stack.install_cmd.as_deref(), Some("npm install"));
        assert_eq!(stack.dev_cmd, "npm run dev");
        assert_eq!(stack.port, 3000);
    }

    #[test]
    fn detects_python_from_requirements_and_manage_py() {
        let probe = ProbeResult {
            has_requirements_txt: true,
            has_manage_py: true,
            ..Default::default()
        };
        let stack = detect_stack(&probe).expect("python/django should be detected");
        assert_eq!(stack.runtime, "python");
        assert_eq!(stack.base_image, "python:3.12-slim");
        assert_eq!(stack.install_cmd.as_deref(), Some("pip install -r requirements.txt"));
        assert_eq!(stack.dev_cmd, "python manage.py runserver 0.0.0.0:8000");
        assert_eq!(stack.port, 8000);
    }

    #[test]
    fn detects_static_from_index_html_only() {
        let probe = ProbeResult { has_index_html: true, ..Default::default() };
        let stack = detect_stack(&probe).expect("static site should be detected");
        assert_eq!(stack.runtime, "static");
        assert_eq!(stack.install_cmd, None);
        assert_eq!(stack.port, 8080);
    }

    #[test]
    fn unrecognized_stack_returns_actionable_error() {
        let probe = ProbeResult::default();
        let err = detect_stack(&probe).unwrap_err();
        assert!(err.contains("shipyard.json"), "error should point the user at the manifest fallback, got: {err}");
    }
}

/// Shell script executed inside a throwaway `alpine`-based container with the
/// app's volume mounted read-only at /app. Prints exactly one JSON line to
/// stdout describing what it found, so the API process never needs direct
/// filesystem access to a volume that may live on a remote node.
pub const SANDBOX_PROBE_SCRIPT: &str = r#"
set -e
cd /app
pkg=$( [ -f package.json ] && cat package.json || echo "" )
sj=$( [ -f shipyard.json ] && cat shipyard.json || echo "" )
has_pkg=false; [ -f package.json ] && has_pkg=true
has_req=false; [ -f requirements.txt ] && has_req=true
has_manage=false; [ -f manage.py ] && has_manage=true
has_index=false; [ -f index.html ] && has_index=true
printf '{"has_package_json":%s,"package_json":%s,"has_requirements_txt":%s,"has_manage_py":%s,"has_index_html":%s,"shipyard_json":%s}\n' \
  "$has_pkg" "$( [ -n "$pkg" ] && printf '%s' "$pkg" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk 'BEGIN{printf "\""} {printf "%s\\n", $0} END{printf "\""}' || echo null )" \
  "$has_req" "$has_manage" "$has_index" \
  "$( [ -n "$sj" ] && printf '%s' "$sj" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk 'BEGIN{printf "\""} {printf "%s\\n", $0} END{printf "\""}' || echo null )"
"#;

/// Minimal nginx config, regenerated on every container boot (only `/app`
/// persists across restarts, not `/etc/nginx`), that roots nginx at the
/// sandbox's mounted volume instead of nginx:alpine's bundled default page.
/// Shared between probe-based detection (this file's `detect_stack`) and
/// template-based creation (`shipyard_api::sandbox_runtime::templates`'s
/// `template_runtime`) so both paths produce an identical, actually-working
/// static site rather than silently diverging. The heredoc delimiter is
/// **quoted** (`<<'NGINX_EOF'`, not `<<NGINX_EOF`) — this is essential, not
/// stylistic: nginx's own `$uri` variable reference must survive untouched,
/// and an unquoted delimiter would let the shell expand it to nothing before
/// nginx ever saw the file. `listen 8080` is a literal tied to this same
/// static stack's fixed port (8080) in both callers — if that port ever
/// changes in either `detect_stack`'s or `template_runtime`'s Static branch,
/// update this literal to match.
pub const STATIC_DEV_CMD: &str = "mkdir -p /etc/nginx/conf.d && cat > /etc/nginx/conf.d/default.conf <<'NGINX_EOF'\nserver {\n    listen 8080;\n    root /app;\n    index index.html;\n    location / {\n        try_files $uri $uri/ =404;\n    }\n}\nNGINX_EOF\nnginx -g 'daemon off;'";

fn dev_script_from_package_json(package_json: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(package_json).ok()?;
    parsed.get("scripts")?.get("dev").and_then(|v| v.as_str()).map(|_| "npm run dev".to_string())
}

/// Resolve the stack a sandbox should run. Manifest (shipyard.json `app` key)
/// always wins when present; otherwise falls back to file-presence detection;
/// otherwise returns an error naming the manifest as the fix.
pub fn detect_stack(probe: &ProbeResult) -> Result<DetectedStack, String> {
    if let Some(sj) = &probe.shipyard_json {
        if let Ok(config) = serde_json::from_str::<crate::edge_fn_config::ShipyardConfig>(sj) {
            if let Some(app) = config.app {
                let base_image = match app.runtime.as_str() {
                    "node" => "node:20-alpine",
                    "python" => "python:3.12-slim",
                    "static" => "nginx:alpine",
                    other => return Err(format!("shipyard.json declares unknown runtime '{other}'")),
                };
                return Ok(DetectedStack {
                    runtime: app.runtime,
                    base_image: base_image.to_string(),
                    install_cmd: app.install,
                    dev_cmd: app.dev,
                    port: app.port,
                    source: DetectionSource::Manifest,
                });
            }
        }
    }

    if probe.has_package_json {
        let dev_cmd = probe
            .package_json
            .as_deref()
            .and_then(dev_script_from_package_json)
            .unwrap_or_else(|| "npm start".to_string());
        return Ok(DetectedStack {
            runtime: "node".to_string(),
            base_image: "node:20-alpine".to_string(),
            install_cmd: Some("npm install".to_string()),
            dev_cmd,
            port: 3000,
            source: DetectionSource::Detected,
        });
    }

    if probe.has_requirements_txt {
        let dev_cmd = if probe.has_manage_py {
            "python manage.py runserver 0.0.0.0:8000".to_string()
        } else {
            "python app.py".to_string()
        };
        return Ok(DetectedStack {
            runtime: "python".to_string(),
            base_image: "python:3.12-slim".to_string(),
            install_cmd: Some("pip install -r requirements.txt".to_string()),
            dev_cmd,
            port: 8000,
            source: DetectionSource::Detected,
        });
    }

    if probe.has_index_html {
        return Ok(DetectedStack {
            runtime: "static".to_string(),
            base_image: "nginx:alpine".to_string(),
            install_cmd: None,
            dev_cmd: STATIC_DEV_CMD.to_string(),
            port: 8080,
            source: DetectionSource::Detected,
        });
    }

    Err("Could not detect a recognized stack (no package.json, requirements.txt, or index.html found). Add a shipyard.json manifest declaring { \"app\": { \"runtime\", \"install\", \"dev\", \"port\" } }.".to_string())
}
