//! 番茄钟云端后端可执行入口。
//!
//! 环境变量：
//! - `DATABASE_URL` 默认 `sqlite://pomodoro.db`
//! - `JWT_SECRET`  默认开发用固定值（生产必须显式设置）
//! - `BIND`       默认 `127.0.0.1:3000`

use anyhow::Result;
use pomodoro_server::{AppState, init_db, router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pomodoro_server=info,tower_http=info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://pomodoro.db".to_string());
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-me".to_string());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());

    let pool = init_db(&db_url).await?;
    let state = AppState { db: pool, jwt_secret };

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    let addr: SocketAddr = listener.local_addr()?;

    tracing::info!("pomodoro-server listening on http://{addr} (db={db_url})");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutting down");
}
