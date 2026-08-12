use std::collections::BTreeMap;

use vm_packages::{
    validate_sha256, validate_tool_name, validate_tool_target, validate_tool_version,
    RegistryEndpoints, ToolArtifactRecord,
};
use vm_provider::Provider;

use crate::error::{VmError, VmResult};

const INSTALLER: &str = include_str!("guest-installer.sh");
const LAUNCHER: &str = r#"
set -eu
umask 077
IFS= read -r CARGO_REGISTRIES_VM_TOKEN
test -n "$CARGO_REGISTRIES_VM_TOKEN"
export CARGO_REGISTRIES_VM_TOKEN
root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools"
mkdir -p "$root"
script="$root/installer.sh"
temporary="$root/.installer.$$.tmp"
printf '%s\n' "$1" > "$temporary"
chmod 700 "$temporary"
mv -f "$temporary" "$script"
mode=$2
shift 2
if test "$mode" = background; then
  nohup "$script" "$@" > "$root/update.log" 2>&1 </dev/null &
else
  exec "$script" "$@"
fi
"#;
const STATE_SCRIPT: &str = r#"
root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools/state"
for state in "$root"/*.state; do
  test -f "$state" || continue
  cat "$state"
done
"#;
const CONSUMABLE_SCRIPT: &str = r#"
root="${XDG_DATA_HOME:-$HOME/.local/share}/vm-tools"
states="$root/state"
releases="$root/releases"
canonical_path() {
  candidate=$1
  if command -v realpath >/dev/null 2>&1; then
    realpath "$candidate" 2>/dev/null && return 0
  fi
  if readlink -f "$candidate" >/dev/null 2>&1; then
    readlink -f "$candidate"
    return
  fi
  depth=0
  while test -L "$candidate"; do
    depth=$((depth + 1))
    test "$depth" -le 40 || return 1
    target=$(readlink "$candidate") || return 1
    case "$target" in
      /*) candidate=$target ;;
      *) candidate="$(dirname "$candidate")/$target" ;;
    esac
  done
  parent=$(CDPATH= cd -P "$(dirname "$candidate")" 2>/dev/null && pwd) || return 1
  printf '%s/%s\n' "$parent" "$(basename "$candidate")"
}
resolved_below() {
  expected=$1
  candidate=$2
  expected="$(canonical_path "$expected" 2>/dev/null || true)"
  candidate="$(canonical_path "$candidate" 2>/dev/null || true)"
  test -n "$expected" && test -n "$candidate" || return 1
  case "$candidate" in
    "$expected"|"$expected"/*) return 0 ;;
    *) return 1 ;;
  esac
}
for state in "$states"/*.state; do
  test -f "$state" || continue
  tab="$(printf '\t')"
  IFS="$tab" read -r name _version _target _digest < "$state"
  links="$states/$name.links"
  result=yes
  if test ! -s "$links"; then
    result=no
  else
    while IFS= read -r destination; do
      if test -z "$destination" || test ! -L "$destination" \
        || test ! -e "$destination" \
        || ! resolved_below "$releases/$name" "$destination"; then
        result=no
        break
      fi
    done < "$links"
  fi
  printf '%s\t%s\n' "$name" "$result"
done
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InstalledTool {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) target: String,
    pub(super) digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstallMode {
    Background,
    Wait,
}

impl InstallMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Wait => "wait",
        }
    }
}

pub(super) fn platform_target(provider: &dyn Provider, environment: &str) -> VmResult<String> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), "uname -s; uname -m".into()],
        )
        .map_err(VmError::from)?;
    platform_target_from_uname(&output)
}

pub(super) fn installed(
    provider: &dyn Provider,
    environment: &str,
) -> VmResult<BTreeMap<String, InstalledTool>> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), STATE_SCRIPT.into()],
        )
        .map_err(VmError::from)?;
    Ok(parse_installed(&output))
}

pub(super) fn consumable(
    provider: &dyn Provider,
    environment: &str,
) -> VmResult<BTreeMap<String, bool>> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), CONSUMABLE_SCRIPT.into()],
        )
        .map_err(VmError::from)?;
    Ok(parse_consumable(&output))
}

pub(super) fn install(
    provider: &dyn Provider,
    environment: &str,
    artifacts: &[ToolArtifactRecord],
    gateway: &str,
    read_token: &str,
    mode: InstallMode,
) -> VmResult<()> {
    if artifacts.is_empty() {
        return Ok(());
    }
    let gateway = RegistryEndpoints::new(gateway).map_err(VmError::from)?;
    let mut command = vec![
        "sh".to_string(),
        "-c".to_string(),
        LAUNCHER.to_string(),
        "vm-tool-launcher".to_string(),
        INSTALLER.to_string(),
        mode.as_str().to_string(),
    ];
    for artifact in artifacts {
        command.push(manifest(artifact, gateway.gateway())?);
    }
    let input = format!("{read_token}\n");
    provider
        .exec_with_stdin(Some(environment), &command, input.as_bytes())
        .map_err(VmError::from)
}

fn manifest(artifact: &ToolArtifactRecord, gateway: &str) -> VmResult<String> {
    artifact.validate().map_err(VmError::from)?;
    let kind = match artifact.kind {
        vm_packages::ToolKind::Binary => "binary",
        vm_packages::ToolKind::Collection => "collection",
    };
    let mut manifest = format!(
        "{}\t{}\t{}\t{}\t{}\t{}{}",
        artifact.tool,
        artifact.version,
        artifact.target,
        kind,
        artifact.artifact_digest,
        gateway.trim_end_matches('/'),
        artifact.artifact_path
    );
    for (destination, source) in &artifact.links {
        manifest.push('\n');
        manifest.push_str(destination);
        manifest.push('\t');
        manifest.push_str(source);
    }
    Ok(manifest)
}

fn platform_target_from_uname(output: &str) -> VmResult<String> {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let system = lines.next().unwrap_or_default().to_ascii_lowercase();
    let architecture = lines.next().unwrap_or_default().to_ascii_lowercase();
    let os = match system.as_str() {
        "linux" => "linux",
        "darwin" => "darwin",
        _ => {
            return Err(VmError::validation(
                format!("Unsupported guest operating system '{system}'"),
                None::<String>,
            ));
        }
    };
    let architecture = match architecture.as_str() {
        "arm64" | "aarch64" => "arm64",
        "amd64" | "x86_64" => "amd64",
        _ => {
            return Err(VmError::validation(
                format!("Unsupported guest architecture '{architecture}'"),
                None::<String>,
            ));
        }
    };
    Ok(format!("{os}-{architecture}"))
}

fn parse_installed(output: &str) -> BTreeMap<String, InstalledTool> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 4
                || validate_tool_name(fields[0]).is_err()
                || validate_tool_version(fields[1]).is_err()
                || validate_tool_target(fields[2]).is_err()
                || validate_sha256(fields[3]).is_err()
            {
                return None;
            }
            let tool = InstalledTool {
                name: fields[0].into(),
                version: fields[1].into(),
                target: fields[2].into(),
                digest: fields[3].into(),
            };
            Some((tool.name.clone(), tool))
        })
        .collect()
}

fn parse_consumable(output: &str) -> BTreeMap<String, bool> {
    output
        .lines()
        .filter_map(|line| {
            let (name, state) = line.split_once('\t')?;
            if validate_tool_name(name).is_err() {
                return None;
            }
            match state {
                "yes" => Some((name.to_string(), true)),
                "no" => Some((name.to_string(), false)),
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::{fs, process::Command};
    use vm_packages::ToolKind;

    #[test]
    fn maps_linux_and_macos_guest_targets() {
        assert_eq!(
            platform_target_from_uname("Linux\naarch64\n").unwrap(),
            "linux-arm64"
        );
        assert_eq!(
            platform_target_from_uname("Darwin\narm64\n").unwrap(),
            "darwin-arm64"
        );
        assert!(platform_target_from_uname("Plan9\nx86_64\n").is_err());
    }

    #[test]
    fn parses_only_valid_guest_state_lines() {
        let output = format!("noise\ncodex\t1.2.3\tlinux-arm64\t{}\n", "a".repeat(64));
        let installed = parse_installed(&output);
        assert_eq!(installed["codex"].version, "1.2.3");
        assert_eq!(installed.len(), 1);
    }

    #[test]
    fn parses_consumability_without_trusting_unknown_rows() {
        let states = parse_consumable("agent-skills\tyes\nbroken\tno\nnoise\nother\tmaybe\n");

        assert_eq!(states.get("agent-skills"), Some(&true));
        assert_eq!(states.get("broken"), Some(&false));
        assert_eq!(states.len(), 2);
        #[cfg(unix)]
        assert!(std::process::Command::new("/bin/sh")
            .args(["-n", "-c", CONSUMABLE_SCRIPT])
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn manifest_keeps_a_collection_as_one_artifact() {
        let artifact = ToolArtifactRecord {
            tool: "agent-skills".into(),
            kind: ToolKind::Collection,
            version: "1.0.0".into(),
            target: "any".into(),
            artifact_digest: "a".repeat(64),
            size_bytes: 1,
            links: BTreeMap::from([
                (".claude/skills".into(), "skills".into()),
                (".codex/skills".into(), "skills".into()),
            ]),
            source_repository: "https://example.com/skills.git".into(),
            source_commit: "b".repeat(40),
            tag: "v1.0.0".into(),
            artifact_path: vm_packages::tool_artifact_path(
                "agent-skills",
                "1.0.0",
                "any",
                &"a".repeat(64),
            ),
            actor: "release".into(),
            published_at: Utc::now(),
            receipt_id: "receipt-1".into(),
        };
        let manifest = manifest(&artifact, "http://packages.internal:3080").unwrap();
        assert_eq!(manifest.lines().count(), 3);
        assert!(manifest.starts_with("agent-skills\t1.0.0\tany\tcollection\t"));
    }

    #[test]
    fn collection_installer_merges_skill_siblings_into_an_existing_root() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let data = directory.path().join("data");
        let digest = "a".repeat(64);
        let release = data
            .join("vm-tools/releases/agent-skills")
            .join(format!("1.0.0-{digest}"))
            .join("skills");
        for skill in ["x-one", "x-two"] {
            let skill = release.join(skill);
            fs::create_dir_all(&skill).unwrap();
            fs::write(skill.join("SKILL.md"), "# Test\n").unwrap();
        }
        fs::create_dir_all(home.join(".codex/skills/.system")).unwrap();
        let installer = directory.path().join("installer.sh");
        fs::write(&installer, INSTALLER).unwrap();
        let manifest = format!(
            "agent-skills\t1.0.0\tany\tcollection\t{digest}\thttp://unused\n.codex/skills\tskills"
        );

        let output = Command::new("sh")
            .arg(&installer)
            .arg(&manifest)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &data)
            .env("CARGO_REGISTRIES_VM_TOKEN", "test-token")
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(home.join(".codex/skills/.system").is_dir());
        assert!(home.join(".codex/skills/x-one").is_symlink());
        assert!(home.join(".codex/skills/x-two").is_symlink());
        let links = fs::read_to_string(data.join("vm-tools/state/agent-skills.links")).unwrap();
        assert_eq!(links.lines().count(), 2);

        #[cfg(unix)]
        {
            fs::remove_file(home.join(".codex/skills/x-one")).unwrap();
            std::os::unix::fs::symlink(
                data.join("vm-tools/releases/agent-skills/missing/skills/x-one"),
                home.join(".codex/skills/x-one"),
            )
            .unwrap();
            let retry = Command::new("sh")
                .arg(&installer)
                .arg(&manifest)
                .env("HOME", &home)
                .env("XDG_DATA_HOME", &data)
                .env("CARGO_REGISTRIES_VM_TOKEN", "test-token")
                .output()
                .unwrap();
            assert!(
                retry.status.success(),
                "{}",
                String::from_utf8_lossy(&retry.stderr)
            );
            assert!(home.join(".codex/skills/x-one/SKILL.md").is_file());
        }
    }
}
