use crate::state::AppState;
use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/ready", get(readiness_check))
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Health check", body = Value)
    )
)]
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "vm-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[utoipa::path(
    get,
    path = "/health/ready",
    responses(
        (status = 200, description = "Readiness check", body = Value)
    )
)]
pub async fn readiness_check(State(state): State<AppState>) -> Json<Value> {
    // Check database connectivity
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(state.orchestrator.pool())
        .await
        .is_ok();

    Json(json!({
        "status": if db_ok { "ready" } else { "not_ready" },
        "service": "vm-api",
        "version": env!("CARGO_PKG_VERSION"),
        "database": if db_ok { "connected" } else { "disconnected" }
    }))
}
