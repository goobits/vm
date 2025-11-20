pub mod health;
pub mod operations;
pub mod snapshots;
pub mod workspaces;

use crate::{auth::auth_middleware, state::AppState};
use axum::{middleware, Router};
use sqlx::SqlitePool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub async fn create_app(pool: SqlitePool) -> anyhow::Result<Router> {
    let state = AppState::new(pool);

    // Allow CORS for local development (frontend on different port)
    let cors = CorsLayer::permissive();

    let app = Router::new()
        .merge(health::routes()) // Health routes don't need auth
        .merge(
            workspaces::routes()
                .merge(operations::routes())
                .merge(snapshots::routes())
                .layer(middleware::from_fn(auth_middleware)), // Auth for workspaces, operations, and snapshots
        )
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    Ok(app)
}
