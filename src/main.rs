mod db;
mod indexer;
mod prompts;
#[cfg(feature = "semantic")]
mod semantic;
mod server;
mod state;
mod tools;
mod watcher;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use db::Database;
use indexer::languages::LanguageRegistry;
use server::CodeContextServer;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // Parse bind address from env or use default
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let bind_addr = format!("{host}:{port}");

    // Database path from env or default
    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "code_context.db".to_string());

    // Initialize database
    let db = Database::init(std::path::Path::new(&db_path))?;
    tracing::info!(path = %db_path, "database initialized");

    // Initialize language registry
    let registry = LanguageRegistry::new();
    tracing::info!(
        languages = registry.supported_languages().len(),
        "language registry initialized"
    );

    // Build app state
    #[allow(unused_mut)]
    let mut state = AppState::new(db, registry);

    // Initialize semantic engine if feature is enabled
    #[cfg(feature = "semantic")]
    {
        match crate::semantic::SemanticEngine::new() {
            Ok(engine) => {
                state = state.with_semantic(engine);
                tracing::info!("semantic search engine initialized");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to initialize semantic engine, semantic search will be unavailable");
            }
        }
    }

    // Cancellation for graceful shutdown
    let ct = CancellationToken::new();

    // Build the MCP StreamableHttp service
    let config = StreamableHttpServerConfig {
        stateful_mode: true,
        json_response: false,
        sse_keep_alive: None,
        cancellation_token: ct.child_token(),
        ..Default::default()
    };

    let service: StreamableHttpService<CodeContextServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(CodeContextServer::new(state.clone())),
            Default::default(),
            config,
        );

    // Build axum router
    let router = axum::Router::new().nest_service("/mcp", service);

    // Bind and serve
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(address = %bind_addr, "Code Context MCP server listening");

    // Graceful shutdown on ctrl-c
    let ct_shutdown = ct.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutdown signal received");
        ct_shutdown.cancel();
    });

    axum::serve(listener, router)
        .with_graceful_shutdown(async move { ct.cancelled_owned().await })
        .await?;

    tracing::info!("server shut down");
    Ok(())
}
