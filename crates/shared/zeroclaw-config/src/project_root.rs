use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

use crate::schema::Config;

const PROJECT_RUNTIME_DIR: &str = ".zeroclaw";

pub fn canonicalize_project_root(project_root: &Path) -> Result<PathBuf> {
    if project_root.as_os_str().is_empty() {
        bail!("project_root must not be empty");
    }

    if !project_root.exists() {
        bail!("project_root does not exist: {}", project_root.display());
    }

    if !project_root.is_dir() {
        bail!(
            "project_root must be a directory: {}",
            project_root.display()
        );
    }

    project_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize project_root: {}",
            project_root.display()
        )
    })
}

pub fn apply_project_root(config: &mut Config, project_root: &Path) -> Result<PathBuf> {
    let canonical = canonicalize_project_root(project_root)?;
    let runtime_dir = canonical.join(PROJECT_RUNTIME_DIR);
    std::fs::create_dir_all(&runtime_dir).with_context(|| {
        format!(
            "failed to create runtime directory: {}",
            runtime_dir.display()
        )
    })?;

    config.workspace_dir = canonical.clone();
    config.config_path = runtime_dir.join("config.toml");
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_project_root_returns_absolute_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("project");
        std::fs::create_dir_all(&nested).unwrap();

        let canonical = canonicalize_project_root(&nested).unwrap();

        assert!(canonical.is_absolute());
        assert_eq!(canonical, nested.canonicalize().unwrap());
    }

    #[test]
    fn canonicalize_project_root_rejects_missing_paths() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-project");

        let error = canonicalize_project_root(&missing).unwrap_err().to_string();
        assert!(error.contains("does not exist"));
    }

    #[test]
    fn canonicalize_project_root_rejects_files() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("not-a-directory.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let error = canonicalize_project_root(&file_path)
            .unwrap_err()
            .to_string();
        assert!(error.contains("directory"));
    }

    #[test]
    fn apply_project_root_updates_workspace_and_runtime_config_path() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();

        let mut config = Config::default();
        config.workspace_dir = temp.path().join("workspace");
        config.config_path = temp.path().join("config.toml");

        let canonical = apply_project_root(&mut config, &project).unwrap();

        assert_eq!(config.workspace_dir, canonical);
        assert_eq!(
            config.config_path,
            canonical.join(".zeroclaw").join("config.toml")
        );
    }
}
