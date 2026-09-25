use super::*;

pub(crate) fn read_routes() -> Router<AppState> {
    Router::new().route(
        "/v1/releases/{release_id}/tool-activation",
        get(get_release_activation),
    )
}

pub(crate) fn controller_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/tool-activations", get(list_activations))
        .route("/v1/jobs/tool-activation/next", post(claim_next_activation))
        .route("/v1/tool-activations/repair", post(repair_activations))
        .route(
            "/v1/tool-activations/{activation_id}/claim",
            post(claim_activation),
        )
        .route(
            "/v1/tool-activations/{activation_id}/plan",
            post(plan_activation),
        )
        .route(
            "/v1/tool-activations/{activation_id}/targets/{target_id}",
            post(update_target),
        )
        .route(
            "/v1/tool-activations/{activation_id}/finish",
            post(finish_activation),
        )
}

async fn get_release_activation(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(release_id): Path<String>,
) -> WorkResult<Json<ToolActivationRecord>> {
    let activation = state.store.tool_activation_for_release(&release_id).await?;
    let release = state.store.release(&release_id).await?;
    let checkout = state.store.get_checkout(&release.checkout_id).await?;
    auth::ensure_checkout_is_visible(&access, &checkout)?;
    Ok(Json(activation))
}

async fn list_activations(State(state): State<AppState>) -> Json<Vec<ToolActivationRecord>> {
    Json(state.store.tool_activations().await)
}

async fn claim_next_activation(
    State(state): State<AppState>,
    Json(request): Json<ClaimToolActivationRequest>,
) -> WorkResult<Json<Option<ToolActivationRecord>>> {
    Ok(Json(
        state.store.claim_tool_activation(None, request).await?,
    ))
}

async fn claim_activation(
    State(state): State<AppState>,
    Path(activation_id): Path<String>,
    Json(request): Json<ClaimToolActivationRequest>,
) -> WorkResult<Json<Option<ToolActivationRecord>>> {
    Ok(Json(
        state
            .store
            .claim_tool_activation(Some(&activation_id), request)
            .await?,
    ))
}

async fn plan_activation(
    State(state): State<AppState>,
    Path(activation_id): Path<String>,
    Json(request): Json<PlanToolActivationRequest>,
) -> WorkResult<Json<ToolActivationRecord>> {
    Ok(Json(
        state
            .store
            .plan_tool_activation(&activation_id, request)
            .await?,
    ))
}

async fn update_target(
    State(state): State<AppState>,
    Path((activation_id, target_id)): Path<(String, String)>,
    Json(request): Json<UpdateToolActivationTargetRequest>,
) -> WorkResult<Json<ToolActivationRecord>> {
    Ok(Json(
        state
            .store
            .update_tool_activation_target(&activation_id, &target_id, request)
            .await?,
    ))
}

async fn finish_activation(
    State(state): State<AppState>,
    Path(activation_id): Path<String>,
    Json(request): Json<FinishToolActivationRequest>,
) -> WorkResult<Json<ToolActivationRecord>> {
    Ok(Json(
        state
            .store
            .finish_tool_activation(&activation_id, request)
            .await?,
    ))
}

async fn repair_activations(State(state): State<AppState>) -> WorkResult<Json<usize>> {
    Ok(Json(state.store.repair_tool_activations().await?))
}
