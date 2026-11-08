use crate::types::{DiffReport, EmailAlertConfig};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub async fn send_email(config: &EmailAlertConfig, report: &DiffReport) -> anyhow::Result<()> {
    let from: Mailbox    = config.smtp.user.parse()?;
    let to: Mailbox      = config.to.parse()?;

    let body = report
        .changes
        .iter()
        .map(|c| match (&c.before, &c.after) {
            (None, Some(a)) => format!("[+] Added: {a}"),
            (Some(b), None) => format!("[-] Removed: {b}"),
            (Some(b), Some(a)) => format!("[~] Changed: \"{b}\" -> \"{a}\""),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(format!("WatchDiff: {} changed", report.label))
        .body(body)?;

    let creds = Credentials::new(config.smtp.user.clone(), config.smtp.pass.clone());
    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp.host)?
            .port(config.smtp.port)
            .credentials(creds)
            .build();

    mailer.send(email).await?;
    Ok(())
}
