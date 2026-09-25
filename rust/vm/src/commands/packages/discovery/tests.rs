use std::fs;
use std::path::{Path, PathBuf};

use git2::Repository;
use vm_packages::{PackageDefinition, PackageEcosystem};

use super::{
    discover, discover_canonical, discover_configured, discover_local, normalize_repository_url,
    SourceRequest, TOOL_MANIFEST,
};

fn package(root: &Path, directory: &str, manifest: &str, content: &str) -> PathBuf {
    let path = root.join(directory);
    fs::create_dir_all(&path).unwrap();
    let repository = Repository::init(&path).unwrap();
    repository
        .remote("origin", &format!("git@example.com:shared/{directory}.git"))
        .unwrap();
    repository
        .reference_symbolic(
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
            true,
            "test default branch",
        )
        .unwrap();
    fs::write(path.join(manifest), content).unwrap();
    path
}

#[test]
fn recursively_discovers_each_supported_ecosystem() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "auth-js",
        "package.json",
        r#"{"name":"@shared/auth"}"#,
    );
    package(
        directory.path(),
        "auth-rs",
        "Cargo.toml",
        "[package]\nname = \"shared-auth\"\nversion = \"1.0.0\"\n",
    );
    package(
        directory.path(),
        "auth-py",
        "pyproject.toml",
        "[project]\nname = \"shared_auth\"\nversion = \"1.0.0\"\n",
    );

    let discovery = discover(
        &[directory.path().to_string_lossy().into_owned()],
        true,
        None,
        None,
    )
    .unwrap();
    let packages = discovery.packages;

    assert_eq!(packages.len(), 3);
    assert_eq!(packages[0].name, "@shared/auth");
    assert_eq!(packages[0].ecosystem, PackageEcosystem::Npm);
    assert_eq!(
        packages[0].repository,
        "ssh://git@example.com/shared/auth-js.git"
    );
    assert_eq!(packages[1].name, "shared_auth");
    assert_eq!(packages[1].ecosystem, PackageEcosystem::Python);
    assert_eq!(packages[2].name, "shared-auth");
    assert_eq!(packages[2].ecosystem, PackageEcosystem::Cargo);
    assert!(packages
        .iter()
        .all(|package| package.default_branch == "main"));
}

#[test]
fn ecosystem_override_resolves_an_ambiguous_repository() {
    let directory = tempfile::tempdir().unwrap();
    let path = package(
        directory.path(),
        "mixed",
        "package.json",
        r#"{"name":"mixed-js"}"#,
    );
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"mixed-rs\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    let target = path.to_string_lossy().into_owned();

    assert!(discover(std::slice::from_ref(&target), false, None, None).is_err());
    let discovery = discover(
        &[target],
        false,
        Some(PackageEcosystem::Cargo),
        Some("stable"),
    )
    .unwrap();
    let packages = discovery.packages;

    assert_eq!(packages[0].name, "mixed-rs");
    assert_eq!(packages[0].default_branch, "stable");
    assert_eq!(
        normalize_repository_url("github.com:shared/mixed.git").unwrap(),
        "ssh://github.com/shared/mixed.git"
    );
}

#[test]
fn recursive_discovery_separates_tool_repositories() {
    let directory = tempfile::tempdir().unwrap();
    let tool = package(
        directory.path(),
        "agent-skills",
        "package.json",
        r#"{"name":"@shared/agent-skills"}"#,
    );
    fs::write(tool.join(TOOL_MANIFEST), "schema: 1\nkind: collection\n").unwrap();

    let discovery = discover(
        &[directory.path().to_string_lossy().into_owned()],
        true,
        None,
        None,
    )
    .unwrap();

    assert!(discovery.packages.is_empty());
    assert_eq!(discovery.tools.len(), 1);
    assert_eq!(discovery.tools[0].name, "agent-skills");
    assert_eq!(discovery.tools[0].kind, vm_packages::ToolKind::Collection);
}

#[test]
fn only_configured_source_shelves_may_be_empty() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().to_string_lossy().into_owned();

    assert!(discover(std::slice::from_ref(&target), true, None, None).is_err());
    let configured = discover_configured(&[target]).unwrap();

    assert!(configured.packages.is_empty());
    assert!(configured.tools.is_empty());
    assert!(configured.failures.is_empty());
}

