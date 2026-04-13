//! SMTP mail sending via `lettre`.
//!
//! Wraps `lettre::SmtpTransport` (synchronous, blocking I/O). Callers
//! must run `send_mail` inside `tokio::task::spawn_blocking` to avoid
//! blocking the tokio worker threads.

use lettre::message::{header::ContentType, Mailbox};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::time::Duration;

/// SMTP connection + read timeout per attempt.
const SMTP_TIMEOUT: Duration = Duration::from_secs(30);

/// SMTP connection parameters (derived from `SysMailAccount`).
#[derive(Clone)]
pub struct SmtpParams {
    pub host: String,
    pub port: u16,
    pub ssl_enable: bool,
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for SmtpParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpParams")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("ssl_enable", &self.ssl_enable)
            .field("username", &self.username)
            .field("password", &"******")
            .finish()
    }
}

/// A single outbound email message.
#[derive(Debug, Clone)]
pub struct MailMessage {
    pub from_name: String,
    pub from_mail: String,
    pub to_mail: String,
    pub subject: String,
    pub html_body: String,
}

/// Send a single email (blocking). Returns `Ok(())` on success or an
/// error string describing the failure.
pub fn send_mail(smtp: &SmtpParams, msg: &MailMessage) -> Result<(), String> {
    let from: Mailbox = format!("\"{}\" <{}>", msg.from_name.replace('"', ""), msg.from_mail)
        .parse()
        .map_err(|e| format!("invalid from address: {e}"))?;

    let to: Mailbox = msg
        .to_mail
        .parse()
        .map_err(|e| format!("invalid to address: {e}"))?;

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(&msg.subject)
        .header(ContentType::TEXT_HTML)
        .body(msg.html_body.clone())
        .map_err(|e| format!("build message: {e}"))?;

    let creds = Credentials::new(smtp.username.clone(), smtp.password.clone());

    let transport = if smtp.ssl_enable {
        SmtpTransport::relay(&smtp.host)
            .map_err(|e| format!("smtp relay: {e}"))?
            .port(smtp.port)
            .credentials(creds)
            .timeout(Some(SMTP_TIMEOUT))
            .build()
    } else {
        SmtpTransport::builder_dangerous(&smtp.host)
            .port(smtp.port)
            .credentials(creds)
            .timeout(Some(SMTP_TIMEOUT))
            .build()
    };

    transport
        .send(&email)
        .map_err(|e| format!("smtp send: {e}"))?;

    Ok(())
}
