use serde::{Deserialize, Serialize};
use vm_core::vm_println;
use vm_packages::PackageEcosystem;

use crate::error::{VmError, VmResult};

use super::runtime::{
    checkout_root, copy_private, exec, exec_in_workspace, exec_output, GuestRuntime,
};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OverrideRecord {
    checkout_id: String,
    consumer: String,
    package: String,
    ecosystem: PackageEcosystem,
    source: String,
    pinned_version: String,
}

impl OverrideRecord {
    pub(super) fn new(
        checkout_id: impl Into<String>,
        consumer: impl Into<String>,
        package: impl Into<String>,
        ecosystem: PackageEcosystem,
        source: impl Into<String>,
        pinned_version: impl Into<String>,
    ) -> Self {
        Self {
            checkout_id: checkout_id.into(),
            consumer: consumer.into(),
            package: package.into(),
            ecosystem,
            source: source.into(),
            pinned_version: pinned_version.into(),
        }
    }

    pub(super) fn write(&self, subject: &GuestRuntime, root: &str) -> VmResult<()> {
        let content = serde_json::to_vec_pretty(self).map_err(VmError::from)?;
        copy_private(subject, &content, &format!("{root}/override.json"))
    }

    pub(super) fn load(
        subject: &GuestRuntime,
        root: &str,
        checkout: &vm_packages::CheckoutRecord,
        consumer: &str,
    ) -> VmResult<Self> {
        let path = format!("{root}/override.json");
        let content = exec_output(subject, ["cat", path.as_str()]).map_err(|error| {
            VmError::validation(
                format!("Managed package override state is missing: {error}"),
                Some("The checkout was retained; restore its override state before cleanup"),
            )
        })?;
        let record: Self = serde_json::from_str(&content).map_err(|error| {
            VmError::validation(
                format!("Managed package override state is invalid: {error}"),
                Some("The checkout was retained; repair its override state before cleanup"),
            )
        })?;
        record.validate(checkout, consumer)?;
        Ok(record)
    }

    pub(super) fn load_optional(
        subject: &GuestRuntime,
        root: &str,
        checkout: &vm_packages::CheckoutRecord,
        consumer: &str,
    ) -> VmResult<Option<Self>> {
        let present = exec_output(
            subject,
            [
                "/bin/sh",
                "-c",
                "if [ -f \"$1/override.json\" ]; then printf yes; else printf no; fi",
                "vm-package-override",
                root,
            ],
        )?;
        if present.trim() == "yes" {
            Self::load(subject, root, checkout, consumer).map(Some)
        } else {
            Ok(None)
        }
    }

    pub(super) fn activate(&self, subject: &GuestRuntime) -> VmResult<()> {
        if self.ecosystem == PackageEcosystem::Cargo {
            self.install_cargo(subject)
        } else {
            exec_in_workspace(
                subject,
                dependency_command(
                    self.ecosystem,
                    &self.package,
                    DependencySource::Worktree(&self.source),
                ),
            )
        }
    }

    pub(super) fn restore(&self, subject: &GuestRuntime) -> VmResult<()> {
        if self.ecosystem == PackageEcosystem::Cargo {
            return remove_cargo(subject, &self.checkout_id);
        }
        exec_in_workspace(
            subject,
            dependency_command(
                self.ecosystem,
                &self.package,
                DependencySource::Published(&self.pinned_version),
            ),
        )
    }

    fn validate(&self, checkout: &vm_packages::CheckoutRecord, consumer: &str) -> VmResult<()> {
        if self.checkout_id != checkout.checkout_id
            || self.package != checkout.package
            || self.consumer != consumer
            || self.source != format!("{}/source", self.root()?)
        {
            return Err(VmError::validation(
                "Managed package override does not match the checkout",
                Some("The checkout was retained to protect source and dependency state"),
            ));
        }
        Ok(())
    }

    fn root(&self) -> VmResult<&str> {
        let root = self.source.strip_suffix("/source").ok_or_else(|| {
            VmError::validation(
                "Managed package source has an unexpected location",
                None::<String>,
            )
        })?;
        let suffix = format!("/package-checkouts/{}", self.checkout_id);
        if !root.ends_with(&suffix) {
            return Err(VmError::validation(
                "Managed package source escaped checkout storage",
                None::<String>,
            ));
        }
        Ok(root)
    }

