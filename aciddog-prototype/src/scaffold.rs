use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScaffoldError {
    #[error("scaffold template not found at {0}")]
    TemplateMissing(PathBuf),

    #[error("io error while scaffolding {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub fn ensure_demo(demo_dir: &Path, template_dir: &Path) -> Result<bool, ScaffoldError> {
    if demo_dir.exists() {
        return Ok(false);
    }
    if !template_dir.exists() {
        return Err(ScaffoldError::TemplateMissing(template_dir.to_path_buf()));
    }
    copy_recursive(template_dir, demo_dir)?;
    Ok(true)
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<(), ScaffoldError> {
    std::fs::create_dir_all(dst).map_err(|e| ScaffoldError::Io {
        path: dst.to_path_buf(),
        source: e,
    })?;
    for entry in std::fs::read_dir(src).map_err(|e| ScaffoldError::Io {
        path: src.to_path_buf(),
        source: e,
    })? {
        let entry = entry.map_err(|e| ScaffoldError::Io {
            path: src.to_path_buf(),
            source: e,
        })?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry.file_type().map_err(|e| ScaffoldError::Io {
            path: from.clone(),
            source: e,
        })?;
        if file_type.is_dir() {
            copy_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| ScaffoldError::Io {
                path: to.clone(),
                source: e,
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ensure_demo_returns_false_when_directory_exists() {
        let tmp = TempDir::new().unwrap();
        let demo = tmp.path().join("demo");
        std::fs::create_dir_all(&demo).unwrap();
        let tpl = tmp.path().join("tpl");
        std::fs::create_dir_all(&tpl).unwrap();

        let created = ensure_demo(&demo, &tpl).unwrap();
        assert!(!created);
    }

    #[test]
    fn ensure_demo_copies_template_when_missing() {
        let tmp = TempDir::new().unwrap();
        let demo = tmp.path().join("demo");
        let tpl = tmp.path().join("tpl");
        std::fs::create_dir_all(&tpl).unwrap();
        std::fs::write(tpl.join("aciddog.wcl"), "team x { description = \"t\" }").unwrap();

        let created = ensure_demo(&demo, &tpl).unwrap();
        assert!(created);
        assert!(demo.join("aciddog.wcl").is_file());
    }

    #[test]
    fn ensure_demo_errors_when_template_missing() {
        let tmp = TempDir::new().unwrap();
        let demo = tmp.path().join("demo");
        let tpl = tmp.path().join("nope");
        let err = ensure_demo(&demo, &tpl).unwrap_err();
        assert!(matches!(err, ScaffoldError::TemplateMissing(_)));
    }
}
