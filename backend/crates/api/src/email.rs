use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use shipyard_common::config::SmtpConfig;

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

    let from: lettre::Address = format!("{} <{}>", config.from_name, config.from_address)
        .parse()
        .map_err(|e: lettre::address::AddressError| e.to_string())?;

    let to: lettre::Address = to_email
        .parse()
        .map_err(|e: lettre::address::AddressError| e.to_string())?;

    let email = Message::builder()
        .from(from.into())
        .to(to.into())
        .subject(format!("You've been invited to {org_name} on Shipyard"))
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .map_err(|e| e.to_string())?;

    let creds = Credentials::new(config.username.clone(), config.password.clone());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
        .map_err(|e| e.to_string())?
        .port(config.port)
        .credentials(creds)
        .build();

    mailer.send(email).await.map_err(|e| e.to_string())?;
    tracing::info!("Invitation email sent to {to_email}");
    Ok(())
}
