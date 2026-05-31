use std::{path::Path, sync::Arc};

use anyhow::{anyhow, bail, Context, Result};
use rustls::{crypto::CryptoProvider, server::ResolvesServerCertUsingSni, sign::CertifiedKey};
use rustls_pemfile::Item;
use rustls_pki_types::PrivateKeyDer;

pub(super) fn ensure_cert_dir(cert_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(cert_dir)
        .with_context(|| format!("failed to create {}", cert_dir.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(cert_dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to lock down {}", cert_dir.display()))?;
    }

    Ok(())
}

pub(super) fn validate_cert_for_host(cert_file: &Path, key_file: &Path, host: &str) -> Result<()> {
    let builder = rustls::ServerConfig::builder();
    let provider = builder.crypto_provider().clone();
    let certified_key = load_certified_key(&provider, cert_file, key_file)?;
    let mut resolver = ResolvesServerCertUsingSni::new();
    resolver
        .add(host, certified_key)
        .with_context(|| format!("certificate is not valid for {host}"))?;
    Ok(())
}

pub(super) fn cert_pair_status(
    cert_file: &Path,
    key_file: &Path,
    host: Option<&str>,
) -> Result<String> {
    if !cert_file.exists() || !key_file.exists() {
        return Ok("missing".to_owned());
    }

    let builder = rustls::ServerConfig::builder();
    let provider = builder.crypto_provider().clone();
    let certified_key = load_certified_key(&provider, cert_file, key_file)?;

    if let Some(host) = host {
        let mut resolver = ResolvesServerCertUsingSni::new();
        resolver
            .add(host, certified_key)
            .with_context(|| format!("certificate is not valid for {host}"))?;
    }

    Ok("ok".to_owned())
}

pub(super) fn load_certified_key(
    provider: &Arc<CryptoProvider>,
    cert_file: &Path,
    key_file: &Path,
) -> Result<CertifiedKey> {
    let cert = std::fs::read(cert_file)
        .with_context(|| format!("failed to read {}", cert_file.display()))?;
    let cert = rustls_pemfile::certs(&mut cert.as_slice())
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse {}", cert_file.display()))?;

    let key = std::fs::read(key_file)
        .with_context(|| format!("failed to read {}", key_file.display()))?;
    let mut keys = rustls_pemfile::read_all(&mut key.as_slice())
        .filter_map(|item| match item.ok()? {
            Item::Sec1Key(key) => Some(key.secret_sec1_der().to_vec()),
            Item::Pkcs1Key(key) => Some(key.secret_pkcs1_der().to_vec()),
            Item::Pkcs8Key(key) => Some(key.secret_pkcs8_der().to_vec()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if keys.len() != 1 {
        bail!("expected exactly one private key in {}", key_file.display());
    }

    let key = PrivateKeyDer::try_from(keys.pop().unwrap())
        .map_err(|err| anyhow!("failed to parse {}: {err}", key_file.display()))?;

    CertifiedKey::from_der(cert, key, provider)
        .with_context(|| format!("failed to load {}", cert_file.display()))
}
