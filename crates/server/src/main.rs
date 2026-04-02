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
    let config_path = crabbit_server::expand_tilde(&args.config);
    let config = crabbit_server::config::Config::load(&config_path)?;

    tracing::info!("opening database at {}", config.db_path);
    let conn = crabbit_server::db::open_db(&config.db_path)?;

    // Recover from any crash that left tasks or agent state stuck
    let recovered = crabbit_server::db::tasks::reset_in_progress_tasks(&conn)?;
    crabbit_server::db::agent::recover_agent_state(&conn)?;
    if recovered > 0 {
        tracing::warn!("startup: reset {} in_progress task(s) to pending", recovered);
    }

    let state = crabbit_server::state::AppState::new(conn, config.clone());
    let router = crabbit_server::routes::build_router(state.clone());

    if config.claude_auth_startup_check {
        let s = state.clone();
        tokio::spawn(async move {
            tracing::info!("startup: checking Claude auth...");
            match crabbit_server::routes::claude_auth::run_check(&s).await {
                Ok(r) => tracing::info!("startup: Claude auth check: {}", r.status),
                Err(e) => tracing::warn!("startup: Claude auth check failed: {}", e),
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!("listening on http://{}", config.bind);
    axum::serve(listener, router).await?;
    Ok(())
}

