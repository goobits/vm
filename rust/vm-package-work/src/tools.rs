use std::cmp::Ordering;
use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use semver::Version;
use serde::Deserialize;
use vm_packages::{
    tool_artifact_key, tool_artifact_path, validate_tool_name, validate_tool_target,
    validate_tool_version, PublishToolArtifact, RegisterTool, ToolArtifactRecord, ToolDefinition,
    ToolIndex, ToolInventory, ToolPublicationReceipt,
};

use crate::server::AppState;
use crate::store::{
    ensure_fingerprint, next_id, operation_fingerprint, validate_idempotency_key, IdempotencyRecord,
};
use crate::{Store, WorkError, WorkResult};

pub(crate) fn read_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/tools", get(list_tools))
        .route("/v1/tools/index", get(get_tool_index))
        .route("/v1/tools/{name}", get(get_tool))
        .route("/v1/tools/{name}/resolve", get(resolve_tool))
        .route("/v1/tool-receipts/{receipt_id}", get(get_tool_receipt))
}

pub(crate) fn controller_routes() -> Router<AppState> {
    Router::new().route("/v1/tools", post(register_tool))
}

pub(crate) fn release_routes() -> Router<AppState> {
    Router::new().route("/v1/tools/{name}/artifacts", post(publish_tool_artifact))
}

#[derive(Deserialize)]
struct ResolveQuery {
    #[serde(default)]
    version: Option<String>,
    target: String,
}

#[derive(Deserialize)]
struct TargetQuery {
    target: String,
}

async fn list_tools(State(state): State<AppState>) -> Json<Vec<ToolDefinition>> {
    Json(state.store.tools().await)
}

async fn get_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> WorkResult<Json<ToolInventory>> {
    Ok(Json(state.store.tool(&name).await?))
}

async fn resolve_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ResolveQuery>,
) -> WorkResult<Json<ToolArtifactRecord>> {
    Ok(Json(
        state
            .store
            .resolve_tool(&name, query.version.as_deref(), &query.target)
            .await?,
    ))
}

async fn get_tool_index(
    State(state): State<AppState>,
    Query(query): Query<TargetQuery>,
) -> WorkResult<Json<ToolIndex>> {
    Ok(Json(state.store.tool_index(&query.target).await?))
}

async fn get_tool_receipt(
    State(state): State<AppState>,
    Path(receipt_id): Path<String>,
) -> WorkResult<Json<ToolPublicationReceipt>> {
    Ok(Json(state.store.tool_receipt(&receipt_id).await?))
}

async fn register_tool(
    State(state): State<AppState>,
    Json(request): Json<RegisterTool>,
) -> WorkResult<(StatusCode, Json<ToolDefinition>)> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.register_tool(request).await?),
    ))
}

async fn publish_tool_artifact(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<PublishToolArtifact>,
) -> WorkResult<(StatusCode, Json<ToolArtifactRecord>)> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.publish_tool_artifact(&name, request).await?),
    ))
}

impl Store {
    pub async fn register_tool(&self, request: RegisterTool) -> WorkResult<ToolDefinition> {
        request.validate()?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.tools.get(&request.name) {
            if existing.kind == request.kind
                && existing.repository == request.repository
                && existing.default_branch == request.default_branch
            {
                return Ok(existing.clone());
            }
            return Err(WorkError::Conflict(format!(
                "tool '{}' is already registered with different settings",
                request.name
            )));
        }

        let definition = ToolDefinition {
            name: request.name,
            kind: request.kind,
            repository: request.repository,
            default_branch: request.default_branch,
            registered_at: Utc::now(),
        };
        let mut next = current.clone();
        next.tools
            .insert(definition.name.clone(), definition.clone());
        self.commit(&mut current, next).await?;
        Ok(definition)
    }

    pub async fn tools(&self) -> Vec<ToolDefinition> {
        self.database.lock().await.tools.values().cloned().collect()
    }

