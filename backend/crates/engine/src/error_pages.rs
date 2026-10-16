//! Branded error pages served for deployed sites and apps.
//!
//! One template renders every status so the pages stay consistent. They are
//! self-contained (no external fonts, scripts or images) because they are
//! served from nginx-static when the thing that failed may be the network or
//! the app itself. Colours are the Drydock tokens from the dashboard, with a
//! dark variant that follows `prefers-color-scheme`.

/// Statuses that get a page under `_errors/<status>.html`.
pub const STATUSES: &[u16] = &[400, 401, 403, 404, 408, 429, 500, 502, 503, 504];

struct Copy {
    title: &'static str,
    body: &'static str,
    /// Offer a "Try again" button: the condition is likely to pass.
    retry: bool,
}

fn copy(status: u16) -> Copy {
    match status {
        400 => Copy { title: "Bad request", body: "The server couldn't understand this request. Check the address and try again.", retry: false },
        401 => Copy { title: "Sign in required", body: "This page needs you to sign in first.", retry: false },
        403 => Copy { title: "Access denied", body: "You don't have permission to view this page.", retry: false },
        404 => Copy { title: "Page not found", body: "This address doesn't lead anywhere. The page may have moved, or the site hasn't been deployed yet.", retry: false },
        408 => Copy { title: "Request timed out", body: "The connection was idle for too long. Try again.", retry: true },
        429 => Copy { title: "Too many requests", body: "You're sending requests faster than this site allows. Wait a moment and try again.", retry: true },
        500 => Copy { title: "Something went wrong", body: "The app hit an error while handling this request. It's been logged for the site's owner.", retry: true },
        502 => Copy { title: "App unreachable", body: "The app didn't answer. It may be starting up, restarting after a deploy, or stopped. This usually clears within a minute.", retry: true },
        503 => Copy { title: "Temporarily unavailable", body: "The app is busy or being updated. Try again in a moment.", retry: true },
        504 => Copy { title: "App took too long to respond", body: "The app didn't reply in time. Try again; if it keeps happening, tell the site's owner.", retry: true },
        _ => Copy { title: "Something went wrong", body: "The request couldn't be completed.", retry: true },
    }
}

/// HTML for one status page.
pub fn render(status: u16) -> String {
    let c = copy(status);
    let retry = if c.retry {
        "<button class=\"btn\" onclick=\"location.reload()\">Try again</button>"
    } else {
        ""
    };
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{status} {title}</title>
<style>
:root {{
  --bg: #ecece8; --surface: #ffffff; --border: #dcdcd6;
  --text: #16181b; --text-2: #41464d; --muted: #5f656c;
  --accent: #c2410c; --accent-fg: #ffffff;
  --sans: 'IBM Plex Sans', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  --mono: 'IBM Plex Mono', ui-monospace, 'SF Mono', Menlo, monospace;
  color-scheme: light;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #0e0f11; --surface: #17191c; --border: #2a2d32;
    --text: #ecedee; --text-2: #c3c6ca; --muted: #9097a0;
    --accent: #f26b1d; --accent-fg: #141619;
    color-scheme: dark;
  }}
}}
*, *::before, *::after {{ box-sizing: border-box; }}
body {{
  margin: 0; min-height: 100vh; display: flex; flex-direction: column;
  background: var(--bg); color: var(--text); font-family: var(--sans);
}}
main {{ flex: 1; display: flex; align-items: center; padding: 48px 24px; }}
.body {{ width: 100%; max-width: 560px; margin: 0 auto; }}
.code {{
  margin: 0 0 8px; font-family: var(--mono); font-weight: 500;
  font-size: clamp(72px, 16vw, 128px); line-height: .9; letter-spacing: -.04em;
}}
.rule {{ width: 48px; height: 3px; background: var(--accent); margin: 20px 0; border: 0; }}
h1 {{ margin: 0 0 12px; font-size: 24px; line-height: 1.25; font-weight: 600; }}
p {{ margin: 0; max-width: 60ch; font-size: 15px; line-height: 1.6; color: var(--text-2); }}
.actions {{ margin-top: 24px; display: flex; flex-wrap: wrap; gap: 8px; }}
.btn {{
  font: inherit; font-size: 14px; font-weight: 500; cursor: pointer;
  padding: 8px 16px; border-radius: 6px; border: 1px solid var(--accent);
  background: var(--accent); color: var(--accent-fg);
}}
.btn.secondary {{ background: transparent; color: var(--text); border-color: var(--border); text-decoration: none; display: inline-block; }}
.btn:focus-visible {{ outline: 2px solid var(--accent); outline-offset: 2px; }}
footer {{
  padding: 16px 24px; border-top: 1px solid var(--border); background: var(--surface);
  font-size: 12px; color: var(--muted); font-family: var(--mono);
}}
</style>
</head>
<body>
<main>
  <div class="body">
    <p class="code" aria-hidden="true">{status}</p>
    <hr class="rule">
    <h1>{title}</h1>
    <p>{body}</p>
    <div class="actions">{retry}<a class="btn secondary" href="/">Go to home page</a></div>
  </div>
</main>
<footer>Served by Shipyard</footer>
</body>
</html>
"#,
        status = status,
        title = c.title,
        body = c.body,
        retry = retry,
    )
}

/// `(file name, html)` for every status page.
pub fn all() -> Vec<(String, String)> {
    STATUSES.iter().map(|s| (format!("{s}.html"), render(*s))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_renders_its_own_code_and_title() {
        for s in STATUSES {
            let html = render(*s);
            assert!(html.contains(&format!("<title>{s} ")), "{s}");
            assert!(html.contains(&format!("aria-hidden=\"true\">{s}</p>")), "{s}");
        }
    }

    #[test]
    fn only_transient_errors_offer_retry() {
        assert!(!render(404).contains("Try again"));
        assert!(!render(403).contains("Try again"));
        for s in [500, 502, 503, 504] {
            assert!(render(s).contains("Try again"), "{s}");
        }
    }

    #[test]
    fn pages_are_self_contained() {
        for (_, html) in all() {
            assert!(!html.contains("http://") && !html.contains("https://"));
            assert!(!html.contains("<script"));
        }
    }
}
