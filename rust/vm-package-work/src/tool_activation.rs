use axum::extract::{Extension, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use vm_packages::{
    ClaimToolActivationRequest, FinishToolActivationRequest, PlanToolActivationRequest, SourceKind,
    ToolActivationLease, ToolActivationRecord, ToolActivationState, ToolActivationTarget,
    ToolActivationTargetState, UpdateToolActivationTargetRequest,
};

use crate::server::{auth, AgentAccess, AppState};
use crate::store::{
    ensure_fingerprint, operation_fingerprint, validate_idempotency_key, Database,
    IdempotencyRecord,
};
use crate::{Store, WorkError, WorkResult};

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

pub(crate) fn enqueue(database: &mut Database, release_id: &str) -> WorkResult<()> {
    let release = database
        .releases
        .get(release_id)
        .ok_or_else(|| WorkError::Internal("activation release is missing".into()))?;
    let checkout = database
        .checkouts
        .get(&release.checkout_id)
        .ok_or_else(|| WorkError::Internal("activation checkout is missing".into()))?;
    if checkout.source_kind == SourceKind::Package {
        return Ok(());
    }
    let activation_id = format!("activate-{}", &vm_packages::sha256_hex(release_id)[..32]);
    if database.tool_activations.contains_key(&activation_id) {
        return Ok(());
    }
    let now = Utc::now();
    database.tool_activations.insert(
        activation_id.clone(),
        ToolActivationRecord {
            activation_id,
            release_id: release_id.to_string(),
            checkout_id: release.checkout_id.clone(),
            tool: release.package.clone(),
            version: release.version.clone(),
            source_commit: release.source_commit.clone(),
            state: ToolActivationState::Queued,
            targets: Vec::new(),
            lease: None,
            created_at: now,
            updated_at: now,
        },
    );
    Ok(())
}

impl Store {
    pub async fn tool_activation_for_release(
        &self,
        release_id: &str,
    ) -> WorkResult<ToolActivationRecord> {
        self.database
            .lock()
            .await
            .tool_activations
            .values()
            .find(|activation| activation.release_id == release_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("tool activation for {release_id}")))
    }

    pub async fn tool_activations(&self) -> Vec<ToolActivationRecord> {
        self.database
            .lock()
            .await
            .tool_activations
            .values()
            .cloned()
            .collect()
    }

    pub async fn claim_tool_activation(
        &self,
        activation_id: Option<&str>,
        request: ClaimToolActivationRequest,
    ) -> WorkResult<Option<ToolActivationRecord>> {
        request.validate()?;
        let mut current = self.database.lock().await;
        let now = Utc::now();
        let selected = match activation_id {
            Some(activation_id) => {
                vm_packages::validate_managed_id("tool activation", activation_id)?;
                current
                    .tool_activations
                    .get(activation_id)
                    .filter(|activation| activation.state != ToolActivationState::Complete)
                    .filter(|activation| lease_available(activation, &request.worker, now))
                    .map(|activation| activation.activation_id.clone())
            }
            None => current
                .tool_activations
                .values()
                .filter(|activation| {
                    activation.state == ToolActivationState::Queued
                        || (activation.state == ToolActivationState::Activating
                            && lease_available(activation, &request.worker, now))
                })
                .min_by_key(|activation| activation.created_at)
                .map(|activation| activation.activation_id.clone()),
        };
        let Some(selected) = selected else {
            return Ok(None);
        };
        let mut next = current.clone();
        let activation = next
            .tool_activations
            .get_mut(&selected)
            .expect("selected activation remains present");
        activation.state = ToolActivationState::Activating;
        activation.lease = Some(ToolActivationLease {
            worker: request.worker,
            expires_at: now + Duration::seconds(request.lease_seconds as i64),
        });
        activation.updated_at = now;
        let result = activation.clone();
        self.commit(&mut current, next).await?;
        Ok(Some(result))
    }

    pub async fn plan_tool_activation(
        &self,
        activation_id: &str,
        request: PlanToolActivationRequest,
    ) -> WorkResult<ToolActivationRecord> {
        request.validate()?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint =
            operation_fingerprint("plan_tool_activation", Some(activation_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return activation_by_id(&current, &existing.target_id);
        }
        let now = Utc::now();
        let mut next = current.clone();
        let activation = next
            .tool_activations
            .get_mut(activation_id)
            .ok_or_else(|| WorkError::NotFound(activation_id.to_string()))?;
        ensure_worker_lease(activation, &request.worker, now)?;
        if activation.targets.is_empty() {
            activation.targets = request
                .targets
                .into_iter()
                .map(|target| ToolActivationTarget {
                    target_id: target.target_id,
                    environment: target.environment,
                    provider: target.provider,
                    initially_running: target.initially_running,
                    state: if target.initially_running {
                        ToolActivationTargetState::Pending
                    } else {
                        ToolActivationTargetState::Deferred
                    },
                    attempts: 0,
                    error: None,
                    updated_at: now,
                })
                .collect();
        } else if !plan_matches(activation, &request.targets) {
            return Err(WorkError::Conflict(
                "tool activation target plan is immutable".into(),
            ));
        }
        activation.updated_at = now;
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: activation_id.to_string(),
            },
        );
        let result = activation.clone();
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn update_tool_activation_target(
        &self,
        activation_id: &str,
        target_id: &str,
        request: UpdateToolActivationTargetRequest,
    ) -> WorkResult<ToolActivationRecord> {
        request.validate()?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint = operation_fingerprint(
            "update_tool_activation_target",
            Some(&format!("{activation_id}/{target_id}")),
            &request,
        )?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return activation_by_id(&current, &existing.target_id);
        }
        let now = Utc::now();
        let mut next = current.clone();
        let activation = next
            .tool_activations
            .get_mut(activation_id)
            .ok_or_else(|| WorkError::NotFound(activation_id.to_string()))?;
        ensure_worker_lease(activation, &request.worker, now)?;
        let target = activation
            .targets
            .iter_mut()
            .find(|target| target.target_id == target_id)
            .ok_or_else(|| WorkError::NotFound(target_id.to_string()))?;
        target.state = request.state;
        target.error = request.error;
        target.attempts = target.attempts.saturating_add(1);
        target.updated_at = now;
        activation.updated_at = now;
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: activation_id.to_string(),
            },
        );
        let result = activation.clone();
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn finish_tool_activation(
        &self,
        activation_id: &str,
        request: FinishToolActivationRequest,
    ) -> WorkResult<ToolActivationRecord> {
        request.validate()?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint =
            operation_fingerprint("finish_tool_activation", Some(activation_id), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return activation_by_id(&current, &existing.target_id);
        }
        let now = Utc::now();
        let mut next = current.clone();
        let activation = next
            .tool_activations
            .get_mut(activation_id)
            .ok_or_else(|| WorkError::NotFound(activation_id.to_string()))?;
        ensure_worker_lease(activation, &request.worker, now)?;
        if activation
            .targets
            .iter()
            .any(|target| target.state == ToolActivationTargetState::Pending)
        {
            return Err(WorkError::Conflict(
                "tool activation still has pending targets".into(),
            ));
        }
        activation.state = if activation
            .targets
            .iter()
            .all(|target| target.state == ToolActivationTargetState::Active)
        {
            ToolActivationState::Complete
        } else {
            ToolActivationState::Waiting
        };
        activation.lease = None;
        activation.updated_at = now;
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: activation_id.to_string(),
            },
        );
        let result = activation.clone();
        self.commit(&mut current, next).await?;
        Ok(result)
    }

    pub async fn repair_tool_activations(&self) -> WorkResult<usize> {
        let mut current = self.database.lock().await;
        let now = Utc::now();
        let mut next = current.clone();
        let mut repaired = 0;
        for activation in next.tool_activations.values_mut() {
            if repair_activation(activation, now) {
                repaired += 1;
            }
        }
        if repaired > 0 {
            self.commit(&mut current, next).await?;
        }
        Ok(repaired)
    }
}