    pub async fn tool(&self, name: &str) -> WorkResult<ToolInventory> {
        validate_tool_name(name)?;
        let database = self.database.lock().await;
        let definition = database
            .tools
            .get(name)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("tool {name}")))?;
        let mut artifacts = database
            .tool_artifacts
            .values()
            .filter(|artifact| artifact.tool == name)
            .cloned()
            .collect::<Vec<_>>();
        artifacts.sort_by(compare_artifacts);
        Ok(ToolInventory {
            definition,
            artifacts,
        })
    }

    pub async fn publish_tool_artifact(
        &self,
        name: &str,
        request: PublishToolArtifact,
    ) -> WorkResult<ToolArtifactRecord> {
        validate_tool_name(name)?;
        request.validate()?;
        validate_idempotency_key(&request.idempotency_key)?;
        let fingerprint = operation_fingerprint("publish_tool", Some(name), &request)?;
        let mut current = self.database.lock().await;
        if let Some(existing) = current.idempotency.get(&request.idempotency_key) {
            ensure_fingerprint(existing, &fingerprint)?;
            return current
                .tool_artifacts
                .get(&existing.target_id)
                .cloned()
                .ok_or_else(|| WorkError::Internal("tool idempotency target is missing".into()));
        }

        let definition = current
            .tools
            .get(name)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("tool {name}")))?;
        let key = tool_artifact_key(name, &request.version, &request.target);
        if let Some(existing) = current.tool_artifacts.get(&key).cloned() {
            if artifact_matches(&existing, &request, &definition) {
                let mut next = current.clone();
                next.idempotency.insert(
                    request.idempotency_key,
                    IdempotencyRecord {
                        fingerprint,
                        target_id: key,
                    },
                );
                self.commit(&mut current, next).await?;
                return Ok(existing);
            }
            return Err(WorkError::Conflict(format!(
                "tool artifact {key} is immutable and already exists"
            )));
        }

        let mut next = current.clone();
        let now = Utc::now();
        let receipt_id = format!("tool-receipt-{:08}", next_id(&mut next));
        let artifact = ToolArtifactRecord {
            tool: name.to_string(),
            kind: definition.kind,
            version: request.version,
            target: request.target,
            artifact_digest: request.artifact_digest,
            size_bytes: request.size_bytes,
            links: request.links,
            source_repository: definition.repository,
            source_commit: request.source_commit,
            tag: request.tag,
            artifact_path: String::new(),
            actor: request.actor,
            published_at: now,
            receipt_id: receipt_id.clone(),
        };
        let mut artifact = artifact;
        artifact.artifact_path = tool_artifact_path(
            &artifact.tool,
            &artifact.version,
            &artifact.target,
            &artifact.artifact_digest,
        );
        let receipt = ToolPublicationReceipt {
            receipt_id: receipt_id.clone(),
            tool: artifact.tool.clone(),
            kind: artifact.kind,
            version: artifact.version.clone(),
            target: artifact.target.clone(),
            source_repository: artifact.source_repository.clone(),
            source_commit: artifact.source_commit.clone(),
            tag: artifact.tag.clone(),
            artifact_digest: artifact.artifact_digest.clone(),
            size_bytes: artifact.size_bytes,
            actor: artifact.actor.clone(),
            timestamp: now,
        };
        next.idempotency.insert(
            request.idempotency_key,
            IdempotencyRecord {
                fingerprint,
                target_id: key.clone(),
            },
        );
        next.tool_artifacts.insert(key, artifact.clone());
        next.tool_receipts.insert(receipt_id, receipt);
        self.commit(&mut current, next).await?;
        Ok(artifact)
    }

    pub async fn resolve_tool(
        &self,
        name: &str,
        version: Option<&str>,
        target: &str,
    ) -> WorkResult<ToolArtifactRecord> {
        validate_tool_name(name)?;
        validate_tool_target(target)?;
        if let Some(version) = version.filter(|version| *version != "latest") {
            validate_tool_version(version)?;
        }
        let database = self.database.lock().await;
        if !database.tools.contains_key(name) {
            return Err(WorkError::NotFound(format!("tool {name}")));
        }
        select_artifact(database.tool_artifacts.values(), name, version, target)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("tool artifact {name} for {target}")))
    }

    pub async fn tool_index(&self, target: &str) -> WorkResult<ToolIndex> {
        validate_tool_target(target)?;
        let database = self.database.lock().await;
        let tools = database
            .tools
            .keys()
            .filter_map(|name| {
                select_artifact(database.tool_artifacts.values(), name, None, target)
                    .cloned()
                    .map(|artifact| (name.clone(), artifact))
            })
            .collect::<BTreeMap<_, _>>();
        Ok(ToolIndex {
            target: target.to_string(),
            generated_at: Utc::now(),
            tools,
        })
    }

    pub async fn tool_receipt(&self, receipt_id: &str) -> WorkResult<ToolPublicationReceipt> {
        self.database
            .lock()
            .await
            .tool_receipts
            .get(receipt_id)
            .cloned()
            .ok_or_else(|| WorkError::NotFound(format!("tool receipt {receipt_id}")))
    }
}

