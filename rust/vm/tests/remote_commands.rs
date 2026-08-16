#![cfg(unix)]

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use tempfile::TempDir;

fn serve(status: &str, body: Value) -> (String, Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let status = status.to_string();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let _ = sender.send(request);
        let body = body.to_string();
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    (endpoint, receiver)
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected = None;
    loop {
        let read = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..read]);
        if let Some(header) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            let header_end = header + 4;
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or_default();
            expected = Some(header_end + content_length);
        }
        if read == 0 || expected.is_some_and(|length| request.len() >= length) {
            return request;
        }
    }
}

fn registry(directory: &TempDir, endpoint: &str) -> std::path::PathBuf {
    let path = directory.path().join("remote-commands.json");
    fs::write(
        &path,
        json!({
            "schema": 1,
            "commands": {
                "issue": {
                    "endpoint": endpoint,
                    "capability": "repository-scoped-capability",
                    "repair_command": "vm start demo-dev"
                }
            }
        })
        .to_string(),
    )
    .unwrap();
    path
}

fn vm(directory: &TempDir, registry: &std::path::Path, context: &str) -> Command {
    let mut command = Command::new(cargo_bin!("vm"));
    command
        .current_dir(directory.path())
        .env("HOME", directory.path())
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", context)
        .env("VM_REMOTE_COMMANDS_FILE", registry)
        .env_remove("VM_MANAGED_GUEST")
        .env_remove("VM_IMAGE_IDENTITY");
    command
}

#[test]
fn registered_namespace_forwards_only_arguments_and_scoped_capability() {
    let directory = TempDir::new().unwrap();
    let (endpoint, requests) = serve(
        "200 OK",
        json!({"schema": 1, "exit_code": 0, "stdout": "#123 ready\n"}),
    );
    let registry = registry(&directory, &endpoint);

    vm(&directory, &registry, "guest")
        .args(["issue", "123"])
        .assert()
        .success()
        .stdout("#123 ready\n")
        .stderr("");

    let request = requests.recv().unwrap();
    let header_end = request
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap()
        + 4;
    let headers = String::from_utf8_lossy(&request[..header_end]);
    assert!(headers.starts_with("POST /v1/commands/issue HTTP/1.1\r\n"));
    assert!(headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("authorization")
                && value.trim() == "Bearer repository-scoped-capability"
        })
    }));
    let body: Value = serde_json::from_slice(&request[header_end..]).unwrap();
    assert_eq!(body["schema"], 1);
    assert_eq!(body["arguments"], json!(["123"]));
    assert!(body["idempotency_key"]
        .as_str()
        .is_some_and(|key| !key.is_empty()));
    assert!(body.get("repository").is_none());
    assert!(body.get("endpoint").is_none());
}

#[test]
fn remote_namespaces_are_not_available_on_the_controller_host() {
    let directory = TempDir::new().unwrap();
    let registry = registry(&directory, "http://127.0.0.1:9");

    vm(&directory, &registry, "host")
        .args(["issue", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown command 'issue'"));
}

#[test]
fn service_failures_name_one_registered_repair_command() {
    let directory = TempDir::new().unwrap();
    let (endpoint, _) = serve("503 Service Unavailable", json!({}));
    let registry = registry(&directory, &endpoint);

    vm(&directory, &registry, "guest")
        .args(["issue", "open", "broken"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("service returned HTTP 503"))
        .stderr(predicate::str::contains("Hint: Run: vm start demo-dev"));
}