fn repair_activation(activation: &mut ToolActivationRecord, now: chrono::DateTime<Utc>) -> bool {
    let expired = activation
        .lease
        .as_ref()
        .is_some_and(|lease| lease.expires_at <= now);
    if expired {
        activation.lease = None;
    }
    let mut failed = false;
    if activation.state == ToolActivationState::Waiting {
        for target in &mut activation.targets {
            if target.state != ToolActivationTargetState::Failed {
                continue;
            }
            target.state = ToolActivationTargetState::Pending;
            target.error = None;
            target.updated_at = now;
            failed = true;
        }
    }
    if expired || failed {
        activation.state = ToolActivationState::Activating;
        activation.updated_at = now;
        true
    } else {
        false
    }
}

fn activation_by_id(database: &Database, activation_id: &str) -> WorkResult<ToolActivationRecord> {
    database
        .tool_activations
        .get(activation_id)
        .cloned()
        .ok_or_else(|| WorkError::Internal("tool activation idempotency target is missing".into()))
}

fn lease_available(
    activation: &ToolActivationRecord,
    worker: &str,
    now: chrono::DateTime<Utc>,
) -> bool {
    activation.lease.as_ref().map_or(true, |lease| {
        lease.worker == worker || lease.expires_at <= now
    })
}

fn ensure_worker_lease(
    activation: &ToolActivationRecord,
    worker: &str,
    now: chrono::DateTime<Utc>,
) -> WorkResult<()> {
    if activation
        .lease
        .as_ref()
        .is_some_and(|lease| lease.worker == worker && lease.expires_at > now)
    {
        Ok(())
    } else {
        Err(WorkError::Conflict(
            "tool activation worker does not hold the current lease".into(),
        ))
    }
}

