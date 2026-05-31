use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{bail, Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use rustls::{server::ResolvesServerCertUsingSni, ServerConfig};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::port::{app_cert_paths, default_cert_paths_in, Registry};

use super::{certs, SharedRegistry};

pub(super) async fn server_config(
    cert_file: Option<PathBuf>,
    key_file: Option<PathBuf>,
    registry: SharedRegistry,
    cert_dir: PathBuf,
) -> Result<RustlsConfig> {
    match (cert_file, key_file) {
        (Some(cert_file), Some(key_file)) => {
            if !cert_file.exists() || !key_file.exists() {
                bail!("missing tls files: {:?} and {:?}", cert_file, key_file);
            }

            let tls_config = RustlsConfig::from_pem_file(&cert_file, &key_file)
                .await
                .with_context(|| {
                    format!("failed to load cert {:?} and key {:?}", cert_file, key_file)
                })?;
            start_cert_reload(tls_config.clone(), cert_file, key_file);
            Ok(tls_config)
        }
        (None, None) => {
            let routes = registry
                .read()
                .map_err(|_| anyhow::anyhow!("route registry lock is poisoned"))?;
            let tls_config =
                RustlsConfig::from_config(Arc::new(build_sni_server_config(&routes, &cert_dir)?));
            drop(routes);
            start_sni_cert_reload(tls_config.clone(), registry, cert_dir);
            Ok(tls_config)
        }
        _ => bail!("pass both --cert-file and --key-file, or neither"),
    }
}

pub(super) fn validate_sni_config(registry: &Registry, cert_dir: &Path) -> Result<()> {
    build_sni_server_config(registry, cert_dir)?;
    Ok(())
}

fn start_cert_reload(config: RustlsConfig, cert_file: PathBuf, key_file: PathBuf) {
    tokio::spawn(async move {
        let mut last_stamp = cert_files_stamp(&cert_file, &key_file);

        loop {
            sleep(Duration::from_secs(1)).await;
            let current_stamp = cert_files_stamp(&cert_file, &key_file);

            if current_stamp.is_some() && current_stamp != last_stamp {
                match config.reload_from_pem_file(&cert_file, &key_file).await {
                    Ok(()) => {
                        last_stamp = current_stamp;
                        info!("reloaded tls certificate");
                    }
                    Err(err) => {
                        warn!(error = %err, "failed to reload tls certificate");
                    }
                }
            }
        }
    });
}

fn start_sni_cert_reload(config: RustlsConfig, registry: SharedRegistry, cert_dir: PathBuf) {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(1)).await;
            let server_config = registry
                .read()
                .map_err(|_| anyhow::anyhow!("route registry lock is poisoned"))
                .and_then(|registry| build_sni_server_config(&registry, &cert_dir));
            match server_config {
                Ok(server_config) => {
                    config.reload_from_config(Arc::new(server_config));
                }
                Err(err) => {
                    warn!(error = %err, "failed to reload sni certificates");
                }
            }
        }
    });
}

fn cert_files_stamp(cert_file: &Path, key_file: &Path) -> Option<(SystemTime, SystemTime)> {
    let cert_modified = std::fs::metadata(cert_file).ok()?.modified().ok()?;
    let key_modified = std::fs::metadata(key_file).ok()?.modified().ok()?;
    Some((cert_modified, key_modified))
}

fn build_sni_server_config(registry: &Registry, cert_dir: &Path) -> Result<ServerConfig> {
    let builder = ServerConfig::builder();
    let provider = builder.crypto_provider().clone();
    let mut resolver = ResolvesServerCertUsingSni::new();

    let (localhost_cert, localhost_key) = default_cert_paths_in(cert_dir);
    if localhost_cert.exists() && localhost_key.exists() {
        resolver
            .add(
                "localhost",
                certs::load_certified_key(&provider, &localhost_cert, &localhost_key)?,
            )
            .context("failed to add localhost certificate")?;
    }

    for name in registry.routes.keys() {
        let host = format!("{name}.localhost");
        let (cert_file, key_file) = app_cert_paths(cert_dir, name);
        if !cert_file.exists() || !key_file.exists() {
            continue;
        }

        resolver
            .add(
                &host,
                certs::load_certified_key(&provider, &cert_file, &key_file)?,
            )
            .with_context(|| format!("failed to add certificate for {host}"))?;
    }

    let mut config = builder
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(resolver));
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}
