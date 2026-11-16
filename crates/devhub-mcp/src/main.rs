use std::time::Duration;

use devhub_core::{generate_mcp_auth_token, Config};
use devhub_mcp::http::McpHttpServer;
use devhub_mcp::DevHubMcp;
use rmcp::ServiceExt;

const USAGE: &str = "usage: devhub-mcp            serve MCP over stdio (default)
       devhub-mcp --http     serve MCP over HTTP on 127.0.0.1, port and token from DevHub's config";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None | Some("--stdio") | Some("--mcp-stdio") => serve_stdio(),
        Some("--http") => serve_http(),
        Some("-h") | Some("--help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown argument {other}\n{USAGE}").into()),
    }
}

fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let service = DevHubMcp.serve(rmcp::transport::stdio()).await?;
            service.waiting().await?;
            Ok(())
        })
}

/// Headless HTTP server: the same server the GUI's toggle starts, without the GUI, so a
/// service manager can keep it up. Uses the GUI's saved port and token; if no token exists
/// yet it creates and saves one, as the GUI does on first start.
fn serve_http() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::load_or_create()?;
    let token = match config.mcp_auth_token.clone() {
        Some(token) if !token.trim().is_empty() => token,
        _ => {
            let token = generate_mcp_auth_token()?;
            config.mcp_auth_token = Some(token.clone());
            config.save()?;
            token
        }
    };
    let server = McpHttpServer::start(config.mcp_http_port, token)?;
    eprintln!("devhub mcp: serving http://{}/mcp", server.address());
    while server.is_running() {
        std::thread::sleep(Duration::from_secs(5));
    }
    Err(server
        .failure()
        .unwrap_or_else(|| "MCP server stopped".to_string())
        .into())
}
