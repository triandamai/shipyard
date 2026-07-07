use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsrFramework {
    NextJs,
    SvelteKit,
    Nuxt,
    Angular,
    TanStackStart,
}

impl SsrFramework {
    pub fn name(&self) -> &'static str {
        match self {
            Self::NextJs => "Next.js",
            Self::SvelteKit => "SvelteKit",
            Self::Nuxt => "Nuxt.js",
            Self::Angular => "Angular (SSR)",
            Self::TanStackStart => "TanStack Start",
        }
    }
}

pub struct DetectedSsr {
    pub framework: SsrFramework,
    pub dockerfile_content: String,
    pub default_port: u16,
}

/// Helper function to check if package.json lists a specific dependency
fn has_dependency(package_json_path: &Path, dep_name: &str) -> bool {
    if package_json_path.exists() {
        if let Ok(content) = std::fs::read_to_string(package_json_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(deps) = val.get("dependencies").and_then(|v| v.as_object()) {
                    if deps.contains_key(dep_name) {
                        return true;
                    }
                }
                if let Some(dev_deps) = val.get("devDependencies").and_then(|v| v.as_object()) {
                    if dev_deps.contains_key(dep_name) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Helper to parse angular.json and find the default or first project name
fn get_angular_project_name(path: &Path) -> String {
    let angular_json_path = path.join("angular.json");
    if angular_json_path.exists() {
        if let Ok(content) = std::fs::read_to_string(angular_json_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(default_proj) = val.get("defaultProject").and_then(|v| v.as_str()) {
                    return default_proj.to_string();
                }
                if let Some(projects) = val.get("projects").and_then(|v| v.as_object()) {
                    if let Some(first_key) = projects.keys().next() {
                        return first_key.clone();
                    }
                }
            }
        }
    }
    "app".to_string()
}

/// Detect if the given directory contains an SSR project
/// and generate a corresponding optimized Dockerfile if found.
pub fn detect_ssr_framework(path: &Path) -> Option<DetectedSsr> {
    let package_json_path = path.join("package.json");

    // 1. Next.js
    if path.join("next.config.js").exists()
        || path.join("next.config.ts").exists()
        || path.join("next.config.mjs").exists()
    {
        let mut has_standalone = false;
        for file_name in &["next.config.js", "next.config.ts", "next.config.mjs"] {
            let p = path.join(file_name);
            if p.exists() {
                if let Ok(content) = std::fs::read_to_string(p) {
                    if content.contains("standalone") {
                        has_standalone = true;
                        break;
                    }
                }
            }
        }

        let dockerfile_content = if has_standalone {
            get_nextjs_standalone_dockerfile()
        } else {
            get_nextjs_fallback_dockerfile()
        };

        return Some(DetectedSsr {
            framework: SsrFramework::NextJs,
            dockerfile_content,
            default_port: 3000,
        });
    }

    // 2. SvelteKit
    if path.join("svelte.config.js").exists() || path.join("svelte.config.ts").exists() {
        return Some(DetectedSsr {
            framework: SsrFramework::SvelteKit,
            dockerfile_content: get_sveltekit_dockerfile(),
            default_port: 3000,
        });
    }

    // 3. Nuxt.js
    if path.join("nuxt.config.js").exists() || path.join("nuxt.config.ts").exists() {
        return Some(DetectedSsr {
            framework: SsrFramework::Nuxt,
            dockerfile_content: get_nuxt_dockerfile(),
            default_port: 3000,
        });
    }

    // 4. Angular SSR
    if path.join("angular.json").exists()
        && (has_dependency(&package_json_path, "@angular/ssr")
            || has_dependency(&package_json_path, "@angular/platform-server"))
    {
        let project_name = get_angular_project_name(path);
        return Some(DetectedSsr {
            framework: SsrFramework::Angular,
            dockerfile_content: get_angular_dockerfile(&project_name),
            default_port: 4000,
        });
    }

    // 5. TanStack Start
    if path.join("app.config.ts").exists()
        || path.join("app.config.js").exists()
        || has_dependency(&package_json_path, "@tanstack/start")
    {
        return Some(DetectedSsr {
            framework: SsrFramework::TanStackStart,
            dockerfile_content: get_tanstack_start_dockerfile(),
            default_port: 3000,
        });
    }

    None
}

// ─── Dockerfile Templates ───────────────────────────────────────────────────

fn get_nextjs_standalone_dockerfile() -> String {
    r#"# Stage 1: Dependencies
FROM node:22-alpine AS deps
RUN apk add --no-cache libc6-compat
WORKDIR /app
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* bun.lockb* ./
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm i --frozen-lockfile; \
  elif [ -f yarn.lock ]; then yarn install --frozen-lockfile; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun install --frozen-lockfile; \
  else npm ci; \
  fi

# Stage 2: Builder
FROM node:22-alpine AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
ENV NEXT_TELEMETRY_DISABLED=1
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm run build; \
  elif [ -f yarn.lock ]; then yarn build; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun run build; \
  else npm run build; \
  fi

# Stage 3: Runner
FROM node:22-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV HOSTNAME="0.0.0.0"

COPY --from=builder /app/public ./public
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static

EXPOSE 3000
CMD ["node", "server.js"]
"#.to_string()
}

fn get_nextjs_fallback_dockerfile() -> String {
    r#"# Stage 1: Dependencies
FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* bun.lockb* ./
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm i; \
  elif [ -f yarn.lock ]; then yarn install; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun install; \
  else npm ci; \
  fi
COPY . .
ENV NEXT_TELEMETRY_DISABLED=1
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm run build; \
  elif [ -f yarn.lock ]; then yarn build; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun run build; \
  else npm run build; \
  fi

# Stage 2: Runner
FROM node:22-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV HOSTNAME="0.0.0.0"
COPY --from=builder /app ./
EXPOSE 3000
CMD ["npm", "start"]
"#.to_string()
}

fn get_sveltekit_dockerfile() -> String {
    r#"# Stage 1: Build
FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* bun.lockb* ./
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm i; \
  elif [ -f yarn.lock ]; then yarn install; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun install; \
  else npm ci; \
  fi
COPY . .
RUN \
  CONFIG_FILE=""; \
  if [ -f svelte.config.js ]; then CONFIG_FILE="svelte.config.js"; \
  elif [ -f svelte.config.ts ]; then CONFIG_FILE="svelte.config.ts"; \
  fi; \
  if [ -n "$CONFIG_FILE" ]; then \
    if ! grep -q "@sveltejs/adapter-node" "$CONFIG_FILE"; then \
      echo "Injecting @sveltejs/adapter-node..."; \
      if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm add -D @sveltejs/adapter-node; \
      elif [ -f yarn.lock ]; then yarn add -D @sveltejs/adapter-node; \
      elif [ -f bun.lockb ]; then corepack enable bun && bun add -D @sveltejs/adapter-node; \
      else npm install --save-dev @sveltejs/adapter-node; \
      fi; \
      sed -i 's/@sveltejs\/adapter-auto/@sveltejs\/adapter-node/g' "$CONFIG_FILE"; \
    fi; \
  fi
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm run build; \
  elif [ -f yarn.lock ]; then yarn build; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun run build; \
  else npm run build; \
  fi

# Stage 2: Runner
FROM node:22-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV HOST=0.0.0.0
COPY --from=builder /app/package.json ./
COPY --from=builder /app/package-lock.json* /app/yarn.lock* /app/pnpm-lock.yaml* /app/bun.lockb* ./
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm i --prod; \
  elif [ -f yarn.lock ]; then yarn install --production; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun install --production; \
  else npm ci --omit=dev; \
  fi
COPY --from=builder /app/build ./build
EXPOSE 3000
CMD ["node", "build/index.js"]
"#.to_string()
}

fn get_nuxt_dockerfile() -> String {
    r#"# Stage 1: Build
FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* bun.lockb* ./
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm i; \
  elif [ -f yarn.lock ]; then yarn install; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun install; \
  else npm ci; \
  fi
COPY . .
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm run build; \
  elif [ -f yarn.lock ]; then yarn build; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun run build; \
  else npm run build; \
  fi

# Stage 2: Runner
FROM node:22-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV HOST=0.0.0.0
COPY --from=builder /app/.output ./.output
EXPOSE 3000
CMD ["node", ".output/server/index.mjs"]
"#.to_string()
}

fn get_angular_dockerfile(project_name: &str) -> String {
    format!(
        r#"# Stage 1: Build
FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* bun.lockb* ./
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm i; \
  elif [ -f yarn.lock ]; then yarn install; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun install; \
  else npm ci; \
  fi
COPY . .
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm run build; \
  elif [ -f yarn.lock ]; then yarn build; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun run build; \
  else npm run build; \
  fi

# Stage 2: Runner
FROM node:22-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=4000
ENV HOST=0.0.0.0
COPY --from=builder /app/dist/{project_name} ./dist/{project_name}
EXPOSE 4000
CMD ["node", "dist/{project_name}/server/server.mjs"]
"#
    )
}

fn get_tanstack_start_dockerfile() -> String {
    r#"# Stage 1: Build
FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json* yarn.lock* pnpm-lock.yaml* bun.lockb* ./
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm i; \
  elif [ -f yarn.lock ]; then yarn install; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun install; \
  else npm ci; \
  fi
COPY . .
RUN \
  if [ -f pnpm-lock.yaml ]; then corepack enable pnpm && pnpm run build; \
  elif [ -f yarn.lock ]; then yarn build; \
  elif [ -f bun.lockb ]; then corepack enable bun && bun run build; \
  else npm run build; \
  fi

# Stage 2: Runner
FROM node:22-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3000
ENV HOST=0.0.0.0
COPY --from=builder /app/.output ./.output
EXPOSE 3000
CMD ["node", ".output/server/index.mjs"]
"#.to_string()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_nextjs_standalone() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        fs::write(path.join("next.config.js"), "module.exports = { output: 'standalone' }").unwrap();

        let detected = detect_ssr_framework(path).unwrap();
        assert_eq!(detected.framework, SsrFramework::NextJs);
        assert_eq!(detected.default_port, 3000);
        assert!(detected.dockerfile_content.contains("standalone"));
    }

    #[test]
    fn test_detect_nextjs_fallback() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        fs::write(path.join("next.config.js"), "module.exports = {}").unwrap();

        let detected = detect_ssr_framework(path).unwrap();
        assert_eq!(detected.framework, SsrFramework::NextJs);
        assert!(detected.dockerfile_content.contains("npm") && detected.dockerfile_content.contains("start"));
    }

    #[test]
    fn test_detect_sveltekit() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        fs::write(path.join("svelte.config.js"), "").unwrap();

        let detected = detect_ssr_framework(path).unwrap();
        assert_eq!(detected.framework, SsrFramework::SvelteKit);
        assert!(detected.dockerfile_content.contains("build/index.js"));
    }

    #[test]
    fn test_detect_nuxt() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        fs::write(path.join("nuxt.config.ts"), "").unwrap();

        let detected = detect_ssr_framework(path).unwrap();
        assert_eq!(detected.framework, SsrFramework::Nuxt);
        assert!(detected.dockerfile_content.contains(".output/server/index.mjs"));
    }

    #[test]
    fn test_detect_angular() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        fs::write(path.join("angular.json"), r#"{"defaultProject": "my-angular-app"}"#).unwrap();
        fs::write(
            path.join("package.json"),
            r#"{"dependencies": {"@angular/ssr": "^17.0.0"}}"#,
        )
        .unwrap();

        let detected = detect_ssr_framework(path).unwrap();
        assert_eq!(detected.framework, SsrFramework::Angular);
        assert_eq!(detected.default_port, 4000);
        assert!(detected.dockerfile_content.contains("my-angular-app"));
        assert!(detected.dockerfile_content.contains("server/server.mjs"));
    }

    #[test]
    fn test_detect_tanstack_start() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        fs::write(path.join("app.config.ts"), "").unwrap();

        let detected = detect_ssr_framework(path).unwrap();
        assert_eq!(detected.framework, SsrFramework::TanStackStart);
        assert_eq!(detected.default_port, 3000);
        assert!(detected.dockerfile_content.contains(".output/server/index.mjs"));
    }

    #[test]
    fn test_detect_none() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        assert!(detect_ssr_framework(path).is_none());
    }
}
