//! LedgerZero routing server + runtime backend entry point.
//!
//! Usage: ledgerzero-backend [path/to/server.config.toml]

use ledgerzero_backend::app::build_router;
use ledgerzero_backend::config::ServerConfig;
use ledgerzero_backend::state::AppState;
use std::path::Path;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let config_path =
        std::env::args().nth(1).unwrap_or_else(|| "server.config.toml".to_string());
    let config = match ServerConfig::load(Path::new(&config_path)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("Hint: copy server.config.example.toml to server.config.toml");
            std::process::exit(1);
        }
    };
    if config.dev_login.enabled {
        eprintln!("WARNING: dev login is ENABLED — local development only.");
    }

    let listen_addr = config.listen_addr.clone();
    let state = Arc::new(AppState::new(config));
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("cannot bind {listen_addr}: {e}");
            std::process::exit(1);
        });
    println!(
        "LedgerZero backend (engine {}) listening on http://{listen_addr}",
        ledgerzero_engine::ENGINE_VERSION
    );
    axum::serve(listener, app).await.expect("server error");
}
