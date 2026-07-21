use std::net::SocketAddr;
use std::thread::JoinHandle;

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;

use crate::DevHubMcp;

pub struct McpHttpServer {
    port: u16,
    shutdown: CancellationToken,
    _thread: JoinHandle<()>,
}

impl McpHttpServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn start(port: u16, auth_token: Option<String>) -> Result<Self, String> {
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = std::net::TcpListener::bind(address)
            .map_err(|error| format!("binding 127.0.0.1:{port}: {error}"))?;
        let port = listener
            .local_addr()
            .map(|address| address.port())
            .unwrap_or(port);
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("configuring MCP listener: {error}"))?;

        let shutdown = CancellationToken::new();
        let child = shutdown.child_token();
        let thread = std::thread::Builder::new()
            .name("devhub-mcp-http".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("devhub mcp: building runtime: {error}");
                        return;
                    }
                };
                runtime.block_on(serve(listener, child, auth_token));
            })
            .map_err(|error| format!("spawning MCP server thread: {error}"))?;
        Ok(Self {
            port,
            shutdown,
            _thread: thread,
        })
    }

    pub fn stop(self) {
        self.shutdown.cancel();
        // The thread is deliberately detached.
    }
}

impl Drop for McpHttpServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn serve(
    listener: std::net::TcpListener,
    shutdown: CancellationToken,
    auth_token: Option<String>,
) {
    let service = StreamableHttpService::new(
        || Ok(DevHubMcp),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(shutdown.child_token()),
    );
    let mut router = axum::Router::new().nest_service("/mcp", service);
    if let Some(expected) = auth_token.filter(|token| !token.is_empty()) {
        router = router.layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let expected = expected.clone();
                async move {
                    let authorized = request
                        .headers()
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .is_some_and(|value| value == format!("Bearer {expected}"));
                    if authorized {
                        next.run(request).await
                    } else {
                        axum::http::Response::builder()
                            .status(axum::http::StatusCode::UNAUTHORIZED)
                            .body(axum::body::Body::empty())
                            .expect("static 401 response")
                    }
                }
            },
        ));
    }
    let listener = match tokio::net::TcpListener::from_std(listener) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("devhub mcp: converting listener: {error}");
            return;
        }
    };
    let _ = axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await;
}
