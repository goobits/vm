pub mod health;
pub mod operations;
pub mod snapshots;
pub mod workspaces;

use crate::{api_docs::ApiDoc, auth::auth_middleware, state::AppState};
use axum::{middleware, routing::get, Json, Router};
use sqlx::SqlitePool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub async fn create_app(pool: SqlitePool) -> anyhow::Result<Router> {
    let state = AppState::new(pool);

    // Allow CORS for local development (frontend on different port)
    let cors = CorsLayer::permissive();

    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .merge(health::routes()) // Health routes don't need auth
        .route("/api-docs/openapi.json", get(openapi_spec))
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

async fn openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