#[test]
fn configured_discovery_isolates_invalid_repositories() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "auth",
        "package.json",
        r#"{"name":"@shared/auth","version":"1.0.0"}"#,
    );
    let broken = directory.path().join("broken");
    fs::create_dir(&broken).unwrap();
    let repository = Repository::init(&broken).unwrap();
    repository
        .remote("origin", "git@example.com:shared/broken.git")
        .unwrap();

    let configured =
        discover_configured(&[directory.path().to_string_lossy().into_owned()]).unwrap();

    assert_eq!(configured.packages.len(), 1);
    assert_eq!(configured.failures.len(), 1);
    assert!(configured.failures[0].message.contains("broken"));
}

#[test]
fn configured_discovery_collapses_equivalent_source_aliases() {
    let directory = tempfile::tempdir().unwrap();
    let first = package(
        directory.path(),
        "first",
        "package.json",
        r#"{"name":"@shared/auth","version":"1.0.0"}"#,
    );
    let second = package(
        directory.path(),
        "second",
        "package.json",
        r#"{"name":"@shared/auth","version":"1.0.0"}"#,
    );
    Repository::open(&second)
        .unwrap()
        .remote_set_url("origin", "ssh://git@example.com/shared/first.git")
        .unwrap();

    let configured =
        discover_configured(&[directory.path().to_string_lossy().into_owned()]).unwrap();

    assert_eq!(configured.packages.len(), 1);
    assert_eq!(
        configured.packages[0].repository,
        "ssh://git@example.com/shared/first.git"
    );
    assert!(configured.conflicts.is_empty());
    assert!(first.is_dir());
}

#[test]
fn configured_discovery_reports_different_origins_for_one_identity() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "current",
        "package.json",
        r#"{"name":"@shared/auth","version":"1.0.0"}"#,
    );
    package(
        directory.path(),
        "archive",
        "package.json",
        r#"{"name":"@shared/auth","version":"0.9.0"}"#,
    );

    let mut configured =
        discover_configured(&[directory.path().to_string_lossy().into_owned()]).unwrap();

    assert!(configured.packages.is_empty());
    assert_eq!(configured.conflicts.len(), 1);
    assert_eq!(configured.conflicts[0].identity, "package:npm:@shared/auth");
    assert_eq!(configured.conflicts[0].sources.len(), 2);

    configured.prefer_registered(
        &[PackageDefinition {
            name: "@shared/auth".into(),
            ecosystem: PackageEcosystem::Npm,
            repository: "ssh://git@example.com/shared/current.git".into(),
            default_branch: "main".into(),
            workspace_release: true,
            registered_at: chrono::Utc::now(),
        }],
        &[],
    );
    assert_eq!(configured.packages.len(), 1);
    assert_eq!(
        configured.packages[0].repository,
        "ssh://git@example.com/shared/current.git"
    );
    assert!(configured.conflicts.is_empty());
}

#[test]
fn configured_discovery_keeps_same_name_in_different_ecosystems() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "npm",
        "package.json",
        r#"{"name":"shared-name","version":"1.0.0"}"#,
    );
    package(
        directory.path(),
        "cargo",
        "Cargo.toml",
        "[package]\nname = \"shared-name\"\nversion = \"1.0.0\"\n",
    );

    let configured =
        discover_configured(&[directory.path().to_string_lossy().into_owned()]).unwrap();

    assert_eq!(configured.packages.len(), 2);
    assert!(configured.conflicts.is_empty());
}

#[test]
fn package_ignore_marker_skips_an_archived_subtree() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "current",
        "package.json",
        r#"{"name":"@shared/auth","version":"1.0.0"}"#,
    );
    let archive = directory.path().join("archive");
    fs::create_dir(&archive).unwrap();
    fs::write(archive.join(".vm-packages-ignore"), "").unwrap();
    package(
        &archive,
        "old",
        "package.json",
        r#"{"name":"@shared/auth","version":"0.9.0"}"#,
    );

    let configured =
        discover_configured(&[directory.path().to_string_lossy().into_owned()]).unwrap();

    assert_eq!(configured.packages.len(), 1);
    assert!(configured.conflicts.is_empty());
}

