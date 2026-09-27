use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    Node,
    Python,
    Static,
    Custom,
    React,
    ReactTs,
    Vue,
    VueTs,
    SvelteKit,
    SvelteKitTs,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_template_seed_script_decodes_to_valid_shell() {
        let b64 = template_seed_script_b64(Template::Node);
        let decoded = BASE64.decode(&b64).expect("must be valid base64");
        let script = String::from_utf8(decoded).expect("must be valid utf8");
        assert!(script.contains("package.json"), "node seed script must write package.json");
        assert!(script.contains("mkdir -p /app"), "seed script must ensure /app exists");
    }

    #[test]
    fn python_template_seed_script_decodes_to_valid_shell() {
        let b64 = template_seed_script_b64(Template::Python);
        let decoded = BASE64.decode(&b64).expect("must be valid base64");
        let script = String::from_utf8(decoded).expect("must be valid utf8");
        assert!(script.contains("requirements.txt"), "python seed script must write requirements.txt");
        assert!(script.contains("app.py"), "python seed script must write app.py, the file dev_cmd actually runs");
        assert!(script.contains("PORT"), "seeded app.py must bind to the sandbox's PORT env var, not a hardcoded port");
        assert!(!script.contains("django"), "the Django placeholder that never actually ran anything must be gone");
        assert!(!script.contains("manage.py"), "manage.py placeholder (sys.exit(0) on startup) must be gone");
    }

    #[test]
    fn static_template_seed_script_decodes_to_valid_shell() {
        let b64 = template_seed_script_b64(Template::Static);
        let decoded = BASE64.decode(&b64).expect("must be valid base64");
        let script = String::from_utf8(decoded).expect("must be valid utf8");
        assert!(script.contains("index.html"), "static seed script must write index.html");
    }

    #[test]
    fn template_runtime_matches_stack_detector_conventions() {
        let (runtime, base_image, install, dev, port) = template_runtime(Template::Node);
        assert_eq!(runtime, "node");
        assert_eq!(base_image, "node:20-alpine");
        assert_eq!(install, Some("npm install"));
        assert_eq!(dev, "npm run dev");
        assert_eq!(port, 3000);

        let (runtime, base_image, install, dev, port) = template_runtime(Template::Python);
        assert_eq!(runtime, "python");
        assert_eq!(base_image, "python:3.12-slim");
        assert_eq!(install, Some("pip install -r requirements.txt"));
        assert_eq!(dev, "python app.py");
        assert_eq!(port, 8000);

        let (runtime, base_image, install, dev, port) = template_runtime(Template::Static);
        assert_eq!(runtime, "static");
        assert_eq!(base_image, "nginx:alpine");
        assert_eq!(install, None);
        // Compare against detect_stack's own output, not just the shared
        // constant against itself -- template_runtime already *returns*
        // STATIC_DEV_CMD, so `assert_eq!(dev, STATIC_DEV_CMD)` would pass
        // even if detect_stack's static branch regressed back to the old
        // broken nginx command. This is the actual invariant this test
        // exists to protect: the template and probe paths must not diverge.
        let probe = shipyard_engine::sandbox_probe::ProbeResult {
            has_index_html: true,
            ..Default::default()
        };
        let detected = shipyard_engine::sandbox_probe::detect_stack(&probe).unwrap();
        assert_eq!(dev, detected.dev_cmd, "template and probe paths must not diverge");
        assert_eq!(port, 8080);
    }

    #[test]
    fn static_dev_cmd_writes_a_quoted_heredoc_so_nginx_variables_are_not_shell_expanded() {
        // Regression guard: the nginx config this writes uses $uri as a
        // literal nginx variable reference. If the heredoc delimiter were
        // ever changed to an unquoted form, the shell would try to expand
        // $uri as one of its own (nonexistent) variables before nginx ever
        // saw the file, silently corrupting the generated config.
        let (_, _, _, dev, _) = template_runtime(Template::Static);
        assert!(dev.contains("<<'NGINX_EOF'"), "heredoc delimiter must be quoted to disable shell expansion inside the config body");
        assert!(dev.contains("$uri"), "the generated config must reference nginx's $uri variable literally");
        assert!(dev.contains("listen 8080;"), "the generated config must listen on the template's declared port");
    }

    #[test]
    fn custom_template_seed_script_writes_a_starter_shipyard_json() {
        let b64 = template_seed_script_b64(Template::Custom);
        let decoded = BASE64.decode(&b64).expect("must be valid base64");
        let script = String::from_utf8(decoded).expect("must be valid utf8");
        assert!(script.contains("shipyard.json"), "custom seed script must write a starter shipyard.json");
        assert!(script.contains("\"runtime\""), "starter manifest must show the app.runtime field");
        assert!(script.contains("\"dev\""), "starter manifest must show the app.dev field");
        assert!(script.contains("cat > /app/shipyard.json.example"), "must write the starter as .example, not a live shipyard.json");
        assert!(!script.contains("cat > /app/shipyard.json <<"), "must not write a live shipyard.json — sandbox_probe.rs would self-detect it prematurely, permanently exiting 'pending' before the user scaffolds anything real");
    }

    #[test]
    fn custom_template_runtime_is_an_idle_placeholder() {
        let (runtime, base_image, install, dev, _port) = template_runtime(Template::Custom);
        assert_eq!(runtime, "custom");
        assert_eq!(base_image, "node:20-alpine");
        assert_eq!(install, None, "nothing to install yet — /app starts empty");
        assert_eq!(dev, "sleep infinity", "must keep the container alive with no real app yet, so the terminal stays usable");
    }

    #[test]
    fn custom_template_is_reachable_by_name() {
        assert_eq!(Template::from_str("custom"), Some(Template::Custom));
    }

    #[test]
    fn vite_family_templates_scaffold_via_the_official_create_vite_cli() {
        let cases = [
            (Template::React, "--template react"),
            (Template::ReactTs, "--template react-ts"),
            (Template::Vue, "--template vue"),
            (Template::VueTs, "--template vue-ts"),
        ];
        for (t, expected_flag) in cases {
            let b64 = template_seed_script_b64(t);
            let decoded = BASE64.decode(&b64).expect("must be valid base64");
            let script = String::from_utf8(decoded).expect("must be valid utf8");
            assert!(script.contains("npm create vite@latest"), "{t:?} seed script must invoke create-vite");
            assert!(script.contains(expected_flag), "{t:?} seed script must pass '{expected_flag}'");
        }
    }

    #[test]
    fn vite_family_templates_bind_dev_server_to_all_interfaces_and_the_port_env_var() {
        for t in [Template::React, Template::ReactTs, Template::Vue, Template::VueTs] {
            let (runtime, base_image, install, dev, port) = template_runtime(t);
            assert_eq!(runtime, "node");
            assert_eq!(base_image, "node:20-alpine");
            assert_eq!(install, Some("npm install"));
            assert!(dev.contains("--host 0.0.0.0"), "{t:?} dev_cmd must bind all interfaces: {dev}");
            assert!(dev.contains("--port $PORT"), "{t:?} dev_cmd must read the PORT env var: {dev}");
            assert_eq!(port, 5173, "Vite's own default dev port");
        }
    }

    #[test]
    fn vite_family_templates_are_reachable_by_name() {
        assert_eq!(Template::from_str("react"), Some(Template::React));
        assert_eq!(Template::from_str("react-ts"), Some(Template::ReactTs));
        assert_eq!(Template::from_str("vue"), Some(Template::Vue));
        assert_eq!(Template::from_str("vue-ts"), Some(Template::VueTs));
    }

    #[test]
    fn sveltekit_templates_scaffold_via_the_official_sv_cli() {
        let cases = [
            (Template::SvelteKit, "--types jsdoc"),
            (Template::SvelteKitTs, "--types ts"),
        ];
        for (t, expected_flag) in cases {
            let b64 = template_seed_script_b64(t);
            let decoded = BASE64.decode(&b64).expect("must be valid base64");
            let script = String::from_utf8(decoded).expect("must be valid utf8");
            assert!(script.contains("sv create"), "{t:?} seed script must invoke the sv CLI");
            assert!(script.contains(expected_flag), "{t:?} seed script must pass '{expected_flag}'");
        }
    }

    #[test]
    fn sveltekit_templates_bind_dev_server_correctly() {
        for t in [Template::SvelteKit, Template::SvelteKitTs] {
            let (runtime, base_image, install, dev, port) = template_runtime(t);
            assert_eq!(runtime, "node");
            assert_eq!(base_image, "node:20-alpine");
            assert_eq!(install, Some("npm install"));
            assert!(dev.contains("--host 0.0.0.0"));
            assert!(dev.contains("--port $PORT"));
            assert_eq!(port, 5173, "SvelteKit's Vite-based dev server default port");
        }
    }

    #[test]
    fn sveltekit_templates_are_reachable_by_name() {
        assert_eq!(Template::from_str("sveltekit"), Some(Template::SvelteKit));
        assert_eq!(Template::from_str("sveltekit-ts"), Some(Template::SvelteKitTs));
    }
}

