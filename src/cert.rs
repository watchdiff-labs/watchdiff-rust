use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;

#[derive(Debug, Clone)]
pub struct CertInfo {
    pub subject: String,
    pub serial: String,
    pub not_after: DateTime<Utc>,
    pub days_until_expiry: i64,
}

pub async fn fetch_cert(host: &str, port: u16) -> anyhow::Result<CertInfo> {
    let mut root_store = tokio_rustls::rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = tokio_rustls::rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));

    let tcp            = TcpStream::connect((host, port)).await?;
    let server_name    = ServerName::try_from(host.to_string())
        .map_err(|_| anyhow::anyhow!("invalid DNS name '{host}'"))?;
    let tls_stream = connector.connect(server_name, tcp).await?;

    let certs = tls_stream
        .get_ref()
        .1
        .peer_certificates()
        .ok_or_else(|| anyhow::anyhow!("no peer certificates presented by {host}"))?;

    let leaf           = certs.first().ok_or_else(|| anyhow::anyhow!("empty certificate chain"))?;
    let (_, parsed)    = x509_parser::parse_x509_certificate(leaf.as_ref())
        .map_err(|e| anyhow::anyhow!("failed to parse certificate: {e}"))?;

    let not_after    = parsed.validity().not_after.to_datetime();
    let not_after    = DateTime::<Utc>::from_timestamp(not_after.unix_timestamp(), 0)
        .ok_or_else(|| anyhow::anyhow!("invalid certificate expiry timestamp"))?;
    let days_until_expiry = (not_after - Utc::now()).num_days();

    Ok(CertInfo {
        subject: parsed.subject().to_string(),
        serial: parsed.raw_serial_as_string(),
        not_after,
        days_until_expiry,
    })
}
