#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use git2::Repository;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

struct FakeGateway {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl FakeGateway {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_requests = Arc::clone(&requests);
        let worker_stopped = Arc::clone(&stopped);
        let worker = thread::spawn(move || {
            while !worker_stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => handle_request(&mut stream, &worker_requests),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("fake gateway accept failed: {error}"),
                }
            }
        });
        Self {
            port,
            requests,
            stopped,
            worker: Some(worker),
        }
    }

    fn package_registrations(&self) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.as_str() == "POST /work/v1/packages")
            .count()
    }
}

impl Drop for FakeGateway {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn handle_request(stream: &mut TcpStream, requests: &Arc<Mutex<Vec<String>>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let request = read_http_request(stream);
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap();
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let mut request_line = headers.lines().next().unwrap().split_whitespace();
    let method = request_line.next().unwrap();
    let path = request_line.next().unwrap();
    requests.lock().unwrap().push(format!("{method} {path}"));

    let authorized = headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("authorization") && value.trim().starts_with("Bearer ")
        })
    });
    let (status, body) = if method == "POST" && path == "/work/v1/packages" {
        let registered: Value = serde_json::from_slice(&request[header_end..]).unwrap();
        (
            "200 OK",
            json!({
                "name": registered["name"],
                "ecosystem": registered["ecosystem"],
                "repository": registered["repository"],
                "default_branch": registered["default_branch"],
                "workspace_release": registered["workspace_release"],
                "registered_at": "2026-08-11T00:00:00Z"
            })
            .to_string(),
        )
    } else if method == "GET" && path == "/work/v1/packages" && authorized {
        (
            "200 OK",
            json!([{
                "name": "shared-auth",
                "ecosystem": "npm",
                "repository": "https://example.com/shared-auth.git",
                "default_branch": "main",
                "registered_at": "2026-08-13T00:00:00Z"
            }])
            .to_string(),
        )
    } else if method == "GET" && path == "/work/v1/tools" && authorized {
        (
            "200 OK",
            json!([{
                "name": "agent-skills",
                "kind": "collection",
                "repository": "https://example.com/agent-skills.git",
                "default_branch": "main",
                "registered_at": "2026-08-13T00:00:00Z"
            }])
            .to_string(),
        )
    } else if method == "GET" && path == "/work/v1/checkouts" {
        ("200 OK", "[]".to_string())
    } else if method == "GET" && matches!(path, "/work/v1/packages" | "/work/v1/tools") {
        ("401 Unauthorized", "{}".to_string())
    } else if matches!(path, "/health" | "/work/health" | "/v2/") {
        ("200 OK", "{}".to_string())
    } else {
        ("404 Not Found", "{}".to_string())
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
}

fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut expected = None;
    loop {
        let read = stream.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if expected.is_none() {
            if let Some(header) = request.windows(4).position(|window| window == b"\r\n\r\n") {
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
        }
        if expected.is_some_and(|length| request.len() >= length) {
            break;
        }
    }
    request
}

fn fixture_package(source_root: &Path) -> PathBuf {
    let package = source_root.join("shared-auth");
    fs::create_dir_all(&package).unwrap();
    let repository = Repository::init(&package).unwrap();
    repository
        .remote("origin", "git@example.com:shared/shared-auth.git")
        .unwrap();
    repository
        .reference_symbolic(
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
            true,
            "test default branch",
        )
        .unwrap();
    fs::write(package.join("package.json"), r#"{"name":"@shared/auth"}"#).unwrap();
    package
}

fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, snapshot: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries = fs::read_dir(current)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                snapshot.push((relative, b"directory".to_vec()));
                visit(root, &path, snapshot);
            } else if metadata.file_type().is_symlink() {
                snapshot.push((
                    relative,
                    fs::read_link(&path)
                        .unwrap()
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec(),
                ));
            } else {
                snapshot.push((relative, fs::read(&path).unwrap()));
            }
        }
    }

    let mut snapshot = Vec::new();
    visit(root, root, &mut snapshot);
    snapshot
}

fn fixture_broken_package(source_root: &Path) -> PathBuf {
    let package = source_root.join("broken-package");
    fs::create_dir_all(&package).unwrap();
    let repository = Repository::init(&package).unwrap();
    repository
        .remote("origin", "git@example.com:shared/broken-package.git")
        .unwrap();
    package
}

