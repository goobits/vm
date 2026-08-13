use serde::Serialize;
use vm_packages::sha256_hex;

use super::{Database, IdempotencyRecord};
use crate::{WorkError, WorkResult};

pub(crate) fn next_id(database: &mut Database) -> u64 {
    database.sequence += 1;
    database.sequence
}

pub(crate) fn validate_key(key: &str) -> WorkResult<()> {
    if key.trim().is_empty() || key.len() > 128 {
        Err(WorkError::Invalid(
            "idempotency key must contain 1 to 128 characters".into(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn ensure_fingerprint(
    existing: &IdempotencyRecord,
    fingerprint: &str,
) -> WorkResult<()> {
    if existing.fingerprint == fingerprint {
        Ok(())
    } else {
        Err(WorkError::Conflict(
            "idempotency key was already used for a different request".into(),
        ))
    }
}

pub(crate) fn operation_fingerprint(
    operation: &str,
    target: Option<&str>,
    value: &impl Serialize,
) -> WorkResult<String> {
    Ok(sha256_hex(&serde_json::to_vec(&(
        operation, target, value,
    ))?))
}
