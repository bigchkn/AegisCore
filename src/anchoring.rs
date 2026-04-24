use std::path::{Path, PathBuf};
use crate::error::AegisCliError;

pub struct ProjectAnchor {
    pub project_root: PathBuf,
    pub aegis_dir: PathBuf,
}

impl ProjectAnchor {
    /// Walk up from `cwd` looking for a `.aegis/` directory.
    pub fn discover(cwd: &Path) -> Result<Self, AegisCliError> {
        let mut current = cwd.to_path_buf();
        loop {
            let candidate = current.join(".aegis");
            if candidate.is_dir() {
                return Ok(Self {
                    project_root: current,
                    aegis_dir: candidate,
                });
            }
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => return Err(AegisCliError::NotAnAegisProject),
            }
        }
    }

    /// Used by `aegis init` — returns cwd without requiring `.aegis/` to exist.
    pub fn use_cwd(cwd: &Path) -> Self {
        Self {
            project_root: cwd.to_path_buf(),
            aegis_dir: cwd.join(".aegis"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn discover_finds_aegis_in_current_dir() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".aegis")).unwrap();
        let anchor = ProjectAnchor::discover(tmp.path()).unwrap();
        assert_eq!(anchor.project_root, tmp.path());
    }

    #[test]
    fn discover_walks_up_two_levels() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join(".aegis")).unwrap();
        let subdir = tmp.path().join("a").join("b");
        fs::create_dir_all(&subdir).unwrap();
        let anchor = ProjectAnchor::discover(&subdir).unwrap();
        assert_eq!(anchor.project_root, tmp.path());
    }

    #[test]
    fn discover_fails_at_root() {
        // Use a path that definitely has no .aegis/ above it — a temp dir that
        // we know has no parent .aegis/.  We walk up, but the tempdir itself
        // has no .aegis/, so we keep going until we reach /.
        // Rather than actually walking the real fs (which would be slow and
        // depend on the environment), we test with a non-existent path instead.
        let result = ProjectAnchor::discover(Path::new("/"));
        assert!(matches!(result, Err(AegisCliError::NotAnAegisProject)));
    }

    #[test]
    fn use_cwd_does_not_require_aegis_dir() {
        let anchor = ProjectAnchor::use_cwd(Path::new("/tmp/new-project"));
        assert_eq!(anchor.project_root, Path::new("/tmp/new-project"));
        assert_eq!(anchor.aegis_dir, Path::new("/tmp/new-project/.aegis"));
    }
}
