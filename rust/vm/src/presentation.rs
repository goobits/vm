//! Stable machine-readable envelopes for commands that expose structured output.

use serde::Serialize;

use crate::error::{VmError, VmResult};

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    schema_version: u32,
    command: &'static str,
    ok: bool,
    data: Option<T>,
    errors: Vec<ErrorRecord>,
}

#[derive(Serialize)]
pub(crate) struct ErrorRecord {
    code: &'static str,
    message: String,
    next_action: Option<String>,
    target: Option<String>,
}

impl ErrorRecord {
    pub fn from_error(error: &VmError) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
            next_action: error.hint().map(str::to_string),
            target: error.target().map(str::to_string),
        }
    }
}

pub fn success<T: Serialize>(command: &'static str, data: T) -> VmResult<()> {
    write(&Envelope {
        schema_version: 1,
        command,
        ok: true,
        data: Some(data),
        errors: Vec::new(),
    })
}

pub fn failure(command: &'static str, error: &VmError) -> VmResult<()> {
    write(&Envelope::<serde_json::Value> {
        schema_version: 1,
        command,
        ok: false,
        data: None,
        errors: vec![ErrorRecord::from_error(error)],
    })
}

pub fn outcome<T: Serialize>(
    command: &'static str,
    data: T,
    errors: Vec<ErrorRecord>,
) -> VmResult<()> {
    let ok = errors.is_empty();
    write(&Envelope {
        schema_version: 1,
        command,
        ok,
        data: Some(data),
        errors,
    })
}

fn write<T: Serialize>(envelope: &Envelope<T>) -> VmResult<()> {
    let json = serde_json::to_string(envelope)?;
    vm_core::vm_println!("{json}");
    Ok(())
}
