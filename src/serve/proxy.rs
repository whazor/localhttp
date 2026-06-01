use std::collections::BTreeMap;

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, Request, StatusCode, Uri, Version},
    response::{IntoResponse, Response},
    Router,
};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use tracing::error;

use crate::port::{app_name_from_host, Route};

use super::SharedRegistry;

type ProxyClient = Client<HttpConnector, Body>;

#[derive(Clone)]
struct AppState {
    registry: SharedRegistry,
    forwarded_proto: &'static str,
    client: ProxyClient,
}

pub(super) fn client() -> ProxyClient {
    Client::builder(TokioExecutor::new()).build(HttpConnector::new())
}

pub(super) fn router(
    registry: SharedRegistry,
    forwarded_proto: &'static str,
    client: ProxyClient,
) -> Router {
    Router::new().fallback(proxy).with_state(AppState {
        registry,
        forwarded_proto,
        client,
    })
}

async fn proxy(State(state): State<AppState>, mut request: Request<Body>) -> Response {
    match prepare_proxy_request(&state, &mut request) {
        Ok(true) => match state.client.request(request).await {
            Ok(response) => response.map(Body::new).into_response(),
            Err(err) => {
                error!(error = %err, "backend request failed");
                StatusCode::BAD_GATEWAY.into_response()
            }
        },
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            error!(error = %err, "proxy request failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn prepare_proxy_request(state: &AppState, request: &mut Request<Body>) -> Result<bool> {
    let routes = state
        .registry
        .read()
        .map_err(|_| anyhow::anyhow!("route registry lock is poisoned"))?;
    let Some(target) = proxy_target(&routes.routes, request.headers(), request.uri())? else {
        return Ok(false);
    };
    drop(routes);

    *request.uri_mut() = target.uri;
    *request.version_mut() = Version::HTTP_11;

    let headers = request.headers_mut();
    remove_hop_by_hop_headers(headers);
    headers.insert(
        header::HOST,
        HeaderValue::from_str(&target.forwarded_host).context("invalid forwarded host")?,
    );
    headers.insert(
        "x-forwarded-proto",
        HeaderValue::from_static(state.forwarded_proto),
    );
    headers.insert(
        "x-forwarded-host",
        HeaderValue::from_str(&target.forwarded_host).context("invalid forwarded host")?,
    );

    Ok(true)
}

struct ProxyTarget {
    uri: Uri,
    forwarded_host: String,
}

fn proxy_target(
    routes: &BTreeMap<String, Route>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Option<ProxyTarget>> {
    let Some(host) = request_host(headers, uri) else {
        return Ok(None);
    };

    let Some(name) = app_name_from_host(host) else {
        return Ok(None);
    };

    let forwarded_host = host_name(host);
    let Some(route) = routes.get(name) else {
        return Ok(None);
    };

    let path = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let uri = format!("http://localhost:{}{}", route.port, path)
        .parse()
        .context("failed to build backend URI")?;

    Ok(Some(ProxyTarget {
        uri,
        forwarded_host,
    }))
}

fn remove_hop_by_hop_headers(headers: &mut HeaderMap) {
    headers.remove(header::CONNECTION);
    headers.remove(header::TE);
    headers.remove(header::TRAILER);
    headers.remove(header::TRANSFER_ENCODING);
    headers.remove(header::UPGRADE);
    headers.remove("proxy-authenticate");
    headers.remove("proxy-authorization");
}

fn request_host<'a>(headers: &'a HeaderMap, uri: &'a Uri) -> Option<&'a str> {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| uri.authority().map(|authority| authority.as_str()))
}

fn host_name(host: &str) -> String {
    host.split_once(':')
        .map_or(host, |(host, _)| host)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, RwLock},
        thread,
    };

    use super::*;
    use crate::port::{Registry, Route};

    #[test]
    fn prepares_proxy_request_with_forwarded_headers() {
        let registry = test_registry();

        let state = AppState {
            registry,
            forwarded_proto: "https",
            client: client(),
        };
        let mut request = Request::builder()
            .uri("/reports?range=today")
            .header(header::HOST, "test-app.localhost:443")
            .body(Body::empty())
            .unwrap();

        assert!(prepare_proxy_request(&state, &mut request).unwrap());
        assert_eq!(
            request.uri().to_string(),
            "http://localhost:43210/reports?range=today"
        );
        assert_eq!(request.version(), Version::HTTP_11);
        assert_eq!(request.headers()[header::HOST], "test-app.localhost");
        assert_eq!(request.headers()["x-forwarded-proto"], "https");
        assert_eq!(request.headers()["x-forwarded-host"], "test-app.localhost");
    }

    #[test]
    fn prepares_proxy_request_with_http2_authority() {
        let registry = test_registry();

        let state = AppState {
            registry,
            forwarded_proto: "https",
            client: client(),
        };
        let mut request = Request::builder()
            .uri("https://test-app.localhost/reports?range=today")
            .body(Body::empty())
            .unwrap();

        assert!(prepare_proxy_request(&state, &mut request).unwrap());
        assert_eq!(
            request.uri().to_string(),
            "http://localhost:43210/reports?range=today"
        );
        assert_eq!(request.version(), Version::HTTP_11);
        assert_eq!(request.headers()[header::HOST], "test-app.localhost");
        assert_eq!(request.headers()["x-forwarded-proto"], "https");
        assert_eq!(request.headers()["x-forwarded-host"], "test-app.localhost");
    }

    fn test_registry() -> SharedRegistry {
        Arc::new(RwLock::new(Registry {
            routes: BTreeMap::from([(
                "test-app".to_owned(),
                Route {
                    port: 43210,
                    updated_at: 0,
                },
            )]),
        }))
    }

    #[tokio::test]
    async fn client_reaches_ipv4_only_localhost_backend() {
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("skipping IPv4 localhost test: {err}");
                return;
            }
        };
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_once(listener);

        let response = client()
            .request(
                Request::builder()
                    .uri(format!("http://localhost:{port}/"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn client_reaches_ipv6_only_localhost_backend() {
        let listener = match TcpListener::bind("[::1]:0") {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("skipping IPv6 localhost test: {err}");
                return;
            }
        };
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_once(listener);

        let response = client()
            .request(
                Request::builder()
                    .uri(format!("http://localhost:{port}/"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        server.join().unwrap();
    }

    fn spawn_http_once(listener: TcpListener) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 1024];
            let _ = stream.read(&mut buffer).unwrap();
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        })
    }
}
