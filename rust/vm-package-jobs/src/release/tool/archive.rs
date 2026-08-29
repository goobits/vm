use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use flate2::{Compression, GzBuilder};
use semver::Version;
use serde::Deserialize;
use vm_packages::ToolBuild;

use crate::runtime::run_command;

use super::build::{native_target, run_isolated};
use crate::release::{git, git_text};

pub(super) struct BuiltCollection {
    pub(super) archive: PathBuf,
    pub(super) source_commit: String,
    pub(super) version: String,
}

pub(super) struct CollectionIdentity {
    pub(super) source_commit: String,
    pub(super) version: String,
}

pub(super) fn confined_build_archive(source: &Path, relative: &str) -> Result<PathBuf> {
    let source = std::fs::canonicalize(source)?;
    let candidate = source.join(relative);
    let metadata = std::fs::symlink_metadata(&candidate)
        .with_context(|| format!("binary build did not create {relative}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || metadata.len() == 0 {
        bail!("binary build archive must be a non-empty regular file: {relative}");
    }
    let archive = std::fs::canonicalize(&candidate)?;
    if !archive.starts_with(&source) {
        bail!("binary build archive escaped the isolated source directory");
    }
    Ok(archive)
}

pub(super) fn verify_binary_archive(archive: &Path, build: &ToolBuild) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
    let mut archive = tar::Archive::new(decoder);
    let expected = build
        .links
        .values()
        .map(PathBuf::from)
        .collect::<std::collections::BTreeSet<_>>();
    let executable = build
        .links
        .iter()
        .filter(|(destination, _)| destination.starts_with(".local/bin/"))
        .map(|(_, source)| PathBuf::from(source))
        .collect::<std::collections::BTreeSet<_>>();
    let mut found = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        validate_archive_path(&path)?;
        if !paths.insert(path.clone()) {
            bail!("binary archive contains duplicate path {}", path.display());
        }
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            bail!(
                "binary archive contains a link or special file: {}",
                path.display()
            );
        }
        if expected.contains(&path) {
            if !entry_type.is_file() || entry.size() == 0 {
                bail!(
                    "linked binary artifact must be a non-empty regular file: {}",
                    path.display()
                );
            }
            if executable.contains(&path) {
                let mode = entry.header().mode()?;
                if mode & 0o111 == 0 {
                    bail!(
                        "linked binary artifact is not executable: {}",
                        path.display()
                    );
                }
                let mut prefix = Vec::new();
                entry.by_ref().take(64).read_to_end(&mut prefix)?;
                validate_executable(&prefix, &build.target, &path)?;
            }
            found.insert(path);
        }
    }
    if found != expected {
        let missing = expected.difference(&found).next().expect("sets differ");
        bail!(
            "binary archive is missing linked artifact {}",
            missing.display()
        );
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        bail!("binary archive contains an unsafe path: {}", path.display());
    }
    Ok(())
}

fn validate_executable(prefix: &[u8], target: &str, path: &Path) -> Result<()> {
    if prefix.starts_with(b"#!") {
        return Ok(());
    }
    if prefix.len() < 20 || !prefix.starts_with(b"\x7fELF") {
        bail!(
            "linked executable is neither an ELF binary nor a script: {}",
            path.display()
        );
    }
    let machine = match prefix[5] {
        1 => u16::from_le_bytes([prefix[18], prefix[19]]),
        2 => u16::from_be_bytes([prefix[18], prefix[19]]),
        _ => bail!(
            "linked ELF executable has invalid byte order: {}",
            path.display()
        ),
    };
    let expected = match target {
        "linux-amd64" => 62,
        "linux-arm64" => 183,
        _ => unreachable!("build target was validated"),
    };
    if machine != expected {
        bail!(
            "linked ELF executable architecture does not match {target}: {}",
            path.display()
        );
    }
    Ok(())
}

pub(super) fn is_portable_script(path: &Path) -> Result<bool> {
    let mut prefix = [0_u8; 2];
    let prefix_length = File::open(path)?.read(&mut prefix)?;
    Ok(prefix_length == prefix.len() && prefix == *b"#!")
}

pub(super) fn verify_binary_command(
    archive: &Path,
    release_root: &Path,
    build: &ToolBuild,
) -> Result<()> {
    let Some(command) = &build.verify else {
        return Ok(());
    };
    let root = release_root.join(format!("verify-{}", build.target));
    std::fs::create_dir(&root)?;
    tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?)).unpack(&root)?;
    let mut command = command.clone();
    let program = std::fs::canonicalize(root.join(&command[0]))?;
    if !program.starts_with(&root) || !std::fs::symlink_metadata(&program)?.is_file() {
        bail!("binary verification executable escaped the extracted archive");
    }
    if !is_portable_script(&program)? && native_target() != Some(build.target.as_str()) {
        bail!(
            "verification command for {} cannot run on this release worker",
            build.target
        );
    }
    command[0] = program.to_string_lossy().into_owned();
    run_isolated(
        &command,
        &root,
        release_root,
        &format!("verify binary tool target {}", build.target),
    )
}

pub(super) fn build_collection(source: &Path, archive: &Path) -> Result<BuiltCollection> {
    let identity = collection_identity(source)?;
    create_archive(source, archive)?;
    Ok(BuiltCollection {
        archive: archive.to_path_buf(),
        source_commit: identity.source_commit,
        version: identity.version,
    })
}

pub(super) fn collection_identity(source: &Path) -> Result<CollectionIdentity> {
    #[derive(Deserialize)]
    struct CollectionManifest {
        version: String,
    }

    let manifest: CollectionManifest = serde_json::from_str(&git_text(
        source,
        &["show", "HEAD:package.json"],
        "read collection release manifest",
    )?)
    .context("collection package.json is invalid")?;
    let version = Version::parse(&manifest.version)
        .context("collection package.json version must be semantic")?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        bail!("collection release version must be stable without build metadata");
    }
    let source_commit = git_text(
        source,
        &["rev-parse", "--verify", "HEAD^{commit}"],
        "resolve collection source commit",
    )?;
    Ok(CollectionIdentity {
        source_commit,
        version: version.to_string(),
    })
}

fn create_archive(source: &Path, destination: &Path) -> Result<()> {
    let tar_path = destination.with_extension("tar");
    run_command(
        git()
            .arg("-C")
            .arg(source)
            .args(["archive", "--format=tar", "--prefix=skills/", "--output"])
            .arg(&tar_path)
            .arg("HEAD"),
        "archive collection source",
    )?;
    let mut tar = File::open(&tar_path)
        .with_context(|| format!("read collection archive {}", tar_path.display()))?;
    let output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .with_context(|| format!("create collection archive {}", destination.display()))?;
    let mut gzip = GzBuilder::new().mtime(0).write(output, Compression::best());
    io::copy(&mut tar, &mut gzip)?;
    gzip.finish()?.sync_all()?;
    std::fs::remove_file(&tar_path)?;
    Ok(())
}

pub(super) fn collection_links() -> BTreeMap<String, String> {
    [
        ".agents/skills",
        ".claude/skills",
        ".codex/skills",
        ".gemini/skills",
        ".config/antigravity/skills",
    ]
    .into_iter()
    .map(|destination| (destination.into(), "skills".into()))
    .collect()
}
