//! The coverage checks: findings and summary arithmetic.

use crate::config::{Config, Policy};
use crate::exceptions::ExceptionEntry;
use crate::inventory::{Inventory, Rating};
use crate::scan::{LinkKind, Location, ScanOutput};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Fail,
    Warn,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Fail => "fail",
            Severity::Warn => "warn",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub check: &'static str,
    pub id: Option<String>,
    pub rating: Option<Vec<String>>,
    pub owner: Option<String>,
    pub covered_rev: Option<u32>,
    pub current_rev: Option<u32>,
    pub severity: Severity,
    pub location: Option<Location>,
}

impl Finding {
    fn new(check: &'static str, severity: Severity) -> Self {
        Finding {
            check,
            id: None,
            rating: None,
            owner: None,
            covered_rev: None,
            current_rev: None,
            severity,
            location: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub denominator: usize,
    pub covered: usize,
    pub exceptions: usize,
    pub fail: usize,
    pub warn: usize,
}

/// by-rating: H → fail; M, L, or unrated → warn. Off suppresses the finding.
fn resolve(policy: Policy, rating_value: Option<Rating>) -> Option<Severity> {
    match policy {
        Policy::Fail => Some(Severity::Fail),
        Policy::Warn => Some(Severity::Warn),
        Policy::Off => None,
        Policy::ByRating => Some(if rating_value == Some(Rating::H) {
            Severity::Fail
        } else {
            Severity::Warn
        }),
    }
}

pub fn run(
    inv: &Inventory,
    scan: &ScanOutput,
    exceptions: &[ExceptionEntry],
    cfg: &Config,
) -> (Vec<Finding>, Summary) {
    let denominator = inv.denominator();

    // Derived targets are exempt from orphaned/stale.
    // Covers: ann~derived~1
    let covers: Vec<_> = scan
        .links
        .iter()
        .filter(|l| l.kind == LinkKind::Covers)
        .collect();

    // Coverage matches on type~slug,
    // any rev; a slug covered only by a stale link is covered.
    // Covers: chk~matching~1, out~summary~1
    let covered_slugs: BTreeSet<String> = covers
        .iter()
        .map(|l| l.id.slug_key())
        .filter(|s| denominator.contains(s))
        .collect();

    // Exceptions match on type~slug, rev ignored.
    // Covers: exc~matching~1
    let exception_slugs: BTreeSet<String> = exceptions.iter().map(|e| e.id.slug_key()).collect();
    let applied_exceptions = exception_slugs.intersection(&denominator).count();

    let mut findings: Vec<Finding> = Vec::new();

    // Covers: chk~uncovered~1
    for slug in &denominator {
        if covered_slugs.contains(slug) || exception_slugs.contains(slug) {
            continue;
        }
        let item = inv
            .current_item(slug)
            .expect("denominator slug has current item");
        if let Some(sev) = resolve(cfg.policy("uncovered"), item.rating_value()) {
            let mut f = Finding::new("uncovered", sev);
            f.id = Some(item.id.to_string());
            f.rating = item.rating_strings();
            f.owner = item.owner.clone();
            findings.push(f);
        }
    }

    for link in &covers {
        let key = link.id.slug_key();
        match inv.current_rev(&key) {
            None => {
                // Link target's type~slug not in inventory at any rev.
                if let Some(sev) = resolve(cfg.policy("orphaned"), None) {
                    let mut f = Finding::new("orphaned", sev);
                    f.id = Some(link.id.to_string());
                    f.location = Some(link.loc.clone());
                    findings.push(f);
                }
            }
            Some(current) if link.id.rev < current => {
                // Covers: chk~stale~1
                let item = inv.current_item(&key).expect("current rev implies item");
                if let Some(sev) = resolve(cfg.policy("stale"), item.rating_value()) {
                    let mut f = Finding::new("stale", sev);
                    f.id = Some(item.id.to_string());
                    f.covered_rev = Some(link.id.rev);
                    f.current_rev = Some(current);
                    f.location = Some(link.loc.clone());
                    findings.push(f);
                }
            }
            Some(_) => {}
        }
    }

    for exc in exceptions {
        if !inv.has_slug(&exc.id.slug_key()) {
            if let Some(sev) = resolve(cfg.policy("stale-exception"), None) {
                let mut f = Finding::new("stale-exception", sev);
                f.id = Some(exc.id.to_string());
                findings.push(f);
            }
        }
    }

    // Covers: ann~malformed~2
    for m in &scan.malformed {
        if let Some(sev) = resolve(cfg.policy("malformed-annotation"), None) {
            let mut f = Finding::new("malformed-annotation", sev);
            f.location = Some(m.loc.clone());
            findings.push(f);
        }
    }

    if cfg.policy("unparented") != Policy::Off {
        for s in &scan.sections {
            if s.has_content && !s.has_annotation {
                if let Some(sev) = resolve(cfg.policy("unparented"), None) {
                    let mut f = Finding::new("unparented", sev);
                    f.location = Some(s.loc.clone());
                    findings.push(f);
                }
            }
        }
    }

    let fail = findings
        .iter()
        .filter(|f| f.severity == Severity::Fail)
        .count();
    let warn = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count();

    // Covers: out~summary~1
    let summary = Summary {
        denominator: denominator.len(),
        covered: covered_slugs.len(),
        exceptions: applied_exceptions,
        fail,
        warn,
    };

    (findings, summary)
}