    fn home(&self) -> VmResult<&str> {
        self.root()?
            .strip_suffix(&format!(
                "/.local/share/vm/package-checkouts/{}",
                self.checkout_id
            ))
            .filter(|home| home.starts_with('/') && !home.is_empty())
            .ok_or_else(|| {
                VmError::validation(
                    "Managed package checkout is outside the guest cache",
                    None::<String>,
                )
            })
    }

    fn install_cargo(&self, subject: &GuestRuntime) -> VmResult<()> {
        let root = self.root()?;
        let home = self.home()?;
        let wrapper = format!("{home}/.local/bin/cargo");
        let wrapper_state = exec_output(
            subject,
            [
                "/bin/sh",
                "-c",
                "if [ ! -e \"$1\" ]; then printf missing; elif grep -Fq '# vm-managed cargo override wrapper' \"$1\"; then printf managed; else printf foreign; fi",
                "vm-cargo-wrapper",
                wrapper.as_str(),
            ],
        )?;
        if wrapper_state.trim() == "foreign" {
            return Err(VmError::validation(
                format!("Refusing to replace existing Cargo executable at {wrapper}"),
                Some("Move the executable or use a guest without a conflicting ~/.local/bin/cargo"),
            ));
        }

        let fragment = format!("{root}/cargo.config");
        copy_private(
            subject,
            format!(
                "{}\n{}\n",
                self.source,
                cargo_patch(&self.package, &self.source)
            )
            .as_bytes(),
            &fragment,
        )?;

        if wrapper_state.trim() == "missing" {
            let actual = exec_output(subject, ["/bin/sh", "-lc", "command -v cargo"])?;
            let actual = actual.trim();
            if actual.is_empty() || actual == wrapper {
                return Err(VmError::validation(
                    "Cargo executable is unavailable in the guest",
                    None::<String>,
                ));
            }
            exec(subject, ["mkdir", "-p", &format!("{home}/.local/bin")])?;
            let script = cargo_wrapper(actual);
            copy_private(subject, script.as_bytes(), &wrapper)?;
            exec(subject, ["chmod", "700", wrapper.as_str()])?;
        }
        vm_println!("Cargo override active for {}", self.package);
        Ok(())
    }
}

pub(super) fn cleanup_failed_attach(subject: &GuestRuntime, root: &str) -> VmResult<()> {
    let has_record = exec_output(
        subject,
        [
            "/bin/sh",
            "-c",
            "if [ -f \"$1/override.json\" ]; then printf yes; else printf no; fi",
            "vm-package-cleanup",
            root,
        ],
    )?;
    if has_record.trim() == "yes" {
        let path = format!("{root}/override.json");
        let content = exec_output(subject, ["cat", path.as_str()])?;
        let record: OverrideRecord = serde_json::from_str(&content).map_err(VmError::from)?;
        record.restore(subject)?;
    }
    exec(subject, ["rm", "-rf", "--", root])
}

pub(super) fn cargo_patch(package: &str, source: &str) -> String {
    format!(
        "patch.crates-io.{}.path={}",
        serde_json::to_string(package).expect("package names serialize"),
        serde_json::to_string(source).expect("managed paths serialize")
    )
}