fn fake_docker(directory: &Path) -> (PathBuf, PathBuf) {
    let executable = directory.join("docker");
    let log = directory.join("docker.log");
    fs::write(
        &executable,
        r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "${VM_FAKE_DOCKER_LOG:?}"
if test "${1:-} ${2:-}" = "image inspect"; then
  printf '%s\n' '[{"Config":{"Labels":{}}}]'
fi
"#,
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    (executable, log)
}

fn fake_tart(directory: &Path) -> PathBuf {
    let executable = directory.join("tart");
    let log = directory.join("tart.log");
    fs::write(
        &executable,
        r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "${VM_FAKE_TART_LOG:?}"
exit 97
"#,
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    log
}

fn configure_source_roots(directory: &TempDir, roots: &[&Path]) {
    configure_sources(directory, roots, &[]);
}

fn configure_sources(directory: &TempDir, roots: &[&Path], canonical: &[&Path]) {
    fs::create_dir_all(directory.path().join(".vm")).unwrap();
    let mut config = String::from("packages:\n");
    if !roots.is_empty() {
        config.push_str("  source_roots:\n");
        for root in roots {
            config.push_str(&format!("    - {}\n", root.display()));
        }
    }
    if !canonical.is_empty() {
        config.push_str("  canonical_sources:\n");
        for root in canonical {
            config.push_str(&format!("    - {}\n", root.display()));
        }
    }
    fs::write(directory.path().join(".vm/config.yaml"), config).unwrap();
}

fn project_config(directory: &TempDir) -> PathBuf {
    let config = directory.path().join("vm.yaml");
    fs::write(
        &config,
        "version: '2.0'\nprovider: docker\nproject:\n  name: package-test\n",
    )
    .unwrap();
    config
}

fn packages_up(
    directory: &TempDir,
    config: &Path,
    fake_bin: &Path,
    docker_log: &Path,
    port: u16,
) -> Output {
    let mut path = std::ffi::OsString::from(fake_bin);
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    Command::new(cargo_bin!("vm"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "packages",
            "up",
            "--runtime",
            "docker",
            "--port",
            &port.to_string(),
            "--registry-image",
            "registry.example/package-server:test",
            "--job-image",
            "registry.example/package-jobs:test",
        ])
        .current_dir(directory.path())
        .env("HOME", directory.path())
        .env("PATH", path)
        .env("VM_FAKE_DOCKER_LOG", docker_log)
        .env("VM_FAKE_TART_LOG", fake_bin.join("tart.log"))
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "host")
        .env("CI", "1")
        .env_remove("VM_MANAGED_GUEST")
        .env_remove("VM_IMAGE_IDENTITY")
        .output()
        .unwrap()
}

fn packages_status(directory: &TempDir, context: &str) -> Output {
    Command::new(cargo_bin("vm"))
        .args(["packages", "status"])
        .env("HOME", directory.path())
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", context)
        .env_remove("VM_PACKAGES_CONSUMER")
        .env_remove("VM_PACKAGES_WORK_GATEWAY")
        .env_remove("VM_PACKAGES_AGENT_TOKEN")
        .output()
        .unwrap()
}

fn packages_register(directory: &TempDir, repository: &Path, gateway: &FakeGateway) -> Output {
    Command::new(cargo_bin!("vm"))
        .args(["packages", "register", repository.to_str().unwrap()])
        .current_dir(directory.path())
        .env("HOME", directory.path())
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "host")
        .env(
            "VM_PACKAGES_WORK_GATEWAY",
            format!("http://127.0.0.1:{}", gateway.port),
        )
        .env_remove("VM_MANAGED_GUEST")
        .output()
        .unwrap()
}

fn packages_doctor(
    directory: &TempDir,
    config: &Path,
    fake_bin: &Path,
    docker_log: &Path,
) -> Output {
    let mut path = std::ffi::OsString::from(fake_bin);
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    Command::new(cargo_bin!("vm"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "packages",
            "doctor",
            "--runtime",
            "docker",
            "--fix",
        ])
        .current_dir(directory.path())
        .env("HOME", directory.path())
        .env("PATH", path)
        .env("VM_FAKE_DOCKER_LOG", docker_log)
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "host")
        .env("CI", "1")
        .env_remove("VM_MANAGED_GUEST")
        .output()
        .unwrap()
}

