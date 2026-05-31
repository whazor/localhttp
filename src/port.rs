use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::handoff::{send_control, CertTarget, ClearTarget, ControlRequest, ControlResponse};

const DEFAULT_CERT_DIR: &str = "/tmp/localhttp/certs";

#[derive(Parser, Debug)]
pub(crate) struct CertsArgs {
    #[arg(long, env = "LOCALHTTP_CERT_FILE")]
    cert_file: Option<PathBuf>,
    #[arg(long, env = "LOCALHTTP_KEY_FILE")]
    key_file: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub(crate) struct ClearArgs {
    name: Option<String>,
    #[arg(long)]
    all: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Registry {
    pub(crate) routes: BTreeMap<String, Route>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Route {
    pub(crate) port: u16,
    pub(crate) updated_at: u64,
}

pub(crate) fn register_app(args: Vec<OsString>) -> Result<()> {
    if args.len() != 1 {
        bail!("usage: localhttp <app-name>");
    }

    let name = args[0]
        .to_str()
        .ok_or_else(|| anyhow!("app name must be valid utf-8"))?;
    let name = normalize_app_name(name)?;
    let response = send_control(&ControlRequest::Register { name: name.clone() })?;
    let port = match response {
        ControlResponse::Register { port } => port,
        _ => bail!("unexpected register response from localhttp serve"),
    };

    if let Err(err) = generate_app_cert(&name, false) {
        eprintln!("warning: failed to install localhttp certificate: {err:#}");
    }

    eprintln!("https://{name}.localhost/");
    println!("{port}");
    Ok(())
}

pub(crate) fn list_routes() -> Result<()> {
    let response = send_control(&ControlRequest::List)?;
    let routes = match response {
        ControlResponse::List { routes } => routes,
        _ => bail!("unexpected list response from localhttp serve"),
    };

    for (name, route) in routes {
        println!("{name}.localhost -> http://127.0.0.1:{}", route.port);
    }

    Ok(())
}

pub(crate) fn clear_routes(args: ClearArgs) -> Result<()> {
    if args.all == args.name.is_some() {
        bail!("pass either `localhttp clear <app-name>` or `localhttp clear --all`");
    }

    let target = if args.all {
        ClearTarget::All
    } else {
        ClearTarget::App {
            name: normalize_app_name(&args.name.expect("validated name exists"))?,
        }
    };

    expect_ok(send_control(&ControlRequest::Clear { target })?)
}

pub(crate) fn generate_certs(args: CertsArgs) -> Result<()> {
    run_mkcert(["-install"], false)?;

    match (args.cert_file, args.key_file) {
        (Some(cert_file), Some(key_file)) => {
            generate_static_cert_files(&cert_file, &key_file)?;
            println!("{}", cert_file.display());
            println!("{}", key_file.display());
        }
        (None, None) => generate_default_certs(true)?,
        _ => bail!("pass both --cert-file and --key-file, or neither"),
    }

    Ok(())
}

pub(crate) fn cert_info() -> Result<()> {
    let response = send_control(&ControlRequest::CertInfo)?;
    let (cert_dir, statuses) = match response {
        ControlResponse::CertInfo { cert_dir, statuses } => (cert_dir, statuses),
        _ => bail!("unexpected cert-info response from localhttp serve"),
    };

    println!("cert dir: {cert_dir}");
    for status in statuses {
        println!("{}: {}", status.host, status.status);
    }
    println!("sni resolver: ok");
    Ok(())
}

fn generate_default_certs(print_paths: bool) -> Result<()> {
    let cert_dir = cert_dir()?;
    let (cert_file, key_file) = default_cert_paths_in(&cert_dir);
    generate_cert_for_control(
        CertTarget::Default,
        ["localhost", "127.0.0.1", "::1"],
        !print_paths,
    )?;
    if print_paths {
        println!("{}", cert_file.display());
        println!("{}", key_file.display());
    }

    let response = send_control(&ControlRequest::List)?;
    let routes = match response {
        ControlResponse::List { routes } => routes,
        _ => bail!("unexpected list response from localhttp serve"),
    };
    for name in routes.keys() {
        generate_app_cert(name, print_paths)?;
    }

    Ok(())
}

fn generate_app_cert(name: &str, print_paths: bool) -> Result<()> {
    run_mkcert(["-install"], true)?;

    let cert_dir = cert_dir()?;
    let (cert_file, key_file) = app_cert_paths(&cert_dir, name);
    let host = format!("{name}.localhost");
    generate_cert_for_control(
        CertTarget::App {
            name: name.to_owned(),
        },
        [host.as_str()],
        !print_paths,
    )?;
    if print_paths {
        println!("{}", cert_file.display());
        println!("{}", key_file.display());
    }
    Ok(())
}

fn generate_cert_for_control<I, S>(target: CertTarget, subjects: I, quiet: bool) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let temp_dir = temp_cert_dir()?;
    let cert_file = temp_dir.join("cert.pem");
    let key_file = temp_dir.join("key.pem");

    let result = (|| -> Result<()> {
        generate_cert_files(&cert_file, &key_file, subjects, quiet)?;

        let cert_pem = std::fs::read_to_string(&cert_file)
            .with_context(|| format!("failed to read {}", cert_file.display()))?;
        let key_pem = std::fs::read_to_string(&key_file)
            .with_context(|| format!("failed to read {}", key_file.display()))?;

        expect_ok(send_control(&ControlRequest::InstallCert {
            target,
            cert_pem,
            key_pem,
        })?)
    })();

    let _ = std::fs::remove_dir_all(&temp_dir);
    result
}

fn generate_static_cert_files(cert_file: &Path, key_file: &Path) -> Result<()> {
    generate_cert_files(
        cert_file,
        key_file,
        ["localhost", "127.0.0.1", "::1"],
        false,
    )
}

fn generate_cert_files<I, S>(
    cert_file: &Path,
    key_file: &Path,
    subjects: I,
    quiet: bool,
) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    ensure_parent_dir(cert_file)?;
    ensure_parent_dir(key_file)?;

    let cert_file_arg = path_arg(cert_file)?;
    let key_file_arg = path_arg(key_file)?;
    let mut args = vec![
        OsString::from("-cert-file"),
        OsString::from(cert_file_arg),
        OsString::from("-key-file"),
        OsString::from(key_file_arg),
    ];
    args.extend(
        subjects
            .into_iter()
            .map(|subject| subject.as_ref().to_owned()),
    );

    run_mkcert(args, quiet)
}

fn run_mkcert<I, S>(args: I, quiet: bool) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = std::process::Command::new("mkcert");
    command.args(args);
    if quiet {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let status = command
        .status()
        .context("failed to run mkcert; install it or enter the Nix dev shell")?;

    if !status.success() {
        bail!("mkcert exited with {status}");
    }

    Ok(())
}

fn path_arg(path: &Path) -> Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("path is not valid utf-8: {:?}", path))
}

