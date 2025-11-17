pub mod health;
pub mod workspaces;

use crate::{auth::auth_middleware, state::AppState};
use axum::{middleware, Router};
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;

pub async fn create_app(pool: SqlitePool) -> anyhow::Result<Router> {
    let state = AppState::new(pool);

    let app = Router::new()
        .merge(health::routes())
        .merge(workspaces::routes())
        .with_state(state)
        .layer(middleware::from_fn(auth_middleware))
        .layer(TraceLayer::new_for_http());

    Ok(app)
}