#[test]
fn isolated_checkout_is_guest_only_with_one_exact_repair_command() {
    let directory = TempDir::new().unwrap();
    let output = Command::new(cargo_bin!("vm"))
        .args(["packages", "checkout", "typemill"])
        .env("HOME", directory.path())
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "host")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("Run inside a managed VM: vm packages checkout typemill"));
}

#[test]
fn fresh_setup_and_existing_state_reconciliation_are_idempotent() {
    let directory = TempDir::new().unwrap();
    let gateway = FakeGateway::start();
    let fake_bin = directory.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let (_, docker_log) = fake_docker(&fake_bin);
    let tart_log = fake_tart(&fake_bin);
    let source_root = directory.path().join("package-sources");
    fixture_package(&source_root);

    configure_source_roots(&directory, &[&source_root]);
    let config = project_config(&directory);

    let first = packages_up(&directory, &config, &fake_bin, &docker_log, gateway.port);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("Package infrastructure is ready"));
    assert!(first_stdout.contains("Reconciling 1 managed package shelf"));
    assert!(first_stdout.contains("Registered @shared/auth (npm)"));

    let appliance = directory.path().join(".vm/infrastructure/packages");
    let read_token = fs::read(appliance.join("read-token")).unwrap();
    let publish_token = fs::read(appliance.join("publish-token")).unwrap();
    assert!(appliance.join("compose.yaml").is_file());
    assert!(appliance.join("state.json").is_file());

    let second = packages_up(&directory, &config, &fake_bin, &docker_log, gateway.port);
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(fs::read(appliance.join("read-token")).unwrap(), read_token);
    assert_eq!(
        fs::read(appliance.join("publish-token")).unwrap(),
        publish_token
    );
    assert_eq!(gateway.package_registrations(), 2);

    let commands = fs::read_to_string(docker_log).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.contains("compose") && line.contains(" up "))
            .count(),
        2
    );
    assert!(!commands.lines().any(|line| line.starts_with("build ")));
    assert!(!commands.lines().any(|line| line.starts_with("pull ")));
    assert!(!commands
        .lines()
        .any(|line| line.split_whitespace().any(|argument| argument == "down")));
    assert!(!commands.lines().any(|line| line.contains("volume rm")));
    assert!(!tart_log.exists(), "Docker reconciliation invoked Tart");
}

#[test]
fn configured_empty_shelf_is_a_successful_noop() {
    let directory = TempDir::new().unwrap();
    let gateway = FakeGateway::start();
    let fake_bin = directory.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let (_, docker_log) = fake_docker(&fake_bin);
    let source_root = directory.path().join("empty-package-sources");
    fs::create_dir_all(&source_root).unwrap();
    configure_source_roots(&directory, &[&source_root]);
    let config = project_config(&directory);

    let output = packages_up(&directory, &config, &fake_bin, &docker_log, gateway.port);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("Package source scan complete; no package or tool repositories found"));
    assert_eq!(gateway.package_registrations(), 0);
}

#[test]
fn exact_registration_is_persisted_and_reconciled_without_source_mutation() {
    let directory = TempDir::new().unwrap();
    let gateway = FakeGateway::start();
    let fake_bin = directory.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let (_, docker_log) = fake_docker(&fake_bin);
    fs::create_dir_all(directory.path().join(".vm")).unwrap();
    fs::write(directory.path().join(".vm/config.yaml"), "{}\n").unwrap();
    let project = fixture_package(&directory.path().join("projects"));
    let config = project_config(&directory);
    let before = tree_snapshot(&project);

    let up = packages_up(&directory, &config, &fake_bin, &docker_log, gateway.port);
    assert!(
        up.status.success(),
        "{}",
        String::from_utf8_lossy(&up.stderr)
    );
    let registered = packages_register(&directory, &project, &gateway);
    assert!(
        registered.status.success(),
        "{}",
        String::from_utf8_lossy(&registered.stderr)
    );
    assert_eq!(tree_snapshot(&project), before);
    let global = fs::read_to_string(directory.path().join(".vm/config.yaml")).unwrap();
    assert!(global.contains("canonical_sources:"));
    assert!(global.contains(project.canonicalize().unwrap().to_str().unwrap()));
    assert!(!global.contains("source_roots:"));

    let reconciled = packages_up(&directory, &config, &fake_bin, &docker_log, gateway.port);
    assert!(
        reconciled.status.success(),
        "{}",
        String::from_utf8_lossy(&reconciled.stderr)
    );
    assert_eq!(tree_snapshot(&project), before);
    assert_eq!(gateway.package_registrations(), 2);

    fs::write(
        directory
            .path()
            .join(".vm/infrastructure/packages/git-token"),
        "test-token",
    )
    .unwrap();
    let doctor = packages_doctor(&directory, &config, &fake_bin, &docker_log);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert_eq!(tree_snapshot(&project), before);
}

