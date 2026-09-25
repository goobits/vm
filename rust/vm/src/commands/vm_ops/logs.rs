//! Human and JSON Lines output for one environment's logs.

use std::io::{self, Write};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use tracing::debug;
use vm_config::config::VmConfig;
use vm_core::error::{Result as ProviderResult, VmError as ProviderError};
use vm_provider::{LogRecord, Provider};

use crate::error::{VmError, VmResult};

pub fn handle_logs(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    follow: bool,
    tail: usize,
    service: Option<&str>,
    json_lines: bool,
) -> VmResult<()> {
    debug!(
        provider = provider.name(),
        follow, tail, service, json_lines, "Viewing VM logs"
    );
    if !json_lines {
        return provider
            .logs_extended(container, follow, tail, service, &config)
            .map_err(VmError::from);
    }
    let environment = container.ok_or_else(|| {
        VmError::validation(
            "Structured logs require an exact environment",
            None::<String>,
        )
    })?;
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    run_json_lines_to(&mut writer, environment, service, |sink| {
        provider.logs_records(container, follow, tail, service, &config, sink)
    })
    .map_err(VmError::from)
}

pub fn emit_prelaunch_failure(environment: Option<&str>, error: &VmError) -> VmResult<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    write_event(
        &mut writer,
        &json!({
            "schema_version": 1,
            "command": "logs",
            "type": "result",
            "environment": environment,
            "ok": false,
            "records": 0,
            "error": error.to_string(),
        }),
    )
    .map_err(VmError::from)
}

fn write_event(writer: &mut impl Write, event: &Value) -> ProviderResult<()> {
    serde_json::to_writer(&mut *writer, event)
        .map_err(|error| ProviderError::Provider(format!("Failed to encode log event: {error}")))?;
    writer
        .write_all(b"\n")
        .map_err(|error| ProviderError::Provider(format!("Failed to write log event: {error}")))?;
    writer
        .flush()
        .map_err(|error| ProviderError::Provider(format!("Failed to flush log event: {error}")))
}

fn run_json_lines_to(
    writer: &mut impl Write,
    environment: &str,
    service: Option<&str>,
    producer: impl FnOnce(&mut dyn FnMut(LogRecord) -> ProviderResult<()>) -> ProviderResult<()>,
) -> ProviderResult<()> {
    let mut records = 0usize;
    let outcome = producer(&mut |record| {
        write_event(
            writer,
            &json!({
                "schema_version": 1,
                "command": "logs",
                "type": "record",
                "environment": environment,
                "service": service,
                "stream": record.stream,
                "timestamp": record.timestamp,
                "encoding": "base64",
                "data_base64": STANDARD.encode(&record.bytes),
            }),
        )?;
        records += 1;
        Ok(())
    });
    write_event(
        writer,
        &json!({
            "schema_version": 1,
            "command": "logs",
            "type": "result",
            "environment": environment,
            "ok": outcome.is_ok(),
            "records": records,
            "error": outcome.as_ref().err().map(ToString::to_string),
        }),
    )?;
    outcome
}

#[cfg(test)]
mod tests {
    use super::run_json_lines_to;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use vm_core::error::VmError;
    use vm_provider::LogRecord;

    #[test]
    fn application_bytes_are_encoded_without_json_interpretation() {
        let mut output = Vec::new();
        run_json_lines_to(&mut output, "demo-dev", Some("api"), |sink| {
            sink(LogRecord {
                stream: "stdout",
                timestamp: Some("2026-09-25T12:34:56Z".into()),
                bytes: b"{\"broken\":\xff}\n".to_vec(),
            })
        })
        .unwrap();
        let events = output
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["type"], "record");
        assert_eq!(events[0]["environment"], "demo-dev");
        assert_eq!(events[0]["service"], "api");
        assert_eq!(events[0]["timestamp"], "2026-09-25T12:34:56Z");
        assert_eq!(
            STANDARD
                .decode(events[0]["data_base64"].as_str().unwrap())
                .unwrap(),
            b"{\"broken\":\xff}\n"
        );
        assert_eq!(events[1]["type"], "result");
        assert_eq!(events[1]["ok"].as_bool(), Some(true));
        assert_eq!(events[1]["records"], 1);
    }

    #[test]
    fn provider_failure_still_emits_final_event() {
        let mut output = Vec::new();
        let error = run_json_lines_to(&mut output, "demo-dev", None, |_sink| {
            Err(VmError::Provider("runtime unavailable".into()))
        })
        .unwrap_err();
        assert!(error.to_string().contains("runtime unavailable"));
        let event: serde_json::Value =
            serde_json::from_slice(output.strip_suffix(b"\n").unwrap()).unwrap();
        assert_eq!(event["type"], "result");
        assert_eq!(event["ok"].as_bool(), Some(false));
        assert_eq!(event["records"], 0);
        assert_eq!(event["error"], "Provider error: runtime unavailable");
    }
}