fn select_artifact<'a>(
    artifacts: impl Iterator<Item = &'a ToolArtifactRecord>,
    name: &str,
    requested_version: Option<&str>,
    target: &str,
) -> Option<&'a ToolArtifactRecord> {
    let exact_version = requested_version.filter(|version| *version != "latest");
    artifacts
        .filter(|artifact| {
            artifact.tool == name && (artifact.target == "any" || artifact.target == target)
        })
        .filter(|artifact| exact_version.map_or(true, |version| artifact.version == version))
        .filter_map(|artifact| {
            Version::parse(&artifact.version)
                .ok()
                .filter(|version| exact_version.is_some() || version.pre.is_empty())
                .map(|version| (artifact, version))
        })
        .max_by(|(left, left_version), (right, right_version)| {
            left_version.cmp(right_version).then_with(|| {
                target_preference(left, target).cmp(&target_preference(right, target))
            })
        })
        .map(|(artifact, _)| artifact)
}

fn target_preference(artifact: &ToolArtifactRecord, target: &str) -> u8 {
    u8::from(artifact.target == target)
}

fn compare_artifacts(left: &ToolArtifactRecord, right: &ToolArtifactRecord) -> Ordering {
    Version::parse(&left.version)
        .ok()
        .cmp(&Version::parse(&right.version).ok())
        .then_with(|| left.target.cmp(&right.target))
}

fn artifact_matches(
    artifact: &ToolArtifactRecord,
    request: &PublishToolArtifact,
    definition: &ToolDefinition,
) -> bool {
    artifact.kind == definition.kind
        && artifact.version == request.version
        && artifact.target == request.target
        && artifact.artifact_digest == request.artifact_digest
        && artifact.size_bytes == request.size_bytes
        && artifact.links == request.links
        && artifact.source_repository == definition.repository
        && artifact.source_commit == request.source_commit
        && artifact.tag == request.tag
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_packages::ToolKind;

    fn definition(name: &str, kind: ToolKind) -> RegisterTool {
        RegisterTool {
            name: name.into(),
            kind,
            repository: format!("https://example.com/{name}.git"),
            default_branch: "main".into(),
        }
    }

    fn publication(version: &str, target: &str, key: &str) -> PublishToolArtifact {
        PublishToolArtifact {
            version: version.into(),
            target: target.into(),
            artifact_digest: format!("{:0<64}", version.replace('.', "")),
            size_bytes: 42,
            links: BTreeMap::from([(".local/bin/tool".into(), "bin/tool".into())]),
            source_commit: "a".repeat(40),
            tag: format!("v{version}"),
            actor: "release-service".into(),
            idempotency_key: key.into(),
        }
    }

    #[tokio::test]
    async fn immutable_publications_resolve_latest_and_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        store
            .register_tool(definition("codex", ToolKind::Binary))
            .await
            .unwrap();
        let first = store
            .publish_tool_artifact("codex", publication("1.0.0", "any", "publish-1"))
            .await
            .unwrap();
        assert_eq!(
            first,
            store
                .publish_tool_artifact("codex", publication("1.0.0", "any", "publish-1"))
                .await
                .unwrap()
        );
        store
            .publish_tool_artifact("codex", publication("1.1.0", "linux-arm64", "publish-2"))
            .await
            .unwrap();
        assert!(store
            .publish_tool_artifact("codex", publication("1.0.0", "any", "different"))
            .await
            .is_ok());
        let mut conflict = publication("1.0.0", "any", "conflict");
        conflict.artifact_digest = "f".repeat(64);
        assert!(store
            .publish_tool_artifact("codex", conflict)
            .await
            .is_err());

        let latest = store
            .resolve_tool("codex", Some("latest"), "linux-arm64")
            .await
            .unwrap();
        assert_eq!(latest.version, "1.1.0");
        let index = store.tool_index("linux-arm64").await.unwrap();
        assert_eq!(index.tools["codex"].artifact_digest, latest.artifact_digest);
        let receipt = store.tool_receipt(&latest.receipt_id).await.unwrap();
        assert_eq!(receipt.source_commit, latest.source_commit);

        drop(store);
        let reopened = Store::open(directory.path()).await.unwrap();
        assert_eq!(
            reopened
                .resolve_tool("codex", Some("1.0.0"), "linux-amd64")
                .await
                .unwrap(),
            first
        );
        assert!(directory
            .path()
            .join("receipts/tools")
            .join(format!("{}.json", latest.receipt_id))
            .is_file());
    }

    #[tokio::test]
    async fn collection_is_one_atomic_target_independent_release() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).await.unwrap();
        store
            .register_tool(definition("agent-skills", ToolKind::Collection))
            .await
            .unwrap();
        let mut release = publication("3.0.0", "any", "skills-3");
        release.links = BTreeMap::from([(".codex/skills".into(), "skills".into())]);
        store
            .publish_tool_artifact("agent-skills", release)
            .await
            .unwrap();
        assert_eq!(
            store
                .resolve_tool("agent-skills", None, "linux-amd64")
                .await
                .unwrap()
                .version,
            "3.0.0"
        );
    }
}