fn cargo_wrapper(actual: &str) -> String {
    let actual = quote_posix_argument(actual);
    format!(
        r#"#!/bin/sh
# vm-managed cargo override wrapper
set -eu
for fragment in "$HOME"/.local/share/vm/package-checkouts/*/cargo.config; do
  [ -f "$fragment" ] || continue
  source_path="$(sed -n '1p' "$fragment")"
  cargo_patch="$(sed -n '2p' "$fragment")"
  if [ ! -d "$source_path" ]; then
    echo "vm: assigned Cargo checkout is missing: $source_path" >&2
    exit 66
  fi
  set -- --config "$cargo_patch" "$@"
done
exec {actual} "$@"
"#
    )
}

fn remove_cargo(subject: &GuestRuntime, checkout_id: &str) -> VmResult<()> {
    let root = checkout_root(subject, checkout_id)?;
    let fragment = format!("{root}/cargo.config");
    exec(subject, ["rm", "-f", "--", fragment.as_str()])?;
    let home = root
        .strip_suffix(&format!("/.local/share/vm/package-checkouts/{checkout_id}"))
        .ok_or_else(|| VmError::validation("Managed checkout root is invalid", None::<String>))?;
    let wrapper = format!("{home}/.local/bin/cargo");
    let base = format!("{home}/.local/share/vm/package-checkouts");
    exec(
        subject,
        [
            "/bin/sh",
            "-c",
            "if ! find \"$1\" -mindepth 2 -maxdepth 2 -type f -name cargo.config -print -quit 2>/dev/null | grep -q .; then if [ -f \"$2\" ] && grep -Fq '# vm-managed cargo override wrapper' \"$2\"; then rm -f -- \"$2\"; fi; fi",
            "vm-cargo-cleanup",
            base.as_str(),
            wrapper.as_str(),
        ],
    )
}

#[derive(Clone, Copy)]
enum DependencySource<'a> {
    Worktree(&'a str),
    Published(&'a str),
}

fn dependency_command(
    ecosystem: PackageEcosystem,
    package: &str,
    source: DependencySource<'_>,
) -> Vec<String> {
    match (ecosystem, source) {
        (PackageEcosystem::Npm, DependencySource::Worktree(path)) => {
            ["npm", "install", "--no-save", "--package-lock=false", path]
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        (PackageEcosystem::Npm, DependencySource::Published(version)) => vec![
            "npm".into(),
            "install".into(),
            "--no-save".into(),
            "--package-lock=false".into(),
            format!("{package}@{version}"),
        ],
        (PackageEcosystem::Python, DependencySource::Worktree(path)) => {
            ["python", "-m", "pip", "install", "--editable", path]
                .into_iter()
                .map(str::to_string)
                .collect()
        }
        (PackageEcosystem::Python, DependencySource::Published(version)) => vec![
            "python".into(),
            "-m".into(),
            "pip".into(),
            "install".into(),
            "--force-reinstall".into(),
            "--no-deps".into(),
            format!("{package}=={version}"),
        ],
        (PackageEcosystem::Cargo, _) => Vec::new(),
    }
}

fn quote_posix_argument(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecosystem_commands_do_not_modify_manifests_or_lockfiles() {
        assert_eq!(
            dependency_command(
                PackageEcosystem::Npm,
                "@internal/auth",
                DependencySource::Published("1.4.2")
            ),
            [
                "npm",
                "install",
                "--no-save",
                "--package-lock=false",
                "@internal/auth@1.4.2"
            ]
        );
        assert_eq!(
            dependency_command(
                PackageEcosystem::Python,
                "internal-auth",
                DependencySource::Worktree("/tmp/auth")
            ),
            ["python", "-m", "pip", "install", "--editable", "/tmp/auth"]
        );
        assert!(dependency_command(
            PackageEcosystem::Cargo,
            "auth",
            DependencySource::Worktree("/tmp/auth")
        )
        .is_empty());
        assert_eq!(
            cargo_patch("auth-core", "/tmp/auth"),
            "patch.crates-io.\"auth-core\".path=\"/tmp/auth\""
        );
    }

    #[test]
    fn override_state_is_confined_to_the_managed_guest_cache() {
        let record = OverrideRecord::new(
            "pkg-auth-20260811-000001",
            "project-a",
            "auth",
            PackageEcosystem::Cargo,
            "/home/developer/.local/share/vm/package-checkouts/pkg-auth-20260811-000001/source",
            "1.4.2",
        );
        assert_eq!(
            record.root().unwrap(),
            "/home/developer/.local/share/vm/package-checkouts/pkg-auth-20260811-000001"
        );
        assert_eq!(record.home().unwrap(), "/home/developer");

        let escaped = OverrideRecord::new(
            "pkg-auth-20260811-000001",
            "project-a",
            "auth",
            PackageEcosystem::Cargo,
            "/workspace/source",
            "1.4.2",
        );
        assert!(escaped.root().is_err());
    }

    #[test]
    fn cargo_wrapper_fails_closed_when_a_checkout_disappears() {
        let wrapper = cargo_wrapper("/home/developer/.cargo/bin/cargo");
        assert!(wrapper.contains("assigned Cargo checkout is missing"));
        assert!(wrapper.contains("exit 66"));
        assert!(wrapper.contains("exec '/home/developer/.cargo/bin/cargo'"));
    }
}
