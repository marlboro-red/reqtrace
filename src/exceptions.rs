//! Exception (not-in-scope) file parsing.

use crate::id::ReqId;
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExceptionEntry {
    pub id: ReqId,
}

pub fn load(path: &Path) -> Result<Vec<ExceptionEntry>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading exception file {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let docs: Vec<serde_yaml::Value> = serde_yaml::from_str(&text)
        .with_context(|| format!("parsing exception file {}", path.display()))?;

    let mut entries = Vec::new();
    for value in docs {
        let map = value
            .as_mapping()
            .ok_or_else(|| anyhow!("{}: exception entries must be mappings", path.display()))?;
        let id_str = map
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("{}: exception entry missing `id`", path.display()))?;
        let id = ReqId::parse(id_str)
            .ok_or_else(|| anyhow!("{}: invalid exception ID `{}`", path.display(), id_str))?;

        // Covers: exc~justification~1
        let justification = map
            .get("justification")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if justification.trim().is_empty() {
            bail!(
                "{}: exception `{}` has no non-empty justification",
                path.display(),
                id
            );
        }
        entries.push(ExceptionEntry { id });
    }
    Ok(entries)
}
