use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use devhub_mcp::http::McpHttpServer;

fn post_with_host(
    port: u16,
    host: &str,
    body: &str,
    session: Option<&str>,
    auth: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to MCP server");
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {host}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: 2025-06-18\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(session) = session {
        request.push_str(&format!("Mcp-Session-Id: {session}\r\n"));
    }
    if let Some(auth) = auth {
        request.push_str(&format!("Authorization: Bearer {auth}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, text)
}

fn post(port: u16, body: &str, session: Option<&str>, auth: Option<&str>) -> (u16, String) {
    post_with_host(port, &format!("127.0.0.1:{port}"), body, session, auth)
}

fn header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{}:", name.to_lowercase());
    response
        .lines()
        .find(|line| line.to_lowercase().starts_with(&prefix))
        .and_then(|line| line.split_once(':').map(|(_, value)| value))
        .map(str::trim)
}

fn initialize_body() -> &'static str {
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#
}

#[test]
fn http_server_serves_tools_over_stateless_streamable_http() {
    let server = McpHttpServer::start(0, "secret".to_string()).expect("start server");
    assert!(server.address().ip().is_loopback());

    let (status, response) = post(server.port(), initialize_body(), None, Some("secret"));
    assert_eq!(status, 200, "initialize response: {response}");
    assert!(response.contains("devhub-mcp"), "response: {response}");
    assert!(
        header(&response, "mcp-session-id").is_none(),
        "stateless server issued a session: {response}"
    );
    assert!(
        response
            .to_lowercase()
            .contains("content-type: application/json"),
        "initialize response was not JSON: {response}"
    );

    let (status, _) = post(
        server.port(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        None,
        Some("secret"),
    );
    assert!(status == 200 || status == 202);

    let (status, response) = post(
        server.port(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        None,
        Some("secret"),
    );
    assert_eq!(status, 200, "tools/list response: {response}");
    assert!(response.contains("list_projects"), "response: {response}");

    server.stop().expect("stop server cleanly");
}

#[test]
fn http_server_accepts_tailscale_serve_host_header() {
    let server = McpHttpServer::start(0, "secret".to_string()).expect("start server");

    let (status, response) = post_with_host(
        server.port(),
        "devhub.example-tailnet.ts.net",
        initialize_body(),
        None,
        Some("secret"),
    );

    assert_eq!(status, 200, "tailnet initialize response: {response}");
    server.stop().expect("stop server cleanly");
}

#[test]
fn http_server_always_enforces_bearer_token() {
    assert!(McpHttpServer::start(0, String::new()).is_err());
    let server = McpHttpServer::start(0, "secret".to_string()).expect("start server");

    let (status, _) = post(server.port(), initialize_body(), None, None);
    assert_eq!(status, 401);

    let (status, _) = post(server.port(), initialize_body(), None, Some("wrong"));
    assert_eq!(status, 401);

    let (status, response) = post(server.port(), initialize_body(), None, Some("secret"));
    assert_eq!(status, 200, "authorized response: {response}");

    server.stop().expect("stop server cleanly");
}

#[test]
fn stopping_the_server_closes_the_listener() {
    let server = McpHttpServer::start(0, "secret".to_string()).expect("start server");
    let port = server.port();
    assert!(TcpStream::connect(("127.0.0.1", port)).is_ok());

    server.stop().expect("stop server cleanly");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(("127.0.0.1", port)).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "listener still accepting after stop"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn http_server_survives_concurrent_restart_and_auth_soak() {
    const RESTARTS: usize = 10;
    const CONCURRENT_REQUESTS: usize = 20;
    const TOOLS_LIST: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;

    let started = Instant::now();
    let mut successful_requests = 0;
    for cycle in 0..RESTARTS {
        let token = format!("secret-{cycle}");
        let server = McpHttpServer::start(0, token.clone()).expect("start server");
        let port = server.port();
        let barrier = Arc::new(Barrier::new(CONCURRENT_REQUESTS + 1));
        let responses = std::thread::scope(|scope| {
            let handles = (0..CONCURRENT_REQUESTS)
                .map(|request| {
                    let barrier = barrier.clone();
                    let token = token.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        let body = if request % 2 == 0 {
                            initialize_body()
                        } else {
                            TOOLS_LIST
                        };
                        post_with_host(
                            port,
                            "devhub.example-tailnet.ts.net",
                            body,
                            None,
                            Some(&token),
                        )
                    })
                })
                .collect::<Vec<_>>();
            barrier.wait();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("join HTTP request"))
                .collect::<Vec<_>>()
        });

        for (request, (status, response)) in responses.into_iter().enumerate() {
            assert_eq!(status, 200, "cycle {cycle}, request {request}: {response}");
            let expected = if request % 2 == 0 {
                "devhub-mcp"
            } else {
                "list_projects"
            };
            assert!(
                response.contains(expected),
                "cycle {cycle}, request {request}: {response}"
            );
            successful_requests += 1;
        }

        assert_eq!(post(port, initialize_body(), None, None).0, 401);
        assert_eq!(post(port, initialize_body(), None, Some("wrong")).0, 401);
        server.stop().expect("stop server cleanly");
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    }

    assert_eq!(successful_requests, RESTARTS * CONCURRENT_REQUESTS);
    eprintln!(
        "HTTP soak completed: {successful_requests} concurrent requests, {RESTARTS} restarts in {:?}",
        started.elapsed()
    );
}
