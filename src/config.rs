//! Config file (`.reqtrace.toml`): discovery, parsing, severity policy.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every check name, alphabetical — also the JSON/report sort domain.
pub const CHECKS: &[&str] = &[
    "malformed-annotation",
    "orphaned",
    "stale",
    "stale-exception",
    "uncovered",
    "unparented",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Fail,
    Warn,
    Off,
    ByRating,
}

impl Policy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fail" => Some(Policy::Fail),
            "warn" => Some(Policy::Warn),
            "off" => Some(Policy::Off),
            "by-rating" => Some(Policy::ByRating),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub globs: Vec<String>,
    /// Exception file path, resolved relative to the config file's directory.
    pub exceptions: Option<PathBuf>,
    pub severity: BTreeMap<String, Policy>,
}

impl Config {
    /// Built-in defaults per the spec's check table. No scan globs — with
    /// neither doc-paths nor config globs, `check` is a usage error.
    pub fn defaults() -> Self {
        let mut severity = BTreeMap::new();
        severity.insert("uncovered".into(), Policy::ByRating);
        severity.insert("stale".into(), Policy::ByRating);
        severity.insert("orphaned".into(), Policy::Fail);
        severity.insert("unparented".into(), Policy::Off);
        severity.insert("stale-exception".into(), Policy::Warn);
        severity.insert("malformed-annotation".into(), Policy::Fail);
        Config {
            globs: Vec::new(),
            exceptions: None,
            severity,
        }
    }

    pub fn policy(&self, check: &str) -> Policy {
        *self.severity.get(check).unwrap_or(&Policy::Off)
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    scan: RawScan,
    #[serde(default)]
    severity: BTreeMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawScan {
    #[serde(default)]
    globs: Vec<String>,
    #[serde(default)]
    exceptions: Option<String>,
}

/// Covers: cli~config-discovery~1
pub fn discover(explicit: Option<&Path>, cwd: &Path) -> Result<Config> {
    if let Some(path) = explicit {
        return load(path);
    }
    for dir in cwd.ancestors() {
        let candidate = dir.join(".reqtrace.toml");
        if candidate.is_file() {
            return load(&candidate);
        }
    }
    Ok(Config::defaults())
}

/// Config severity replaces defaults, per check.
///
/// Covers: chk~severity-config~1
pub fn load(path: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading config {}", path.display()))?;
    let raw: RawConfig =
        toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;

    let mut cfg = Config::defaults();
    cfg.globs = raw.scan.globs;
    if let Some(exc) = raw.scan.exceptions {
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        cfg.exceptions = Some(base.join(exc));
    }
    for (check, value) in raw.severity {
        if !CHECKS.contains(&check.as_str()) {
            bail!(
                "{}: unknown check `{}` in [severity]",
                path.display(),
                check
            );
        }
        let policy = Policy::parse(&value).ok_or_else(|| {
            anyhow::anyhow!(
                "{}: invalid severity `{}` for `{}` (want fail|warn|off|by-rating)",
                path.display(),
                value,
                check
            )
        })?;
        cfg.severity.insert(check, policy);
    }
    Ok(cfg)
}
