use crate::model::ProjectIndex;
use aegis_core::{AegisError, Result};
use std::fs;
use std::path::Path;

pub fn parse_index_file(path: &Path) -> Result<ProjectIndex> {
    let content = fs::read_to_string(path).map_err(|e| AegisError::StorageIo {
        path: path.to_path_buf(),
        source: e,
    })?;

    let index: ProjectIndex =
        toml::from_str(&content).map_err(|e| AegisError::ConfigParseError {
            path: path.to_path_buf(),
            source: e,
        })?;

    Ok(index)
}
