use crate::model::Milestone;
use aegis_core::{AegisError, Result};
use std::fs;
use std::path::Path;

pub fn parse_milestone_file(path: &Path) -> Result<Milestone> {
    let content = fs::read_to_string(path).map_err(|e| AegisError::StorageIo {
        path: path.to_path_buf(),
        source: e,
    })?;

    let milestone: Milestone =
        toml::from_str(&content).map_err(|e| AegisError::ConfigParseError {
            path: path.to_path_buf(),
            source: e,
        })?;

    Ok(milestone)
}
