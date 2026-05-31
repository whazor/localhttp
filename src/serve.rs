use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result};
use clap::Parser;
use tokio::signal;
use tracing::info;

use crate::port::{cert_dir, Registry};

mod certs;
mod control;
mod proxy;
mod tls;

pub(super) type SharedRegistry = Arc<RwLock<Registry>>;

#[derive(Parser, Debug)]
pub(crate) struct ServeArgs {
    #[arg(long, env = "LOCALHTTP_HTTP_ADDR", default_value = "0.0.0.0:80")]
    http_addr: SocketAddr,
    #[arg(long, env = "LOCALHTTP_HTTPS_ADDR", default_value = "0.0.0.0:443")]
    https_addr: SocketAddr,
    #[arg(long, env = "LOCALHTTP_CERT_FILE")]
    cert_file: Option<PathBuf>,
    #[arg(long, env = "LOCALHTTP_KEY_FILE")]
    key_file: Option<PathBuf>,
    #[arg(long, env = "LOCALHTTP_HTTP_ONLY")]
    http_only: bool,
}

pub(crate) async fn serve(args: ServeArgs) -> Result<()> {
    let registry = Arc::new(RwLock::new(Registry::default()));
    let cert_dir = cert_dir()?;
    certs::ensure_cert_dir(&cert_dir)?;
    control::start_listener(registry.clone(), cert_dir.clone())?;
    let client = proxy::client();

    info!(addr = %args.http_addr, "starting http server");
    let http = axum_server::bind(args.http_addr)
        .serve(proxy::router(registry.clone(), "http", client.clone()).into_make_service());

    if args.http_only {
        http.await.context("http server failed")?;
        return Ok(());
    }

    let tls_config =
        tls::server_config(args.cert_file, args.key_file, registry.clone(), cert_dir).await?;

    info!(addr = %args.https_addr, "starting https server");
    let https = axum_server::bind_rustls(args.https_addr, tls_config)
        .serve(proxy::router(registry, "https", client).into_make_service());

    tokio::select! {
        result = http => result.context("http server failed")?,
        result = https => result.context("https server failed")?,
        result = signal::ctrl_c() => result.context("failed to listen for ctrl-c")?,
    }

    Ok(())
}
