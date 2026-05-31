use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    thread,
};

use anyhow::{bail, Context, Result};
use tracing::{info, warn};

use crate::{
    handoff::{
        ensure_runtime_dir, socket_path, CertStatus, CertTarget, ClearTarget, ControlRequest,
        ControlResponse,
    },
    port::{
        allocate_port, app_cert_paths, default_cert_paths_in, ensure_parent_dir,
        normalize_app_name, unix_timestamp, Route,
    },
};

use super::{
    certs::{self, ensure_cert_dir, validate_cert_for_host},
    tls, SharedRegistry,
};

pub(super) fn start_listener(registry: SharedRegistry, cert_dir: PathBuf) -> Result<()> {
    ensure_runtime_dir()?;
    let socket = socket_path();
    if socket.exists() {
        std::fs::remove_file(&socket)
            .with_context(|| format!("failed to remove stale {}", socket.display()))?;
    }

    let listener = UnixListener::bind(&socket)
        .with_context(|| format!("failed to bind {}", socket.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o666))
            .with_context(|| format!("failed to make {} writable", socket.display()))?;
    }

    info!(socket = %socket.display(), "starting serve control socket");
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(err) = handle_control(stream, &registry, &cert_dir) {
                        warn!(error = %err, "failed to handle control request");
                    }
                }
                Err(err) => warn!(error = %err, "failed to accept control request"),
            }
        }
    });

    Ok(())
}

fn handle_control(
    mut stream: UnixStream,
    registry: &SharedRegistry,
    cert_dir: &Path,
) -> Result<()> {
    let mut body = String::new();
    stream
        .read_to_string(&mut body)
        .context("failed to read control request")?;

    let request: ControlRequest =
        serde_json::from_str(&body).context("failed to parse control request")?;
    let response = match handle_request(request, registry, cert_dir) {
        Ok(response) => response,
        Err(err) => ControlResponse::Error {
            error: format!("{err:#}"),
        },
    };

    let response = serde_json::to_vec(&response).context("failed to encode control response")?;
    stream
        .write_all(&response)
        .context("failed to write control response")?;
    Ok(())
}

fn handle_request(
    request: ControlRequest,
    registry: &SharedRegistry,
    cert_dir: &Path,
) -> Result<ControlResponse> {
    match request {
        ControlRequest::Register { name } => {
            let name = normalize_app_name(&name)?;
            let port = allocate_port()?;
            insert_route(registry, name, port)?;
            Ok(ControlResponse::Register { port })
        }
        ControlRequest::List => {
            let registry = registry
                .read()
                .map_err(|_| anyhow::anyhow!("route registry lock is poisoned"))?;
            Ok(ControlResponse::List {
                routes: registry.routes.clone(),
            })
        }
        ControlRequest::Clear { target } => {
            let mut registry = registry
                .write()
                .map_err(|_| anyhow::anyhow!("route registry lock is poisoned"))?;
            match target {
                ClearTarget::All => registry.routes.clear(),
                ClearTarget::App { name } => {
                    let name = normalize_app_name(&name)?;
                    registry.routes.remove(&name);
                }
            }
            Ok(ControlResponse::Ok)
        }
        ControlRequest::InstallCert {
            target,
            cert_pem,
            key_pem,
        } => {
            install_handed_off_cert(cert_dir, target, &cert_pem, &key_pem)?;
            Ok(ControlResponse::Ok)
        }
        ControlRequest::CertInfo => {
            let registry = registry
                .read()
                .map_err(|_| anyhow::anyhow!("route registry lock is poisoned"))?;
            let statuses = cert_statuses(cert_dir, registry.routes.keys())?;
            tls::validate_sni_config(&registry, cert_dir)?;
            Ok(ControlResponse::CertInfo {
                cert_dir: cert_dir.display().to_string(),
                statuses,
            })
        }
    }
}

fn insert_route(registry: &SharedRegistry, name: String, port: u16) -> Result<()> {
    let mut registry = registry
        .write()
        .map_err(|_| anyhow::anyhow!("route registry lock is poisoned"))?;
    registry.routes.insert(
        name,
        Route {
            port,
            updated_at: unix_timestamp()?,
        },
    );
    Ok(())
}

