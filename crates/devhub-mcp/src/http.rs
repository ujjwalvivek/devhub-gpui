use std::net::SocketAddr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;

use crate::DevHubMcp;

pub struct McpHttpServer {
    address: SocketAddr,
    shutdown: CancellationToken,
    thread: Option<JoinHandle<()>>,
    failure: Arc<Mutex<Option<String>>>,
}

impl McpHttpServer {
    pub fn port(&self) -> u16 {
        self.address.port()
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
            && self.failure().is_none()
    }

    pub fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .map(|failure| failure.clone())
            .unwrap_or_else(|_| Some("MCP server failure state is unavailable".to_string()))
    }

    pub fn start(port: u16, auth_token: String) -> Result<Self, String> {
        let auth_token = auth_token.trim().to_string();
        if auth_token.is_empty() {
            return Err("MCP auth token cannot be empty".into());
        }
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = std::net::TcpListener::bind(address)
            .map_err(|error| format!("binding 127.0.0.1:{port}: {error}"))?;
        let address = listener.local_addr().unwrap_or(address);
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("configuring MCP listener: {error}"))?;

        let shutdown = CancellationToken::new();
        let child = shutdown.child_token();
        let failure = Arc::new(Mutex::new(None));
        let thread_failure = failure.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("devhub-mcp-http".to_string())
            .spawn(move || {
                let result = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(serve(listener, child, auth_token, ready_tx)),
                    Err(error) => {
                        let error = format!("building runtime: {error}");
                        let _ = ready_tx.send(Err(error.clone()));
                        Err(error)
                    }
                };
                if let Err(error) = result {
                    eprintln!("devhub mcp: {error}");
                    record_failure(&thread_failure, error);
                }
            })
            .map_err(|error| format!("spawning MCP server thread: {error}"))?;

        let mut server = Self {
            address,
            shutdown,
            thread: Some(thread),
            failure,
        };
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(server),
            Ok(Err(error)) => {
                server.join_thread();
                Err(error)
            }
            Err(_) => {
                server.join_thread();
                Err(server
                    .failure()
                    .unwrap_or_else(|| "MCP server stopped during startup".to_string()))
            }
        }
    }

    pub fn stop(mut self) -> Result<(), String> {
        self.shutdown.cancel();
        self.join_thread();
        self.failure().map_or(Ok(()), Err)
    }

    fn join_thread(&mut self) {
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                record_failure(&self.failure, "server thread panicked".to_string());
            }
        }
    }
}

impl Drop for McpHttpServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.join_thread();
    }
}

fn record_failure(failure: &Mutex<Option<String>>, error: String) {
    if let Ok(mut failure) = failure.lock() {
        *failure = Some(error);
    }
}

async fn serve(
    listener: std::net::TcpListener,
    shutdown: CancellationToken,
    auth_token: String,
    ready: mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let service = StreamableHttpService::new(
        || Ok(DevHubMcp),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_cancellation_token(shutdown.child_token())
            // DevHub tools are independent read-only requests. Stateless JSON avoids
            // holding an SSE session open through reverse proxies such as Tailscale Serve.
            .with_stateful_mode(false)
            .with_json_response(true)
            // Reverse proxies such as Tailscale Serve preserve their public Host header.
            // Loopback exposure and mandatory bearer authentication remain DevHub's boundary.
            .disable_allowed_hosts(),
    );
    let expected = format!("Bearer {auth_token}");
    let router =
        axum::Router::new()
            .nest_service("/mcp", service)
            .layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let expected = expected.clone();
                    async move {
                        let authorized = request
                            .headers()
                            .get(axum::http::header::AUTHORIZATION)
                            .and_then(|value| value.to_str().ok())
                            .is_some_and(|value| value == expected);
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
    let listener = tokio::net::TcpListener::from_std(listener).map_err(|error| {
        let error = format!("converting listener: {error}");
        let _ = ready.send(Err(error.clone()));
        error
    })?;
    let _ = ready.send(Ok(()));
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await
        .map_err(|error| format!("serving HTTP: {error}"))
}
