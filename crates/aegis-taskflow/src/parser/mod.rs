pub mod index;
pub mod milestone;

use crate::model::{Milestone, ProjectIndex};
use aegis_core::Result;
use std::path::Path;

pub fn parse_index(path: &Path) -> Result<ProjectIndex> {
    index::parse_index_file(path)
}

pub fn parse_milestone(path: &Path) -> Result<Milestone> {
    milestone::parse_milestone_file(path)
}
