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

mod routes;
pub(crate) use routes::{controller_routes, read_routes};

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
        let mut latest_by_tool = std::collections::BTreeMap::new();
        for activation in next.tool_activations.values() {
            let candidate = (activation.created_at, activation.activation_id.clone());
            let latest = latest_by_tool
                .entry(activation.tool.clone())
                .or_insert_with(|| candidate.clone());
            if candidate > *latest {
                *latest = candidate;
            }
        }
        let latest = latest_by_tool
            .into_values()
            .map(|(_, activation_id)| activation_id)
            .collect::<std::collections::BTreeSet<_>>();
        let mut repaired = 0;
        let mut reset_plans = Vec::new();
        for activation in next.tool_activations.values_mut() {
            let repair_key = format!("repair-empty-plan-{}", activation.activation_id);
            let reopen_empty = latest.contains(&activation.activation_id)
                && activation.state == ToolActivationState::Complete
                && activation.targets.is_empty()
                && !next.idempotency.contains_key(&repair_key);
            if repair_activation(activation, now, reopen_empty) {
                repaired += 1;
            }
            if reopen_empty {
                reset_plans.push((
                    format!("plan-{}", activation.activation_id),
                    repair_key,
                    activation.activation_id.clone(),
                ));
            }
        }
        for (plan_key, repair_key, activation_id) in reset_plans {
            next.idempotency.remove(&plan_key);
            next.idempotency.insert(
                repair_key,
                IdempotencyRecord {
                    fingerprint: "repair_empty_activation_plan_v1".into(),
                    target_id: activation_id,
                },
            );
        }
        if repaired > 0 {
            self.commit(&mut current, next).await?;
        }
        Ok(repaired)
    }
}

fn repair_activation(
    activation: &mut ToolActivationRecord,
    now: chrono::DateTime<Utc>,
    reopen_empty: bool,
) -> bool {
    if reopen_empty {
        activation.state = ToolActivationState::Queued;
        activation.lease = None;
        activation.updated_at = now;
        return true;
    }
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
mod tests;
