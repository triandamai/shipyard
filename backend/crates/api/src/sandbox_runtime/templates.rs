use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    Node,
    Python,
    Static,
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
        assert_eq!(dev, "python manage.py runserver 0.0.0.0:8000");
        assert_eq!(port, 8000);

        let (runtime, base_image, install, dev, port) = template_runtime(Template::Static);
        assert_eq!(runtime, "static");
        assert_eq!(base_image, "nginx:alpine");
        assert_eq!(install, None);
        assert_eq!(dev, "nginx -g 'daemon off;'");
        assert_eq!(port, 8080);
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
django==5.0
SHIPYARD_EOF
cat > /app/manage.py <<'SHIPYARD_EOF'
#!/usr/bin/env python
import sys
print("Sandbox app placeholder - replace manage.py with your Django project.")
sys.exit(0)
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

pub fn template_seed_script_b64(t: Template) -> String {
    let script = match t {
        Template::Node => NODE_SEED_SCRIPT,
        Template::Python => PYTHON_SEED_SCRIPT,
        Template::Static => STATIC_SEED_SCRIPT,
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
        Template::Python => ("python", "python:3.12-slim", Some("pip install -r requirements.txt"), "python manage.py runserver 0.0.0.0:8000", 8000),
        Template::Static => ("static", "nginx:alpine", None, "nginx -g 'daemon off;'", 8080),
    }
}

impl Template {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "node" => Some(Template::Node),
            "python" => Some(Template::Python),
            "static" => Some(Template::Static),
            _ => None,
        }
    }
}