const NODE_SEED_SCRIPT: &str = r#"mkdir -p /app
cat > /app/package.json <<'SHIPYARD_EOF'
{
  "name": "sandbox-app",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "vite --host 0.0.0.0 --port $PORT"
  },
  "devDependencies": {
    "vite": "^5.0.0"
  }
}
SHIPYARD_EOF
cat > /app/index.html <<'SHIPYARD_EOF'
<!doctype html>
<html>
  <head><title>Sandbox App</title></head>
  <body>
    <h1>Hello from your new sandbox app!</h1>
    <p>Edit <code>index.html</code> or add files to get started.</p>
  </body>
</html>
SHIPYARD_EOF
"#;

const PYTHON_SEED_SCRIPT: &str = r#"mkdir -p /app
cat > /app/requirements.txt <<'SHIPYARD_EOF'
SHIPYARD_EOF
cat > /app/app.py <<'SHIPYARD_EOF'
import http.server
import os
import socketserver

PORT = int(os.environ.get("PORT", 8000))


class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/":
            body = (
                b"<!doctype html><html><head><title>Sandbox App</title></head>"
                b"<body><h1>Hello from your new sandbox app!</h1>"
                b"<p>Edit <code>app.py</code> or add files to get started.</p>"
                b"</body></html>"
            )
            self.send_response(200)
            self.send_header("Content-Type", "text/html")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            super().do_GET()


