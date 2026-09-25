use super::*;

pub(super) fn validate_build_request(request: &CompleteToolBuildRequest) -> WorkResult<()> {
    validate_label("build actor", &request.actor)?;
    validate_idempotency_key(&request.idempotency_key)?;
    validate_sha256(&request.manifest_digest)?;
    if matches!(
        (
            request.failure.as_ref(),
            request.failure_kind,
            request.artifacts.is_empty()
        ),
        (Some(_), _, false) | (None, Some(_), _) | (None, None, true)
    ) {
        return Err(WorkError::Invalid(
            "tool build must contain either artifacts or one failure".into(),
        ));
    }
    if let Some(failure) = &request.failure {
        if failure.trim().is_empty() || failure.len() > 4_000 {
            return Err(WorkError::Invalid(
                "tool build failure must contain 1 to 4000 characters".into(),
            ));
        }
        return Ok(());
    }
    if request.artifacts.len() > 16 {
        return Err(WorkError::Invalid(
            "tool build cannot contain more than 16 artifacts".into(),
        ));
    }
    let mut targets = BTreeSet::new();
    for artifact in &request.artifacts {
        if !targets.insert(&artifact.target) {
            return Err(WorkError::Invalid(
                "tool build artifact targets must be unique".into(),
            ));
        }
        PublishToolArtifact {
            version: request.version.clone(),
            target: artifact.target.clone(),
            artifact_digest: artifact.artifact_digest.clone(),
            size_bytes: artifact.size_bytes,
            links: artifact.links.clone(),
            source_commit: request.source_commit.clone(),
            tag: format!("v{}", request.version),
            actor: request.actor.clone(),
            idempotency_key: request.idempotency_key.clone(),
        }
        .validate()?;
    }
    Ok(())
}
