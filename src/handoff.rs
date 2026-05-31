use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::port::Route;

const DEFAULT_RUNTIME_DIR: &str = "/tmp/localhttp";
const DEFAULT_SERVE_SOCKET: &str = "/tmp/localhttp/serve.sock";

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ControlRequest {
    Register {
        name: String,
    },
    List,
    Clear {
        target: ClearTarget,
    },
    InstallCert {
        target: CertTarget,
        cert_pem: String,
        key_pem: String,
    },
    CertInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ClearTarget {
    All,
    App { name: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum CertTarget {
    Default,
    App { name: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ControlResponse {
    Ok,
    Register {
        port: u16,
    },
    List {
        routes: BTreeMap<String, Route>,
    },
    CertInfo {
        cert_dir: String,
        statuses: Vec<CertStatus>,
    },
    Error {
        error: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CertStatus {
    pub(crate) host: String,
    pub(crate) status: String,
}

pub(crate) fn socket_path() -> PathBuf {
    std::env::var("LOCALHTTP_SERVE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_SERVE_SOCKET))
}

pub(crate) fn runtime_dir() -> PathBuf {
    socket_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_RUNTIME_DIR))
}

pub(crate) fn ensure_runtime_dir() -> Result<()> {
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o1777))
            .with_context(|| format!("failed to make {} writable", dir.display()))?;
    }

    Ok(())
}

pub(crate) fn send_control(request: &ControlRequest) -> Result<ControlResponse> {
    let socket = socket_path();
    let mut stream = UnixStream::connect(&socket)
        .with_context(|| format!("failed to connect to {}", socket.display()))?;

    let body = serde_json::to_vec(request).context("failed to encode control request")?;
    stream
        .write_all(&body)
        .context("failed to send control request")?;
    stream
        .shutdown(Shutdown::Write)
        .context("failed to finish control request")?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .context("failed to read control response")?;
    let response: ControlResponse =
        serde_json::from_str(&response).context("failed to decode control response")?;

    if let ControlResponse::Error { error } = response {
        bail!("{error}");
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_register_request() {
        let request = ControlRequest::Register {
            name: "test-app".to_owned(),
        };

        let encoded = serde_json::to_string(&request).unwrap();
        assert!(encoded.contains(r#""type":"register""#));

        let decoded: ControlRequest = serde_json::from_str(&encoded).unwrap();
        assert!(matches!(decoded, ControlRequest::Register { name } if name == "test-app"));
    }
}