class ReusableTCPServer(socketserver.TCPServer):
    allow_reuse_address = True


with ReusableTCPServer(("0.0.0.0", PORT), Handler) as httpd:
    print(f"Serving on 0.0.0.0:{PORT}")
    httpd.serve_forever()
SHIPYARD_EOF
"#;

const STATIC_SEED_SCRIPT: &str = r#"mkdir -p /app
cat > /app/index.html <<'SHIPYARD_EOF'
<!doctype html>
<html>
  <head><title>Sandbox App</title></head>
  <body>
    <h1>Hello from your new static sandbox app!</h1>
  </body>
</html>
SHIPYARD_EOF
"#;

/// Writes the starter manifest as `shipyard.json.example`, NOT the live
/// `shipyard.json` filename. `shipyard_engine::sandbox_probe`'s stack
/// detector looks for a file literally named `shipyard.json` and, if found,
/// treats it as an authoritative, already-resolved manifest — checked before
/// any framework-specific heuristic like `package.json`/`requirements.txt`.
/// Combined with `provision_sandbox`'s redetect-on-restart logic (which
/// re-probes a `'pending'` Custom sandbox on every start until detection
/// succeeds once, then locks in permanently), a *live* `shipyard.json` here
/// would get self-detected as a real app on the sandbox's very first
/// Stop+Start — before the user has scaffolded anything — permanently
/// flipping `manifest_source` out of `'pending'` with a `dev_cmd` that
/// immediately fails (no real `package.json` exists yet) and, since it can
/// never be `'pending'` again, no path back to ever detecting the real
/// project the user eventually adds. Keeping this as `.example` means the
/// probe's exact-filename check never matches it until the user deliberately
/// renames it (`mv shipyard.json.example shipyard.json`) once they've
/// customized it for their real project — preserving the "starter template
/// to copy from" intent without the premature self-detection.
const CUSTOM_SEED_SCRIPT: &str = r#"mkdir -p /app
cat > /app/shipyard.json.example <<'SHIPYARD_EOF'
{
  "app": {
    "runtime": "node",
    "install": "npm install",
    "dev": "npm run dev",
    "port": 3000
  }
}
SHIPYARD_EOF
"#;