#[test]
fn invalid_canonical_source_is_degraded_in_place_without_quarantine() {
    let directory = TempDir::new().unwrap();
    let gateway = FakeGateway::start();
    let fake_bin = directory.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let (_, docker_log) = fake_docker(&fake_bin);
    let source = fixture_broken_package(&directory.path().join("projects"));
    configure_sources(&directory, &[], &[&source]);
    let config = project_config(&directory);
    let before = tree_snapshot(&source);

    let output = packages_up(&directory, &config, &fake_bin, &docker_log, gateway.port);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Package infrastructure: degraded"));
    assert_eq!(tree_snapshot(&source), before);
    assert!(source.exists());
    assert!(!directory.path().join("projects/.vm-quarantine").exists());
    assert_eq!(gateway.package_registrations(), 0);

    fs::write(
        directory
            .path()
            .join(".vm/infrastructure/packages/git-token"),
        "test-token",
    )
    .unwrap();
    let doctor = packages_doctor(&directory, &config, &fake_bin, &docker_log);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert!(String::from_utf8_lossy(&doctor.stderr).contains("repair"));
    assert_eq!(tree_snapshot(&source), before);
}

#[test]
fn unhealthy_repository_is_quarantined_without_failing_up() {
    let directory = TempDir::new().unwrap();
    let gateway = FakeGateway::start();
    let fake_bin = directory.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let (_, docker_log) = fake_docker(&fake_bin);
    let source_root = directory.path().join("package-sources");
    fixture_package(&source_root);
    let broken = fixture_broken_package(&source_root);
    configure_source_roots(&directory, &[&source_root]);
    let config = project_config(&directory);

    let output = packages_up(&directory, &config, &fake_bin, &docker_log, gateway.port);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Package infrastructure: degraded"));
    assert!(!broken.exists());
    assert!(source_root
        .join(".vm-quarantine/broken-package/.git")
        .exists());
    assert_eq!(gateway.package_registrations(), 1);
}

#[test]
fn guest_package_status_verifies_access_without_mutating_state() {
    let directory = TempDir::new().unwrap();
    let gateway = FakeGateway::start();
    let output = Command::new(cargo_bin!("vm"))
        .args(["packages", "status"])
        .current_dir(directory.path())
        .env("HOME", directory.path())
        .env("VM_TOOL_DIR", directory.path().join(".vm"))
        .env("VM_MANAGED_GUEST", "1")
        .env("VM_PACKAGES_CONSUMER", "package-test")
        .env(
            "VM_PACKAGES_WORK_GATEWAY",
            format!("http://127.0.0.1:{}", gateway.port),
        )
        .env("VM_PACKAGES_AGENT_TOKEN", "agent-token")
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "guest")
        .env("CI", "1")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stdout, "Package infrastructure: healthy\n");

    let mut requests = gateway.requests.lock().unwrap().clone();
    requests.sort();
    assert_eq!(requests, ["GET /work/v1/packages", "GET /work/v1/tools"]);
}

#[test]
fn guest_package_status_reduces_missing_access_to_one_answer() {
    let directory = TempDir::new().unwrap();
    let output = packages_status(&directory, "guest");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Package infrastructure: action required\n"
    );
}

#[test]
fn host_package_status_reduces_missing_setup_to_one_answer() {
    let directory = TempDir::new().unwrap();
    let output = packages_status(&directory, "host");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Package infrastructure: action required\n"
    );
}

#[test]
fn invalid_configured_root_fails_before_appliance_mutation() {
    let directory = TempDir::new().unwrap();
    let fake_bin = directory.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let (_, docker_log) = fake_docker(&fake_bin);
    let missing_root = directory.path().join("missing-package-sources");
    configure_source_roots(&directory, &[&missing_root]);
    let config = project_config(&directory);

    let output = packages_up(&directory, &config, &fake_bin, &docker_log, 3080);

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("resolve package registration path"));
    assert!(!docker_log.exists());
    assert!(!directory
        .path()
        .join(".vm/infrastructure/packages/state.json")
        .exists());
}
