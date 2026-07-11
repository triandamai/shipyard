use std::time::Duration;
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use shipyard_common::config::SmtpConfig;

/// Hard limit for a single SMTP send — keeps us well inside Cloudflare's
/// 100-second gateway timeout and prevents hung connections.
const SMTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Build an async SMTP transport respecting the configured security mode.
///
/// security = "tls"      → implicit TLS (SMTPS), standard port 465
/// security = "none"     → plain SMTP, no TLS (port 25 / local relay)
/// security = "starttls" → STARTTLS (default), standard port 587
fn build_mailer(config: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let creds = Credentials::new(config.username.clone(), config.password.clone());
    let mailer = match config.security.to_lowercase().as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
            .map_err(|e| e.to_string())?
            .port(config.port)
            .credentials(creds)
            .timeout(Some(SMTP_TIMEOUT))
            .build(),
        "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(creds)
            .timeout(Some(SMTP_TIMEOUT))
            .build(),
        _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
            .map_err(|e| e.to_string())?
            .port(config.port)
            .credentials(creds)
            .timeout(Some(SMTP_TIMEOUT))
            .build(),
    };
    Ok(mailer)
}

/// Load SMTP config from `system_config` DB table, falling back to the
/// static app config (env vars / config file).
pub async fn load_smtp_config(db: &sqlx::PgPool, fallback: &SmtpConfig) -> SmtpConfig {
    let keys = &["smtp_enabled","smtp_host","smtp_port","smtp_username","smtp_password","smtp_from_address","smtp_from_name"];
    let rows: Vec<(String, String)> = match sqlx::query_as(
        "SELECT key, value::text FROM system_config WHERE key = ANY($1)",
    )
    .bind(keys)
    .fetch_all(db)
    .await
    {
        Ok(r) => r,
        Err(_) => return fallback.clone(),
    };

    let map: std::collections::HashMap<String, String> = rows
        .into_iter()
        .map(|(k, v)| (k, v.trim_matches('"').to_string()))
        .collect();

    SmtpConfig {
        enabled:      map.get("smtp_enabled").map(|v| v == "true").unwrap_or(fallback.enabled),
        host:         map.get("smtp_host").cloned().unwrap_or_else(|| fallback.host.clone()),
        port:         map.get("smtp_port").and_then(|v| v.parse().ok()).unwrap_or(fallback.port),
        username:     map.get("smtp_username").cloned().unwrap_or_else(|| fallback.username.clone()),
        password:     map.get("smtp_password").cloned().unwrap_or_else(|| fallback.password.clone()),
        from_address: map.get("smtp_from_address").cloned().unwrap_or_else(|| fallback.from_address.clone()),
        from_name:    map.get("smtp_from_name").cloned().unwrap_or_else(|| fallback.from_name.clone()),
        security:     map.get("smtp_security").cloned().unwrap_or_else(|| fallback.security.clone()),
    }
}

pub async fn send_test_email(
    config: &SmtpConfig,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    if !config.enabled {
        return Err("SMTP is not enabled".to_string());
    }

    // lettre::Address accepts only a bare `user@domain` — the display-name form
    // `"Name <user@domain>"` must be built as a Mailbox instead.
    let from_addr: lettre::Address = config.from_address
        .parse()
        .map_err(|e: lettre::address::AddressError| e.to_string())?;
    let from = lettre::message::Mailbox::new(
        if config.from_name.is_empty() { None } else { Some(config.from_name.clone()) },
        from_addr,
    );

    let to_addr: lettre::Address = to
        .parse()
        .map_err(|e: lettre::address::AddressError| e.to_string())?;

    let email = Message::builder()
        .from(from)
        .to(lettre::message::Mailbox::new(None, to_addr))
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| e.to_string())?;

    tokio::time::timeout(SMTP_TIMEOUT, build_mailer(config)?.send(email))
        .await
        .map_err(|_| "SMTP timed out after 30s — check host/port/firewall".to_string())?
        .map_err(|e| e.to_string())?;
    tracing::info!("Test email sent to {to}");
    Ok(())
}

pub async fn send_invitation_email(
    config: &SmtpConfig,
    to_email: &str,
    org_name: &str,
    invite_token: &str,
    base_url: &str,
) -> Result<(), String> {
    if !config.enabled {
        tracing::info!("SMTP disabled — skipping invitation email to {to_email}");
        return Ok(());
    }

    let accept_url = format!("{base_url}/accept-invite/{invite_token}");
    let body = format!(
        "You've been invited to join {org_name} on Shipyard.\n\nAccept your invitation:\n{accept_url}\n\nThis link expires in 7 days."
    );

    let from_addr: lettre::Address = config.from_address
        .parse()
        .map_err(|e: lettre::address::AddressError| e.to_string())?;
    let from = lettre::message::Mailbox::new(
        if config.from_name.is_empty() { None } else { Some(config.from_name.clone()) },
        from_addr,
    );

    let to_addr: lettre::Address = to_email
        .parse()
        .map_err(|e: lettre::address::AddressError| e.to_string())?;

    let email = Message::builder()
        .from(from)
        .to(lettre::message::Mailbox::new(None, to_addr))
        .subject(format!("You've been invited to {org_name} on Shipyard"))
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .map_err(|e| e.to_string())?;

    tokio::time::timeout(SMTP_TIMEOUT, build_mailer(config)?.send(email))
        .await
        .map_err(|_| "SMTP timed out after 30s — check host/port/firewall".to_string())?
        .map_err(|e| e.to_string())?;
    tracing::info!("Invitation email sent to {to_email}");
    Ok(())
}
