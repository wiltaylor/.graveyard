//! TLS material loader for webservice HTTPS binds.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

pub async fn load_tls_config(cert_path: &Path, key_path: &Path) -> Result<rustls::ServerConfig> {
    let cert_bytes = tokio::fs::read(cert_path)
        .await
        .with_context(|| format!("reading TLS cert {}", cert_path.display()))?;
    let key_bytes = tokio::fs::read(key_path)
        .await
        .with_context(|| format!("reading TLS key {}", key_path.display()))?;

    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_bytes.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("parsing cert PEM {}", cert_path.display()))?;
    if certs.is_empty() {
        return Err(anyhow!("no certificates found in {}", cert_path.display()));
    }

    let key: PrivateKeyDer<'static> = {
        let mut reader = key_bytes.as_slice();
        rustls_pemfile::private_key(&mut reader)
            .with_context(|| format!("parsing key PEM {}", key_path.display()))?
            .ok_or_else(|| anyhow!("no private key found in {}", key_path.display()))?
    };

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("building rustls ServerConfig")?;
    Ok(config)
}
