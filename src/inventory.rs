//! Inventory loading, validation, and current-rev arithmetic.

use crate::id::ReqId;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Extracted,
    Assumed,
    Confirmed,
    Disputed,
    Retired,
}

impl Status {
    // Covers: inv~status-set~1
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "extracted" => Some(Status::Extracted),
            "assumed" => Some(Status::Assumed),
            "confirmed" => Some(Status::Confirmed),
            "disputed" => Some(Status::Disputed),
            "retired" => Some(Status::Retired),
            _ => None,
        }
    }

    // Covers: inv~coverage-denominator~2
    pub fn in_denominator(self) -> bool {
        matches!(self, Status::Confirmed | Status::Assumed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    H,
    M,
    L,
}

impl Rating {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "H" => Some(Rating::H),
            "M" => Some(Rating::M),
            "L" => Some(Rating::L),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Rating::H => "H",
            Rating::M => "M",
            Rating::L => "L",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub id: ReqId,
    pub status: Status,
    /// `[value, risk]`, each H|M|L.
    pub rating: Option<Vec<Rating>>,
    pub owner: Option<String>,
    pub file: String,
}

impl Item {
    /// The *value* component — first element — drives by-rating severity.
    pub fn rating_value(&self) -> Option<Rating> {
        self.rating.as_ref().and_then(|r| r.first().copied())
    }

    pub fn rating_strings(&self) -> Option<Vec<String>> {
        self.rating
            .as_ref()
            .map(|r| r.iter().map(|v| v.as_str().to_string()).collect())
    }
}

#[derive(Debug)]
pub struct Inventory {
    pub items: Vec<Item>,
    /// slug_key -> index into `items` of the current (highest-rev) item.
    current: BTreeMap<String, usize>,
}

impl Inventory {
    pub fn load(path: &Path) -> Result<Self> {
        let files = inventory_files(path)?;
        if files.is_empty() {
            bail!("no *.yaml inventory files under {}", path.display());
        }
        let mut items: Vec<Item> = Vec::new();
        for file in &files {
            let text = std::fs::read_to_string(file)
                .with_context(|| format!("reading inventory {}", file.display()))?;
            if text.trim().is_empty() {
                continue;
            }
            let docs: Vec<serde_yaml::Value> = serde_yaml::from_str(&text)
                .with_context(|| format!("parsing inventory {}", file.display()))?;
            for value in docs {
                items.push(parse_item(&value, file, &text)?);
            }
        }

        // Covers: inv~dup-ids~2
        let mut seen: BTreeSet<(String, u32)> = BTreeSet::new();
        for item in &items {
            if !seen.insert((item.id.slug_key(), item.id.rev)) {
                bail!(
                    "duplicate ID `{}` in {}: same type~slug and rev appears twice",
                    item.id,
                    item.file
                );
            }
        }

        // Covers: inv~current-rev~1
        let mut current: BTreeMap<String, usize> = BTreeMap::new();
        for (idx, item) in items.iter().enumerate() {
            let key = item.id.slug_key();
            match current.get(&key) {
                Some(&prev) if items[prev].id.rev >= item.id.rev => {}
                _ => {
                    current.insert(key, idx);
                }
            }
        }

        Ok(Inventory { items, current })
    }

    pub fn current_item(&self, slug_key: &str) -> Option<&Item> {
        self.current.get(slug_key).map(|&i| &self.items[i])
    }

    pub fn current_rev(&self, slug_key: &str) -> Option<u32> {
        self.current_item(slug_key).map(|i| i.id.rev)
    }

    pub fn has_slug(&self, slug_key: &str) -> bool {
        self.current.contains_key(slug_key)
    }

    /// Slug keys whose current rev is `confirmed` or `assumed`.
    ///
    /// Covers: inv~coverage-denominator~2, inv~current-rev~1
    pub fn denominator(&self) -> BTreeSet<String> {
        self.current
            .iter()
            .filter(|(_, &i)| self.items[i].status.in_denominator())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

fn parse_item(value: &serde_yaml::Value, file: &Path, text: &str) -> Result<Item> {
    let map = value
        .as_mapping()
        .ok_or_else(|| anyhow!("{}: inventory entries must be mappings", file.display()))?;

    let id_str = str_field(map, "id")
        .ok_or_else(|| anyhow!("{}: inventory entry missing required `id`", file.display()))?;
    let line = find_line(text, &id_str);

    // Reject with the offending file/line.
    // Covers: inv~id-grammar~2
    let id = ReqId::parse(&id_str).ok_or_else(|| {
        anyhow!(
            "{}:{}: invalid requirement ID `{}`",
            file.display(),
            line,
            id_str
        )
    })?;

    let status_str = str_field(map, "status").ok_or_else(|| {
        anyhow!(
            "{}:{}: item `{}` missing required `status`",
            file.display(),
            line,
            id
        )
    })?;
    let status = Status::parse(&status_str).ok_or_else(|| {
        anyhow!(
            "{}:{}: item `{}` has invalid status `{}`",
            file.display(),
            line,
            id,
            status_str
        )
    })?;

    let rating = match map.get("rating") {
        None | Some(serde_yaml::Value::Null) => None,
        Some(serde_yaml::Value::Sequence(seq)) => {
            if seq.is_empty() || seq.len() > 2 {
                bail!(
                    "{}:{}: item `{}` rating must be [value] or [value, risk]",
                    file.display(),
                    line,
                    id
                );
            }
            let mut ratings = Vec::new();
            for v in seq {
                let s = v.as_str().unwrap_or_default();
                let r = Rating::parse(s).ok_or_else(|| {
                    anyhow!(
                        "{}:{}: item `{}` has invalid rating `{}` (want H|M|L)",
                        file.display(),
                        line,
                        id,
                        s
                    )
                })?;
                ratings.push(r);
            }
            Some(ratings)
        }
        Some(_) => bail!(
            "{}:{}: item `{}` rating must be a list",
            file.display(),
            line,
            id
        ),
    };

    let owner = str_field(map, "owner");

    // All other keys are preserved in the YAML but ignored by v1.
    Ok(Item {
        id,
        status,
        rating,
        owner,
        file: file.display().to_string(),
    })
}

fn str_field(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn find_line(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|l| l.contains(needle))
        .map(|i| i + 1)
        .unwrap_or(0)
}

fn inventory_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        bail!("inventory path {} does not exist", path.display());
    }
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(path)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("yaml"))
        .map(|e| e.into_path())
        .collect();
    files.sort();
    Ok(files)
}
