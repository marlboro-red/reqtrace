//! Human and JSON report rendering.

use crate::checks::{Finding, Severity, Summary};
use serde::Serialize;

/// Findings sorted by check, then ID, then file, then line.
///
/// Covers: cli~determinism~2
pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        let ka = (
            a.check,
            a.id.as_deref().unwrap_or(""),
            a.location.as_ref().map(|l| l.file.as_str()).unwrap_or(""),
            a.location.as_ref().map(|l| l.line).unwrap_or(0),
        );
        let kb = (
            b.check,
            b.id.as_deref().unwrap_or(""),
            b.location.as_ref().map(|l| l.file.as_str()).unwrap_or(""),
            b.location.as_ref().map(|l| l.line).unwrap_or(0),
        );
        ka.cmp(&kb)
    });
}

/// One line per finding, worst first, then a one-line summary.
pub fn human(findings: &[Finding], summary: &Summary) -> String {
    let mut out = String::new();
    for severity in [Severity::Fail, Severity::Warn] {
        for f in findings.iter().filter(|f| f.severity == severity) {
            render_finding(&mut out, f);
        }
    }
    let total = summary.fail + summary.warn;
    out.push_str(&format!(
        "{} finding{} ({} fail, {} warn) · {}/{} covered · {} exception{}\n",
        total,
        plural(total),
        summary.fail,
        summary.warn,
        summary.covered,
        summary.denominator,
        summary.exceptions,
        plural(summary.exceptions),
    ));
    out
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn render_finding(out: &mut String, f: &Finding) {
    let sev = match f.severity {
        Severity::Fail => "FAIL",
        Severity::Warn => "WARN",
    };
    let id = f.id.as_deref().unwrap_or("");
    let detail = match f.check {
        "uncovered" => {
            let mut parts = Vec::new();
            if let Some(r) = &f.rating {
                parts.push(format!("[{}]", r.join(",")));
            }
            if let Some(o) = &f.owner {
                parts.push(format!("owner:{}", o));
            }
            parts.join(" ")
        }
        "stale" => format!(
            "covered rev {}, current rev {}",
            f.covered_rev.unwrap_or(0),
            f.current_rev.unwrap_or(0)
        ),
        "orphaned" => "not in inventory at any rev".to_string(),
        "stale-exception" => "exception target not in inventory".to_string(),
        "malformed-annotation" => "line fails the annotation grammar".to_string(),
        "unparented" => "section has content but no Covers:/Derived:".to_string(),
        _ => String::new(),
    };

    let line = format!("{} {:<10} {:<22} {}", sev, f.check, id, detail);
    out.push_str(line.trim_end());
    out.push('\n');
    if let Some(loc) = &f.location {
        match &loc.section {
            Some(s) => out.push_str(&format!("                → {} § \"{}\"\n", loc.file, s)),
            None => out.push_str(&format!("                → {}\n", loc.file)),
        }
    }
}

// Covers: out~json-stable~1
#[derive(Serialize)]
struct JsonReport<'a> {
    version: u32,
    summary: JsonSummary,
    findings: Vec<JsonFinding<'a>>,
}

// Covers: out~summary~1
#[derive(Serialize)]
struct JsonSummary {
    denominator: usize,
    covered: usize,
    exceptions: usize,
    findings: JsonCounts,
}

#[derive(Serialize)]
struct JsonCounts {
    fail: usize,
    warn: usize,
}

#[derive(Serialize)]
struct JsonFinding<'a> {
    check: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rating: Option<&'a Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    covered_rev: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_rev: Option<u32>,
    severity: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<JsonLocation<'a>>,
}

#[derive(Serialize)]
struct JsonLocation<'a> {
    file: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    section: Option<&'a str>,
    line: usize,
}

pub fn json(findings: &[Finding], summary: &Summary) -> String {
    let report = JsonReport {
        version: 1,
        summary: JsonSummary {
            denominator: summary.denominator,
            covered: summary.covered,
            exceptions: summary.exceptions,
            findings: JsonCounts {
                fail: summary.fail,
                warn: summary.warn,
            },
        },
        findings: findings
            .iter()
            .map(|f| JsonFinding {
                check: f.check,
                id: f.id.as_deref(),
                rating: f.rating.as_ref(),
                owner: f.owner.as_deref(),
                covered_rev: f.covered_rev,
                current_rev: f.current_rev,
                severity: f.severity.as_str(),
                location: f.location.as_ref().map(|l| JsonLocation {
                    file: &l.file,
                    section: l.section.as_deref(),
                    line: l.line,
                }),
            })
            .collect(),
    };
    let mut s = serde_json::to_string_pretty(&report).expect("report serializes");
    s.push('\n');
    s
}
