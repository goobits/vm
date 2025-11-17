use crate::state::AppState;
use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/ready", get(readiness_check))
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "vm-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn readiness_check(State(state): State<AppState>) -> Json<Value> {
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
