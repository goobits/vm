use serde::{Deserialize, Serialize};
use std::path::Path;
use vm_core::file_system::{has_any_dir, has_any_file, has_file};

/// Provider-neutral project markers detected once from the host workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFacts {
    pub package_json: bool,
    pub pnpm_lock: bool,
    pub npm_lock: bool,
    pub requirements_txt: bool,
    pub pyproject_toml: bool,
    pub setup_py: bool,
    pub pipfile: bool,
    pub gemfile: bool,
    pub cargo_toml: bool,
    pub go_mod: bool,
    pub composer_json: bool,
    pub docker: bool,
    pub kubernetes: bool,
}

impl ProjectFacts {
    #[must_use]
    pub fn detect(dir: &Path) -> Self {
        Self {
            package_json: has_file(dir, "package.json"),
            pnpm_lock: has_file(dir, "pnpm-lock.yaml"),
            npm_lock: has_file(dir, "package-lock.json"),
            requirements_txt: has_file(dir, "requirements.txt"),
            pyproject_toml: has_file(dir, "pyproject.toml"),
            setup_py: has_file(dir, "setup.py"),
            pipfile: has_file(dir, "Pipfile"),
            gemfile: has_file(dir, "Gemfile"),
            cargo_toml: has_file(dir, "Cargo.toml"),
            go_mod: has_file(dir, "go.mod"),
            composer_json: has_file(dir, "composer.json"),
            docker: has_any_file(
                dir,
                &["Dockerfile", "docker-compose.yml", "docker-compose.yaml"],
            ),
            kubernetes: has_any_file(dir, &["k8s.yaml", "k8s.yml"])
                || has_any_dir(dir, &["kubernetes", "k8s"]),
        }
    }

    #[must_use]
    pub fn has_python_project(&self) -> bool {
        self.requirements_txt || self.pyproject_toml || self.setup_py || self.pipfile
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectFacts;

    #[test]
    fn detects_multi_runtime_and_lockfile_facts() {
        let project = tempfile::tempdir().unwrap();
        for marker in [
            "package.json",
            "pnpm-lock.yaml",
            "pyproject.toml",
            "Cargo.toml",
            "go.mod",
        ] {
            std::fs::write(project.path().join(marker), "").unwrap();
        }

        let facts = ProjectFacts::detect(project.path());

        assert!(facts.package_json);
        assert!(facts.pnpm_lock);
        assert!(!facts.npm_lock);
        assert!(facts.has_python_project());
        assert!(facts.cargo_toml);
        assert!(facts.go_mod);
    }
}
