//! Doc scanning: annotation grammar, heading attribution, fence skipping.

use crate::id::{ReqId, ID_PATTERN};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Covers,
    Derived,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    pub section: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub kind: LinkKind,
    pub id: ReqId,
    pub loc: Location,
}

#[derive(Debug, Clone)]
pub struct Malformed {
    pub loc: Location,
}

/// One heading-delimited section (or a whole non-Markdown file), for `unparented`.
#[derive(Debug, Clone)]
pub struct SectionRecord {
    pub loc: Location,
    pub has_content: bool,
    pub has_annotation: bool,
}

#[derive(Debug, Default)]
pub struct ScanOutput {
    pub links: Vec<Link>,
    pub malformed: Vec<Malformed>,
    pub sections: Vec<SectionRecord>,
}

/// A file to scan plus the path string used in reports (CWD-relative, `/`-separated).
#[derive(Debug, Clone)]
pub struct ScanFile {
    pub path: PathBuf,
    pub display: String,
}

// Plain / line-comment form.
// Covers: ann~grammar~2
fn plain_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"^\s*(?:[#/*;-]+\s*)?(Covers|Derived):\s*({id}(?:\s*,\s*{id})*)\s*$",
            id = ID_PATTERN
        ))
        .unwrap()
    })
}

// HTML-comment form; opener and closer both required.
// Covers: ann~grammar~2
fn html_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"^\s*<!--\s*(Covers|Derived):\s*({id}(?:\s*,\s*{id})*)\s*-->\s*$",
            id = ID_PATTERN
        ))
        .unwrap()
    })
}

// Keyword after optional prefix, but full line fails the grammar.
// Covers: ann~malformed~2
fn trigger_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*(?:<!--\s*)?(?:[#/*;-]+\s*)?(?:Covers|Derived):").unwrap())
}

fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^#{1,6}\s+(.*?)\s*$").unwrap())
}

fn fence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*(?:```|~~~)").unwrap())
}

fn is_markdown(f: &ScanFile) -> bool {
    matches!(
        f.path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("md") | Some("markdown")
    )
}

pub fn scan_files(files: &[ScanFile]) -> Result<ScanOutput> {
    let mut out = ScanOutput::default();
    for f in files {
        scan_one(f, &mut out).with_context(|| format!("scanning {}", f.path.display()))?;
    }
    Ok(out)
}

fn scan_one(f: &ScanFile, out: &mut ScanOutput) -> Result<()> {
    let bytes = std::fs::read(&f.path)?;
    let text = String::from_utf8_lossy(&bytes);
    let md = is_markdown(f);

    let mut in_fence = false;
    let mut section: Option<String> = None;
    let mut record = SectionRecord {
        loc: Location {
            file: f.display.clone(),
            section: None,
            line: 1,
        },
        has_content: false,
        has_annotation: false,
    };

    for (idx, line) in text.lines().enumerate() {
        let lineno = idx + 1;

        // Fenced lines invisible to annotation *and* heading scanning.
        // Covers: ann~fences~1
        if md && fence_re().is_match(line) {
            in_fence = !in_fence;
            continue;
        }
        if md && in_fence {
            continue;
        }

        let ann = plain_re()
            .captures(line)
            .or_else(|| html_re().captures(line));
        if let Some(caps) = ann {
            let kind = if &caps[1] == "Covers" {
                LinkKind::Covers
            } else {
                LinkKind::Derived
            };
            for raw in caps[2].split(',') {
                // The grammar guarantees each piece parses.
                if let Some(id) = ReqId::parse(raw.trim()) {
                    out.links.push(Link {
                        kind,
                        id,
                        loc: Location {
                            file: f.display.clone(),
                            section: section.clone(),
                            line: lineno,
                        },
                    });
                }
            }
            record.has_annotation = true;
            continue;
        }

        if trigger_re().is_match(line) {
            out.malformed.push(Malformed {
                loc: Location {
                    file: f.display.clone(),
                    section: section.clone(),
                    line: lineno,
                },
            });
            record.has_content = true;
            continue;
        }

        // Headings tracked only in Markdown; annotation grammar
        // already consumed the line above, so an annotation is never a heading.
        // Covers: ann~headings~1
        if md {
            if let Some(caps) = heading_re().captures(line) {
                out.sections.push(record);
                let title = caps[1].trim_end_matches('#').trim_end().to_string();
                section = Some(title.clone());
                record = SectionRecord {
                    loc: Location {
                        file: f.display.clone(),
                        section: Some(title),
                        line: lineno,
                    },
                    has_content: false,
                    has_annotation: false,
                };
                continue;
            }
        }

        if !line.trim().is_empty() {
            record.has_content = true;
        }
    }

    out.sections.push(record);
    Ok(())
}