const REACT_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template react\n";
const REACT_TS_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template react-ts\n";
const VUE_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template vue\n";
const VUE_TS_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template vue-ts\n";
const SVELTEKIT_SEED_SCRIPT: &str = "npx --yes sv create /app --template minimal --types jsdoc --no-add-ons --install npm\n";
const SVELTEKIT_TS_SEED_SCRIPT: &str = "npx --yes sv create /app --template minimal --types ts --no-add-ons --install npm\n";

pub fn template_seed_script_b64(t: Template) -> String {
    let script = match t {
        Template::Node => NODE_SEED_SCRIPT,
        Template::Python => PYTHON_SEED_SCRIPT,
        Template::Static => STATIC_SEED_SCRIPT,
        Template::Custom => CUSTOM_SEED_SCRIPT,
        Template::React => REACT_SEED_SCRIPT,
        Template::ReactTs => REACT_TS_SEED_SCRIPT,
        Template::Vue => VUE_SEED_SCRIPT,
        Template::VueTs => VUE_TS_SEED_SCRIPT,
        Template::SvelteKit => SVELTEKIT_SEED_SCRIPT,
        Template::SvelteKitTs => SVELTEKIT_TS_SEED_SCRIPT,
    };
    BASE64.encode(script)
}

/// Returns (runtime, base_image, install_cmd, dev_cmd, port) — matches the
/// exact values `shipyard_engine::sandbox_probe::detect_stack` would infer
/// for these stacks, so a template-created app behaves identically to a
/// probe-detected one on every start after the first.
pub fn template_runtime(t: Template) -> (&'static str, &'static str, Option<&'static str>, &'static str, u16) {
    match t {
        Template::Node => ("node", "node:20-alpine", Some("npm install"), "npm run dev", 3000),
        Template::Python => ("python", "python:3.12-slim", Some("pip install -r requirements.txt"), "python app.py", 8000),
        Template::Static => (
            "static",
            "nginx:alpine",
            None,
            shipyard_engine::sandbox_probe::STATIC_DEV_CMD,
            8080,
        ),
        // No real app yet — /app starts empty except for the starter
        // shipyard.json the seed script writes. `sleep infinity` keeps the
        // container alive so the terminal stays usable; sandbox_runtime::manager's
        // provision_sandbox re-detects the real stack on every restart while
        // this sandbox's manifest_source stays 'pending'.
        Template::Custom => ("custom", "node:20-alpine", None, "sleep infinity", 3000),
        Template::React => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::ReactTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::Vue => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::VueTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::SvelteKit => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::SvelteKitTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
    }
}

impl Template {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "node" => Some(Template::Node),
            "python" => Some(Template::Python),
            "static" => Some(Template::Static),
            "custom" => Some(Template::Custom),
            "react" => Some(Template::React),
            "react-ts" => Some(Template::ReactTs),
            "vue" => Some(Template::Vue),
            "vue-ts" => Some(Template::VueTs),
            "sveltekit" => Some(Template::SvelteKit),
            "sveltekit-ts" => Some(Template::SvelteKitTs),
            _ => None,
        }
    }
}
