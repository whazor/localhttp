use std::ffi::OsString;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod handoff;
mod port;
mod serve;

#[derive(Parser, Debug)]
#[command(name = "localhttp")]
#[command(about = "Register local app ports and proxy *.localhost to them")]
#[command(
    after_help = "Register an app with `localhttp <app-name>`, for example `localhttp test-app`."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the HTTP/HTTPS reverse proxy server.
    Serve(serve::ServeArgs),
    /// Generate localhost certificates using mkcert.
    Certs(port::CertsArgs),
    /// Show daemon certificate status.
    CertInfo,
    /// List registered apps.
    List,
    /// Remove one registered app, or all apps with --all.
    Clear(port::ClearArgs),
    /// Register an app name and print an available backend port.
    #[command(external_subcommand)]
    App(Vec<OsString>),
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    match Cli::parse().command {
        Command::Serve(args) => serve::serve(args).await,
        Command::Certs(args) => port::generate_certs(args),
        Command::CertInfo => port::cert_info(),
        Command::List => port::list_routes(),
        Command::Clear(args) => port::clear_routes(args),
        Command::App(args) => port::register_app(args),
    }
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("localhttp=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
