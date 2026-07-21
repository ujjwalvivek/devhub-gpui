use devhub_mcp::DevHubMcp;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = DevHubMcp.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