fn cert_statuses<'a, I>(cert_dir: &Path, route_names: I) -> Result<Vec<CertStatus>>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut statuses = Vec::new();
    let (cert_file, key_file) = default_cert_paths_in(cert_dir);
    statuses.push(CertStatus {
        host: "localhost".to_owned(),
        status: certs::cert_pair_status(&cert_file, &key_file, Some("localhost"))?,
    });

    for name in route_names {
        let host = format!("{name}.localhost");
        let (cert_file, key_file) = app_cert_paths(cert_dir, name);
        statuses.push(CertStatus {
            host: host.clone(),
            status: certs::cert_pair_status(&cert_file, &key_file, Some(&host))?,
        });
    }

    Ok(statuses)
}

fn install_handed_off_cert(
    cert_dir: &Path,
    target: CertTarget,
    cert_pem: &str,
    key_pem: &str,
) -> Result<()> {
    ensure_cert_dir(cert_dir)?;

    let (cert_file, key_file, host) = match target {
        CertTarget::Default => {
            let (cert_file, key_file) = default_cert_paths_in(cert_dir);
            (cert_file, key_file, "localhost".to_owned())
        }
        CertTarget::App { name } => {
            validate_handoff_app_name(&name)?;
            let (cert_file, key_file) = app_cert_paths(cert_dir, &name);
            (cert_file, key_file, format!("{name}.localhost"))
        }
    };

    ensure_parent_dir(&cert_file)?;
    ensure_parent_dir(&key_file)?;

    let cert_tmp = cert_file.with_extension("pem.tmp");
    let key_tmp = key_file.with_extension("pem.tmp");
    std::fs::write(&cert_tmp, cert_pem)
        .with_context(|| format!("failed to write {}", cert_tmp.display()))?;
    std::fs::write(&key_tmp, key_pem)
        .with_context(|| format!("failed to write {}", key_tmp.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(&cert_tmp, std::fs::Permissions::from_mode(0o644))
            .with_context(|| format!("failed to chmod {}", cert_tmp.display()))?;
        std::fs::set_permissions(&key_tmp, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to chmod {}", key_tmp.display()))?;
    }

    let result = validate_cert_for_host(&cert_tmp, &key_tmp, &host);
    if let Err(err) = result {
        let _ = std::fs::remove_file(&cert_tmp);
        let _ = std::fs::remove_file(&key_tmp);
        return Err(err);
    }

    std::fs::rename(&cert_tmp, &cert_file)
        .with_context(|| format!("failed to replace {}", cert_file.display()))?;
    std::fs::rename(&key_tmp, &key_file)
        .with_context(|| format!("failed to replace {}", key_file.display()))?;
    info!(host, "installed handed off certificate");
    Ok(())
}

fn validate_handoff_app_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('-')
        || name.ends_with('-')
        || !name
            .chars()
            .all(|char| char.is_ascii_lowercase() || char.is_ascii_digit() || char == '-')
    {
        bail!("invalid app name in cert install: {name}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use super::*;
    use crate::port::Registry;

    #[test]
    fn register_list_and_clear_routes() {
        let registry = Arc::new(RwLock::new(Registry::default()));
        let cert_dir = std::env::temp_dir();

        insert_route(&registry, "test-app".to_owned(), 43210).unwrap();

        let response = handle_request(ControlRequest::List, &registry, &cert_dir).unwrap();
        let routes = match response {
            ControlResponse::List { routes } => routes,
            _ => panic!("unexpected list response"),
        };
        assert!(routes.contains_key("test-app"));

        handle_request(
            ControlRequest::Clear {
                target: ClearTarget::App {
                    name: "test-app".to_owned(),
                },
            },
            &registry,
            &cert_dir,
        )
        .unwrap();

        let response = handle_request(ControlRequest::List, &registry, &cert_dir).unwrap();
        let routes = match response {
            ControlResponse::List { routes } => routes,
            _ => panic!("unexpected list response"),
        };
        assert!(routes.is_empty());
    }
}
