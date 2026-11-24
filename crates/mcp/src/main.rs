use rmcp::ServiceExt;
use saispace_core::Db;
use saispace_mcp::{Config, SaiSpaceServer};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = Config::from_env()?;

    let db = Db::open(&config.db_path).await?;

    let server = SaiSpaceServer::new(db, config.agent_name);
    let running_server = server.serve(rmcp::transport::io::stdio_pipes()).await?;
    running_server.waiting().await?;

    Ok(())
}
