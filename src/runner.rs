//! Command orchestration: scan-set resolution, check/validate entry points.

use crate::checks;
use crate::config::{self, Config};
use crate::exceptions;
use crate::inventory::Inventory;
use crate::report;
use crate::scan::{self, ScanFile};
use anyhow::{bail, Context, Result};
use globset::{Glob, GlobSetBuilder};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct CheckOpts {
    pub inventory: PathBuf,
    pub config: Option<PathBuf>,
    pub json: Option<PathBuf>,
    pub doc_paths: Vec<PathBuf>,
}

/// Runs `check`; returns the process exit code (0/1). All `Err` returns are
/// tool-input or I/O errors and map to exit 2 in `main`.
///
/// Covers: cli~error-lanes~1
pub fn run_check(opts: &CheckOpts) -> Result<i32> {
    let cwd = std::env::current_dir().context("resolving current directory")?;
    let cfg = config::discover(opts.config.as_deref(), &cwd)?;
    let inv = Inventory::load(&opts.inventory)?;
    let exc = match &cfg.exceptions {
        Some(p) => exceptions::load(p)?,
        None => Vec::new(),
    };

    let files = resolve_scan_set(&opts.doc_paths, &cfg, &cwd)?;
    let scanned = scan::scan_files(&files)?;

    let (mut findings, summary) = checks::run(&inv, &scanned, &exc, &cfg);
    report::sort_findings(&mut findings);

    if let Some(path) = &opts.json {
        std::fs::write(path, report::json(&findings, &summary))
            .with_context(|| format!("writing JSON report to {}", path.display()))?;
    }
    print!("{}", report::human(&findings, &summary));

    // Warnings alone exit 0.
    // Covers: cli~exit-codes~1
    Ok(if summary.fail > 0 { 1 } else { 0 })
}

pub struct ValidateOpts {
    pub inventory: PathBuf,
    pub config: Option<PathBuf>,
}

/// Lints the inventory and, when configured, the exception file.
pub fn run_validate(opts: &ValidateOpts) -> Result<i32> {
    let cwd = std::env::current_dir().context("resolving current directory")?;
    let inv = Inventory::load(&opts.inventory)?;
    let denominator = inv.denominator().len();
    let slugs: std::collections::BTreeSet<String> =
        inv.items.iter().map(|i| i.id.slug_key()).collect();
    println!(
        "inventory OK: {} item{}, {} slug{}, {} in denominator",
        inv.items.len(),
        if inv.items.len() == 1 { "" } else { "s" },
        slugs.len(),
        if slugs.len() == 1 { "" } else { "s" },
        denominator,
    );

    let cfg = config::discover(opts.config.as_deref(), &cwd)?;
    if let Some(path) = &cfg.exceptions {
        let entries = exceptions::load(path)?;
        println!(
            "exceptions OK: {} entr{}",
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" }
        );
    }
    Ok(0)
}

/// Doc-paths replace config globs entirely; neither → usage error.
///
/// Covers: cli~docpaths~1
pub fn resolve_scan_set(doc_paths: &[PathBuf], cfg: &Config, cwd: &Path) -> Result<Vec<ScanFile>> {
    // BTreeMap keyed by display path: dedups and gives a deterministic scan order.
    let mut set: BTreeMap<String, PathBuf> = BTreeMap::new();

    if !doc_paths.is_empty() {
        for p in doc_paths {
            if p.is_file() {
                set.insert(display_path(p, cwd), p.clone());
            } else if p.is_dir() {
                for entry in walkdir::WalkDir::new(p)
                    .sort_by_file_name()
                    .into_iter()
                    .filter_entry(|e| !is_hidden(e))
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                {
                    set.insert(display_path(entry.path(), cwd), entry.into_path());
                }
            } else {
                bail!("doc path {} does not exist", p.display());
            }
        }
    } else if !cfg.globs.is_empty() {
        let mut builder = GlobSetBuilder::new();
        for g in &cfg.globs {
            builder.add(Glob::new(g).with_context(|| format!("invalid glob `{}`", g))?);
        }
        let globs = builder.build().context("building glob set")?;
        for entry in walkdir::WalkDir::new(cwd)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| !is_hidden(e))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let rel = entry.path().strip_prefix(cwd).unwrap_or(entry.path());
            if globs.is_match(rel) {
                set.insert(display_path(entry.path(), cwd), entry.into_path());
            }
        }
    } else {
        bail!("no docs to scan: pass <doc-paths> or set [scan].globs in config");
    }

    Ok(set
        .into_iter()
        .map(|(display, path)| ScanFile { path, display })
        .collect())
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
}

/// Paths reported CWD-relative with `/` separators.
///
/// Covers: cli~determinism~2
fn display_path(path: &Path, cwd: &Path) -> String {
    let rel = path.strip_prefix(cwd).unwrap_or(path);
    let s = rel.to_string_lossy().replace('\\', "/");
    s.strip_prefix("./").unwrap_or(&s).to_string()
}