fn temp_cert_dir() -> Result<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before unix epoch")?
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("localhttp-mkcert-{}-{suffix}", std::process::id()));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to lock down {}", dir.display()))?;
    }

    Ok(dir)
}

pub(crate) fn app_name_from_host(host: &str) -> Option<&str> {
    let host = host.split_once(':').map_or(host, |(host, _)| host);
    host.strip_suffix(".localhost")
        .filter(|name| !name.is_empty())
}

pub(crate) fn normalize_app_name(name: &str) -> Result<String> {
    let name = name
        .trim()
        .trim_end_matches(".localhost")
        .to_ascii_lowercase();

    if name.is_empty() {
        bail!("app name cannot be empty");
    }

    if name.starts_with('-') || name.ends_with('-') {
        bail!("app name cannot start or end with '-'");
    }

    if !name
        .chars()
        .all(|char| char.is_ascii_lowercase() || char.is_ascii_digit() || char == '-')
    {
        bail!("app name can only contain ascii letters, digits, and '-'");
    }

    Ok(name)
}

pub(crate) fn allocate_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("failed to allocate local port")?;
    Ok(listener.local_addr()?.port())
}

pub(crate) fn cert_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("LOCALHTTP_CERT_DIR") {
        return Ok(PathBuf::from(dir));
    }

    Ok(PathBuf::from(DEFAULT_CERT_DIR))
}

pub(crate) fn default_cert_paths_in(cert_dir: &Path) -> (PathBuf, PathBuf) {
    (
        cert_dir.join("localhttp.pem"),
        cert_dir.join("localhttp-key.pem"),
    )
}

pub(crate) fn app_cert_paths(cert_dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let host = format!("{name}.localhost");
    (
        cert_dir.join(format!("{host}.pem")),
        cert_dir.join(format!("{host}-key.pem")),
    )
}

pub(crate) fn ensure_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    Ok(())
}

pub(crate) fn unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before unix epoch")?
        .as_secs())
}

fn expect_ok(response: ControlResponse) -> Result<()> {
    match response {
        ControlResponse::Ok => Ok(()),
        _ => bail!("unexpected response from localhttp serve"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_app_name_from_localhost_host() {
        assert_eq!(app_name_from_host("test-app.localhost"), Some("test-app"));
        assert_eq!(
            app_name_from_host("test-app.localhost:443"),
            Some("test-app")
        );
        assert_eq!(app_name_from_host("localhost"), None);
    }

    #[test]
    fn normalizes_app_names() {
        assert_eq!(
            normalize_app_name("Test-App.localhost").unwrap(),
            "test-app"
        );
        assert!(normalize_app_name("-bad").is_err());
        assert!(normalize_app_name("bad_name").is_err());
    }
}
