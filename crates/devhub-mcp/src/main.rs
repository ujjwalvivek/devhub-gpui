use std::time::Duration;

use devhub_core::{
    generate_mcp_auth_token, load_projects, save_projects, scan_directories_cancellable,
    scan_remote_host_cancellable, sort_projects, CancellationToken, Config, ProjectSource,
};
use devhub_mcp::http::McpHttpServer;
use devhub_mcp::DevHubMcp;
use rmcp::ServiceExt;

const USAGE: &str = "usage: devhub-mcp            serve MCP over stdio (default)
       devhub-mcp --http     serve MCP over HTTP on 127.0.0.1, port and token from DevHub's config
       devhub-mcp --scan     rescan the project catalog (DevHub's Refresh Projects) and save it";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None | Some("--stdio") | Some("--mcp-stdio") => serve_stdio(),
        Some("--http") => serve_http(),
        Some("--scan") => scan(),
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

/// Headless catalog refresh: the same scan as the app's Refresh Projects (local roots and SSH
/// hosts from DevHub's settings), saved to the shared cache that the app and the MCP tools read.
/// Unlike the interactive scan, an unreachable SSH host keeps its last-known projects, so an
/// unattended run never drops a machine that is merely asleep.
fn scan() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load_or_create()?;
    if config.scan_dirs.is_empty() && config.remote_hosts.is_empty() {
        return Err(
            "no scan sources: add a local folder or SSH source in DevHub's settings".into(),
        );
    }
    let cancellation = CancellationToken::new();
    let mut errors = Vec::new();
    let roots: Vec<_> = config
        .scan_dirs
        .iter()
        .filter(|root| match std::fs::metadata(root) {
            Ok(metadata) if metadata.is_dir() => true,
            Ok(_) => {
                errors.push(format!("scan root is not a directory: {}", root.display()));
                false
            }
            Err(error) => {
                errors.push(format!("cannot scan {}: {error}", root.display()));
                false
            }
        })
        .cloned()
        .collect();
    let mut successful_sources = usize::from(!roots.is_empty());
    let mut projects = scan_directories_cancellable(&roots, config.max_depth, &cancellation)?;
    let previous = load_projects().ok().flatten().unwrap_or_default();
    for host in &config.remote_hosts {
        match scan_remote_host_cancellable(host, &cancellation) {
            Ok(mut remote_projects) => {
                successful_sources += 1;
                projects.append(&mut remote_projects);
            }
            Err(error) => {
                errors.push(format!(
                    "{}: {error} (kept its last-known projects)",
                    host.label()
                ));
                projects.extend(previous.iter().filter(|project| {
                    matches!(&project.source, ProjectSource::Remote { name, .. } if name == host.label())
                }).cloned());
            }
        }
    }
    if successful_sources == 0 {
        return Err(format!("no source could be scanned: {}", errors.join(" | ")).into());
    }
    sort_projects(&mut projects);
    save_projects(&projects)?;
    for error in &errors {
        eprintln!("devhub scan: {error}");
    }
    eprintln!("devhub scan: saved {} projects", projects.len());
    Ok(())
}
