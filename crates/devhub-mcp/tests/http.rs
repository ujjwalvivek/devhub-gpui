use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use devhub_mcp::http::McpHttpServer;

fn post(port: u16, body: &str, session: Option<&str>, auth: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to MCP server");
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nMCP-Protocol-Version: 2025-06-18\r\nContent-Length: {}\r\nConnection: close\r\n",
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
fn http_server_serves_tools_over_streamable_http() {
    let server = McpHttpServer::start(0, None).expect("start server");

    let (status, response) = post(server.port(), initialize_body(), None, None);
    assert_eq!(status, 200, "initialize response: {response}");
    assert!(response.contains("devhub-mcp"), "response: {response}");
    let session = header(&response, "mcp-session-id")
        .expect("session header")
        .to_string();

    let (status, _) = post(
        server.port(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        Some(&session),
        None,
    );
    assert!(status == 200 || status == 202);

    let (status, response) = post(
        server.port(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_projects","arguments":{}}}"#,
        Some(&session),
        None,
    );
    assert_eq!(status, 200, "tools/call response: {response}");
    assert!(response.contains("catalog_as_of"), "response: {response}");

    server.stop();
}

#[test]
fn http_server_enforces_bearer_token_when_configured() {
    let server = McpHttpServer::start(0, Some("secret".to_string())).expect("start server");

    let (status, _) = post(server.port(), initialize_body(), None, None);
    assert_eq!(status, 401);

    let (status, _) = post(server.port(), initialize_body(), None, Some("wrong"));
    assert_eq!(status, 401);

    let (status, response) = post(server.port(), initialize_body(), None, Some("secret"));
    assert_eq!(status, 200, "authorized response: {response}");

    server.stop();
}

#[test]
fn stopping_the_server_closes_the_listener() {
    let server = McpHttpServer::start(0, None).expect("start server");
    let port = server.port();
    assert!(TcpStream::connect(("127.0.0.1", port)).is_ok());

    server.stop();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(("127.0.0.1", port)).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "listener still accepting after stop"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
