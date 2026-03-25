use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "crabbit-server", about = "Crabbit GitHub agent server")]
struct Args {
    #[arg(short, long, default_value = "~/.config/crabbit/server.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "crabbit_server=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let config_path = expand_tilde(&args.config);
    let config = crabbit_server::config::Config::load(&config_path)?;

    tracing::info!("opening database at {}", config.db_path);
    let conn = crabbit_server::db::open_db(&config.db_path)?;

    let state = crabbit_server::state::AppState::new(conn, config.clone());
    let router = crabbit_server::routes::build_router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!("listening on http://{}", config.bind);
    axum::serve(listener, router).await?;
    Ok(())
}

fn expand_tilde(path: &std::path::Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(&s[2..]);
        }
    }
    path.to_path_buf()
}