#[test]
fn exact_discovery_retains_physical_roots_and_release_attestation() {
    let directory = tempfile::tempdir().unwrap();
    let source = package(
        directory.path(),
        "typemill",
        "package.json",
        r#"{"name":"typemill","version":"1.0.0"}"#,
    );
    let discovered =
        discover_local(&[source.to_string_lossy().into_owned()], false, None, None).unwrap();

    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].root, source.canonicalize().unwrap());
    let SourceRequest::Package(request) = &discovered[0].request else {
        panic!("expected package registration");
    };
    assert!(request.workspace_release);
}

#[test]
fn exact_tool_registration_requires_the_git_root() {
    let directory = tempfile::tempdir().unwrap();
    let source = package(
        directory.path(),
        "codeatlas",
        "package.json",
        r#"{"name":"codeatlas","version":"1.0.0"}"#,
    );
    let nested = source.join("packages/codeatlas");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join(TOOL_MANIFEST), "schema: 1\nkind: collection\n").unwrap();

    let error = discover_local(&[nested.to_string_lossy().into_owned()], false, None, None)
        .err()
        .unwrap();

    assert!(error.to_string().contains("not a Git repository root"));
    assert_eq!(
        error.hint().unwrap(),
        format!("Use {} instead", source.canonicalize().unwrap().display())
    );
}

#[test]
fn canonical_discovery_keeps_healthy_sources_when_one_is_missing() {
    let directory = tempfile::tempdir().unwrap();
    let source = package(
        directory.path(),
        "typemill",
        "package.json",
        r#"{"name":"typemill","version":"1.0.0"}"#,
    );
    let missing = directory.path().join("missing");

    let discovery = discover_canonical(
        &[
            source.to_string_lossy().into_owned(),
            missing.to_string_lossy().into_owned(),
        ],
        &[],
        &[],
    );

    assert_eq!(discovery.packages.len(), 1);
    assert!(discovery.packages[0].workspace_release);
    assert_eq!(discovery.failures.len(), 1);
    assert!(!missing.exists());
}

#[test]
fn canonical_discovery_reuses_registered_branch_and_ecosystem() {
    let directory = tempfile::tempdir().unwrap();
    let source = package(
        directory.path(),
        "multi-package",
        "Cargo.toml",
        "[package]\nname = \"multi-package\"\nversion = \"1.0.0\"\n",
    );
    fs::write(
        source.join("package.json"),
        r#"{"name":"multi-package-js","version":"1.0.0"}"#,
    )
    .unwrap();
    let registered = PackageDefinition {
        name: "multi-package".into(),
        ecosystem: PackageEcosystem::Cargo,
        repository: "https://example.com/shared/multi-package.git".into(),
        default_branch: "release".into(),
        workspace_release: true,
        registered_at: chrono::Utc::now(),
    };
    let repository = git2::Repository::open(&source).unwrap();
    repository
        .remote_set_url("origin", &registered.repository)
        .unwrap();

    let discovery =
        discover_canonical(&[source.to_string_lossy().into_owned()], &[registered], &[]);

    assert!(discovery.failures.is_empty());
    assert_eq!(discovery.packages.len(), 1);
    assert_eq!(discovery.packages[0].ecosystem, PackageEcosystem::Cargo);
    assert_eq!(discovery.packages[0].default_branch, "release");
}

#[test]
fn invalid_tool_manifest_fails_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let tool = package(
        directory.path(),
        "broken-tool",
        "package.json",
        r#"{"name":"broken-tool"}"#,
    );
    fs::write(tool.join(TOOL_MANIFEST), "kind: plugin\n").unwrap();

    let error = discover(
        &[directory.path().to_string_lossy().into_owned()],
        true,
        None,
        None,
    )
    .err()
    .unwrap()
    .to_string();

    assert!(error.contains("Invalid"));
    assert!(error.contains(TOOL_MANIFEST));
}
