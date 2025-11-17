use crate::state::AppState;
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "vm-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