fn plan_matches(
    activation: &ToolActivationRecord,
    requested: &[vm_packages::ToolActivationTargetPlan],
) -> bool {
    activation.targets.len() == requested.len()
        && activation
            .targets
            .iter()
            .zip(requested)
            .all(|(current, requested)| {
                current.target_id == requested.target_id
                    && current.environment == requested.environment
                    && current.provider == requested.provider
                    && current.initially_running == requested.initially_running
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database_with_release(kind: SourceKind) -> Database {
        let mut database = Database::default();
        let now = Utc::now();
        database.checkouts.insert(
            "checkout-1".into(),
            vm_packages::CheckoutRecord {
                checkout_id: "checkout-1".into(),
                package: "auth".into(),
                source_kind: kind,
                agent: "agent".into(),
                consumers: vec!["project".into()],
                task: "release".into(),
                workspace_release: true,
                source_only: false,
                initial_release: false,
                state: vm_packages::WorkflowState::Published,
                base_branch: Some("main".into()),
                base_commit: Some("a".repeat(40)),
                branch: None,
                worktree: None,
                lease: None,
                created_at: now,
                updated_at: now,
                transitions: Vec::new(),
            },
        );
        database.releases.insert(
            "rel-1".into(),
            vm_packages::ReleaseRecord {
                release_id: "rel-1".into(),
                submission_id: "sub-1".into(),
                checkout_id: "checkout-1".into(),
                package: "auth".into(),
                version: "1.0.0".into(),
                source_repository: "https://example.com/auth.git".into(),
                source_commit: "a".repeat(40),
                tag: "v1.0.0".into(),
                artifact_digest: "b".repeat(64),
                source_pushed: true,
                source_archive_digest: None,
                registry: "https://packages.example/npm".into(),
                expected_publications: Vec::new(),
                publications: Vec::new(),
                state: vm_packages::WorkflowState::Published,
                created_at: now,
                updated_at: now,
            },
        );
        database
    }

    #[test]
    fn release_activation_excludes_language_packages() {
        let mut database = database_with_release(SourceKind::Package);

        enqueue(&mut database, "rel-1").unwrap();
        assert!(database.tool_activations.is_empty());
    }

    #[tokio::test]
    async fn activation_plan_is_durable_idempotent_and_resumable() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        {
            let mut current = store.database.lock().await;
            let mut next = database_with_release(SourceKind::ToolCollection);
            enqueue(&mut next, "rel-1").unwrap();
            enqueue(&mut next, "rel-1").unwrap();
            store.commit(&mut current, next).await.unwrap();
        }
        let claim = ClaimToolActivationRequest {
            worker: "worker-1".into(),
            lease_seconds: 120,
        };
        let activation = store
            .claim_tool_activation(None, claim)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            activation.activation_id,
            format!("activate-{}", &vm_packages::sha256_hex("rel-1")[..32])
        );
        let plan = PlanToolActivationRequest {
            worker: "worker-1".into(),
            targets: vec![
                vm_packages::ToolActivationTargetPlan {
                    target_id: "docker-running".into(),
                    environment: "running-dev".into(),
                    provider: "docker".into(),
                    initially_running: true,
                },
                vm_packages::ToolActivationTargetPlan {
                    target_id: "docker-stopped".into(),
                    environment: "stopped-dev".into(),
                    provider: "docker".into(),
                    initially_running: false,
                },
            ],
            idempotency_key: "activation-plan-1".into(),
        };
        let planned = store
            .plan_tool_activation(&activation.activation_id, plan.clone())
            .await
            .unwrap();
        assert_eq!(planned.targets.len(), 2);
        assert_eq!(
            store
                .plan_tool_activation(&activation.activation_id, plan)
                .await
                .unwrap(),
            planned
        );
        store
            .update_tool_activation_target(
                &activation.activation_id,
                "docker-running",
                UpdateToolActivationTargetRequest {
                    worker: "worker-1".into(),
                    state: ToolActivationTargetState::Active,
                    error: None,
                    idempotency_key: "activate-running-1".into(),
                },
            )
            .await
            .unwrap();
        let waiting = store
            .finish_tool_activation(
                &activation.activation_id,
                FinishToolActivationRequest {
                    worker: "worker-1".into(),
                    idempotency_key: "finish-activation-1".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(waiting.state, ToolActivationState::Waiting);
        drop(store);

        let reopened = Store::open(directory.path()).await.unwrap();
        let resumed = reopened
            .claim_tool_activation(
                Some(&activation.activation_id),
                ClaimToolActivationRequest {
                    worker: "worker-2".into(),
                    lease_seconds: 120,
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resumed.targets.len(), 2);
        reopened
            .update_tool_activation_target(
                &activation.activation_id,
                "docker-stopped",
                UpdateToolActivationTargetRequest {
                    worker: "worker-2".into(),
                    state: ToolActivationTargetState::Active,
                    error: None,
                    idempotency_key: "activate-stopped-1".into(),
                },
            )
            .await
            .unwrap();
        let complete = reopened
            .finish_tool_activation(
                &activation.activation_id,
                FinishToolActivationRequest {
                    worker: "worker-2".into(),
                    idempotency_key: "finish-activation-2".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(complete.state, ToolActivationState::Complete);
    }
}
