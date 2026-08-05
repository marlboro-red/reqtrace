//! Requirement ID grammar: `type~slug~rev`.
//!
//! Covers: inv~id-grammar~2

use regex::Regex;
use std::fmt;
use std::sync::OnceLock;

/// Bare ID pattern, unanchored, for embedding in larger regexes.
/// Slugs forbid leading, trailing, and consecutive hyphens.
pub const ID_PATTERN: &str = r"[a-z][a-z0-9]*~[a-z0-9]+(?:-[a-z0-9]+)*~[1-9][0-9]*";

fn full_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([a-z][a-z0-9]*)~([a-z0-9]+(?:-[a-z0-9]+)*)~([1-9][0-9]*)$").unwrap()
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReqId {
    pub ty: String,
    pub slug: String,
    pub rev: u32,
}

impl ReqId {
    pub fn parse(s: &str) -> Option<Self> {
        let caps = full_re().captures(s)?;
        let rev: u32 = caps[3].parse().ok()?;
        Some(ReqId {
            ty: caps[1].to_string(),
            slug: caps[2].to_string(),
            rev,
        })
    }

    /// `type~slug` — the identity coverage and exceptions match on.
    ///
    /// Covers: chk~matching~1
    pub fn slug_key(&self) -> String {
        format!("{}~{}", self.ty, self.slug)
    }
}

impl fmt::Display for ReqId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}~{}~{}", self.ty, self.slug, self.rev)
    }
}
