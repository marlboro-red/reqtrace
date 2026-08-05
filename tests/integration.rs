//! Integration tests against the spec's normative requirements and examples.

use reqtrace::checks::{self, Severity};
use reqtrace::config::{Config, Policy};
use reqtrace::exceptions;
use reqtrace::id::ReqId;
use reqtrace::inventory::{Inventory, Status};
use reqtrace::report;
use reqtrace::scan::{self, LinkKind, ScanFile};
use std::path::Path;

fn write(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
    path
}

fn scan_file(path: std::path::PathBuf) -> ScanFile {
    let display = path.file_name().unwrap().to_string_lossy().to_string();
    ScanFile { path, display }
}

// Covers: inv~id-grammar~2
#[test]
fn id_grammar() {
    for ok in [
        "req~login-throttling~2",
        "cli~exit-codes~1",
        "a~b~1",
        "dsn~retry-queue~10",
    ] {
        assert!(ReqId::parse(ok).is_some(), "should accept {ok}");
    }
    for bad in [
        "Req~x~1",    // uppercase type
        "req~x~0",    // rev 0
        "req~x~01",   // leading zero
        "req~-x~1",   // leading hyphen
        "req~x-~1",   // trailing hyphen
        "req~x--y~1", // consecutive hyphens
        "req~x",      // missing rev
        "req~x~1~2",  // extra segment
    ] {
        assert!(ReqId::parse(bad).is_none(), "should reject {bad}");
    }
}

// Covers: inv~current-rev~1, inv~coverage-denominator~2
#[test]
fn current_rev_and_denominator() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        r#"
- id: req~a~1
  status: confirmed
- id: req~a~2
  status: disputed
- id: req~b~1
  status: assumed
- id: req~c~1
  status: retired
"#,
    );
    let inv = Inventory::load(&inv_path).unwrap();
    // Highest rev wins; its status alone decides membership.
    assert_eq!(inv.current_rev("req~a"), Some(2));
    assert_eq!(inv.current_item("req~a").unwrap().status, Status::Disputed);
    let denom = inv.denominator();
    assert!(!denom.contains("req~a"), "current rev disputed → out");
    assert!(denom.contains("req~b"));
    assert!(!denom.contains("req~c"));
}

// Covers: inv~dup-ids~2
#[test]
fn duplicate_rev_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        "- id: req~a~1\n  status: confirmed\n- id: req~a~1\n  status: assumed\n",
    );
    assert!(Inventory::load(&inv_path).is_err());
}

#[test]
fn invalid_status_and_id_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let bad_status = write(dir.path(), "s.yaml", "- id: req~a~1\n  status: shipped\n");
    assert!(Inventory::load(&bad_status).is_err());
    let bad_id = write(dir.path(), "i.yaml", "- id: REQ~a~1\n  status: confirmed\n");
    let err = Inventory::load(&bad_id).unwrap_err().to_string();
    assert!(
        err.contains("REQ~a~1"),
        "error names the offending ID: {err}"
    );
    assert!(err.contains("i.yaml:1"), "error carries the line: {err}");
}

// Covers: ann~grammar~2, ann~fences~1, ann~headings~1, ann~malformed~2
#[test]
fn scanner_forms_fences_and_attribution() {
    let dir = tempfile::tempdir().unwrap();
    let content = [
        "Preamble covers nothing.",
        "",
        "# Design",
        "",
        "Covers: req~a~2, req~b~1",
        "",
        "```",
        "Covers: req~fake~9",
        "# Not A Heading",
        "```",
        "",
        "<!-- Covers: req~c~1 -->",
        "// Covers: req~d~1",
        "/// Derived: dsn~e~1",
        "- Covers: req~f~1",
        "# Covers: notanid",
    ]
    .join("\n");
    let path = write(dir.path(), "doc.md", &content);
    let out = scan::scan_files(&[scan_file(path)]).unwrap();

    let ids: Vec<String> = out.links.iter().map(|l| l.id.to_string()).collect();
    assert_eq!(
        ids,
        ["req~a~2", "req~b~1", "req~c~1", "req~d~1", "dsn~e~1", "req~f~1"],
        "fenced Covers ignored, all comment forms recognized"
    );
    assert!(out
        .links
        .iter()
        .all(|l| l.loc.section.as_deref() == Some("Design")));
    assert_eq!(out.links[4].kind, LinkKind::Derived);
    assert_eq!(out.malformed.len(), 1, "`# Covers: notanid` is malformed");
    assert_eq!(out.malformed[0].loc.line, 16);
    // The fenced pseudo-heading must not have opened a section.
    assert!(out
        .sections
        .iter()
        .all(|s| s.loc.section.as_deref() != Some("Not A Heading")));
}

#[test]
fn unbalanced_html_comment_is_malformed() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "doc.md", "<!-- Covers: req~a~1\n");
    let out = scan::scan_files(&[scan_file(path)]).unwrap();
    assert!(out.links.is_empty());
    assert_eq!(out.malformed.len(), 1);
}

// Non-Markdown files: no heading tracking, section = file.
// Covers: ann~headings~1
#[test]
fn code_comments_are_not_headings() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "job.py",
        "# just a python comment\nx = 1\n# Covers: req~a~1\n",
    );
    let out = scan::scan_files(&[scan_file(path)]).unwrap();
    assert_eq!(out.links.len(), 1);
    assert_eq!(out.links[0].loc.section, None);
    assert!(
        out.malformed.is_empty(),
        "plain comments aren't malformed annotations"
    );
}

/// The spec §6 scenario: one uncovered H item (fail), one stale link (warn),
/// one applied exception. Verifies the summary invariant
/// denominator = covered + exceptions + uncovered findings.
#[test]
fn end_to_end_spec_example() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        r#"
- id: req~lockout-notice~1
  status: confirmed
  rating: [H, M]
  owner: pm-jane
- id: req~login-throttling~2
  status: confirmed
- id: req~mobile-push~1
  status: confirmed
"#,
    );
    let doc = write(
        dir.path(),
        "design.md",
        "# Rate limiting design\n\nCovers: req~login-throttling~1\n",
    );
    let exc_path = write(
        dir.path(),
        "not-in-scope.yaml",
        "- id: req~mobile-push~1\n  justification: \"Mobile repo's responsibility\"\n",
    );

    let inv = Inventory::load(&inv_path).unwrap();
    let exc = exceptions::load(&exc_path).unwrap();
    let scanned = scan::scan_files(&[scan_file(doc)]).unwrap();
    let cfg = Config::defaults();
    let (mut findings, summary) = checks::run(&inv, &scanned, &exc, &cfg);
    report::sort_findings(&mut findings);

    assert_eq!(summary.denominator, 3);
    assert_eq!(summary.covered, 1, "stale-but-linked counts as covered");
    assert_eq!(summary.exceptions, 1);
    assert_eq!(
        summary.denominator,
        summary.covered
            + summary.exceptions
            + findings.iter().filter(|f| f.check == "uncovered").count(),
        "out~summary~1 invariant"
    );

    assert_eq!(findings.len(), 2);
    let stale = findings.iter().find(|f| f.check == "stale").unwrap();
    assert_eq!(
        stale.severity,
        Severity::Warn,
        "unrated → warn under by-rating"
    );
    assert_eq!(stale.id.as_deref(), Some("req~login-throttling~2"));
    assert_eq!(stale.covered_rev, Some(1));
    assert_eq!(stale.current_rev, Some(2));
    assert_eq!(
        stale.location.as_ref().unwrap().section.as_deref(),
        Some("Rate limiting design")
    );

    let uncovered = findings.iter().find(|f| f.check == "uncovered").unwrap();
    assert_eq!(
        uncovered.severity,
        Severity::Fail,
        "H rating → fail under by-rating"
    );
    assert_eq!(uncovered.id.as_deref(), Some("req~lockout-notice~1"));
    assert_eq!(uncovered.owner.as_deref(), Some("pm-jane"));

    // Rendering twice is byte-identical.
    // Covers: cli~determinism~2
    let json1 = report::json(&findings, &summary);
    let json2 = report::json(&findings, &summary);
    assert_eq!(json1, json2);
    assert!(json1.contains("\"version\": 1"));

    let human = report::human(&findings, &summary);
    assert!(
        human.contains("FAIL uncovered  req~lockout-notice~1   [H,M] owner:pm-jane"),
        "{human}"
    );
    assert!(
        human.contains("2 findings (1 fail, 1 warn) · 1/3 covered · 1 exception"),
        "{human}"
    );
    let fail_pos = human.find("FAIL").unwrap();
    let warn_pos = human.find("WARN").unwrap();
    assert!(fail_pos < warn_pos, "worst first");
}

// Derived targets exempt from orphaned.
// Covers: ann~derived~1
#[test]
fn derived_is_not_orphaned() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        "- id: req~a~1\n  status: confirmed\n",
    );
    let doc = write(
        dir.path(),
        "d.md",
        "Covers: req~a~1\nDerived: dsn~new-thing~1\n",
    );
    let inv = Inventory::load(&inv_path).unwrap();
    let scanned = scan::scan_files(&[scan_file(doc)]).unwrap();
    let (findings, _) = checks::run(&inv, &scanned, &[], &Config::defaults());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn orphaned_and_stale_exception() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        "- id: req~a~1\n  status: confirmed\n",
    );
    let doc = write(dir.path(), "d.md", "Covers: req~a~1, req~ghost~1\n");
    let exc_path = write(
        dir.path(),
        "exc.yaml",
        "- id: req~gone~1\n  justification: obsolete\n",
    );
    let inv = Inventory::load(&inv_path).unwrap();
    let exc = exceptions::load(&exc_path).unwrap();
    let scanned = scan::scan_files(&[scan_file(doc)]).unwrap();
    let (findings, summary) = checks::run(&inv, &scanned, &exc, &Config::defaults());

    let orphaned = findings.iter().find(|f| f.check == "orphaned").unwrap();
    assert_eq!(orphaned.severity, Severity::Fail);
    assert_eq!(orphaned.id.as_deref(), Some("req~ghost~1"));

    let stale_exc = findings
        .iter()
        .find(|f| f.check == "stale-exception")
        .unwrap();
    assert_eq!(stale_exc.severity, Severity::Warn);
    assert_eq!(summary.exceptions, 0, "unapplied exception not counted");
}

// Exception matches on type~slug across revs.
// Covers: exc~matching~1
#[test]
fn exception_survives_revving() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        "- id: req~push~2\n  status: confirmed\n",
    );
    let exc_path = write(
        dir.path(),
        "exc.yaml",
        "- id: req~push~1\n  justification: elsewhere\n",
    );
    let inv = Inventory::load(&inv_path).unwrap();
    let exc = exceptions::load(&exc_path).unwrap();
    let scanned = scan::scan_files(&[]).unwrap();
    let (findings, summary) = checks::run(&inv, &scanned, &exc, &Config::defaults());
    assert!(
        findings.is_empty(),
        "old-rev exception still suppresses: {findings:?}"
    );
    assert_eq!(summary.exceptions, 1);
}

// Covers: exc~justification~1
#[test]
fn exception_without_justification_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let exc_path = write(
        dir.path(),
        "exc.yaml",
        "- id: req~a~1\n  justification: \"  \"\n",
    );
    assert!(exceptions::load(&exc_path).is_err());
}

// Covers: chk~severity-config~1
#[test]
fn severity_config_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        "- id: req~a~1\n  status: confirmed\n  rating: [H]\n",
    );
    let inv = Inventory::load(&inv_path).unwrap();
    let scanned = scan::scan_files(&[]).unwrap();

    let mut cfg = Config::defaults();
    cfg.severity.insert("uncovered".into(), Policy::Off);
    let (findings, summary) = checks::run(&inv, &scanned, &[], &cfg);
    assert!(findings.is_empty());
    assert_eq!(summary.fail, 0);

    let cfg2 = Config::defaults();
    let (findings2, _) = checks::run(&inv, &scanned, &[], &cfg2);
    assert_eq!(findings2[0].severity, Severity::Fail, "H → fail by default");
}

#[test]
fn config_file_parsing_and_unknown_check() {
    let dir = tempfile::tempdir().unwrap();
    let good = write(
        dir.path(),
        ".reqtrace.toml",
        "[scan]\nglobs = [\"docs/**/*.md\"]\nexceptions = \"exc.yaml\"\n\n[severity]\nuncovered = \"warn\"\nmalformed-annotation = \"off\"\n",
    );
    let cfg = reqtrace::config::load(&good).unwrap();
    assert_eq!(cfg.policy("uncovered"), Policy::Warn);
    assert_eq!(cfg.policy("malformed-annotation"), Policy::Off);
    assert_eq!(
        cfg.policy("orphaned"),
        Policy::Fail,
        "untouched defaults survive"
    );
    assert_eq!(
        cfg.exceptions.as_ref().unwrap(),
        &dir.path().join("exc.yaml")
    );

    let bad = write(
        dir.path(),
        "bad.toml",
        "[severity]\nbogus-check = \"fail\"\n",
    );
    assert!(reqtrace::config::load(&bad).is_err());
}

// Covers: cli~docpaths~1
#[test]
fn scan_set_resolution() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "docs/a.md", "hello\n");
    write(dir.path(), "docs/b.md", "world\n");
    write(dir.path(), "src/c.rs", "// code\n");
    write(dir.path(), ".hidden/d.md", "secret\n");

    // doc-paths replace globs entirely
    let mut cfg = Config::defaults();
    cfg.globs = vec!["src/**/*.rs".into()];
    let files =
        reqtrace::runner::resolve_scan_set(&[dir.path().join("docs")], &cfg, dir.path()).unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.display.as_str()).collect();
    assert_eq!(names, ["docs/a.md", "docs/b.md"]);

    // globs used when no doc-paths
    let files2 = reqtrace::runner::resolve_scan_set(&[], &cfg, dir.path()).unwrap();
    let names2: Vec<&str> = files2.iter().map(|f| f.display.as_str()).collect();
    assert_eq!(names2, ["src/c.rs"]);

    // neither → usage error
    let empty_cfg = Config::defaults();
    assert!(reqtrace::runner::resolve_scan_set(&[], &empty_cfg, dir.path()).is_err());
}

#[test]
fn unparented_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let inv_path = write(
        dir.path(),
        "inv.yaml",
        "- id: req~a~1\n  status: confirmed\n",
    );
    let doc = write(
        dir.path(),
        "d.md",
        "# Annotated\n\nCovers: req~a~1\n\n# Bare\n\nSome prose here.\n",
    );
    let inv = Inventory::load(&inv_path).unwrap();
    let scanned = scan::scan_files(&[scan_file(doc)]).unwrap();
    let mut cfg = Config::defaults();
    cfg.severity.insert("unparented".into(), Policy::Warn);
    let (findings, _) = checks::run(&inv, &scanned, &[], &cfg);
    let unparented: Vec<_> = findings
        .iter()
        .filter(|f| f.check == "unparented")
        .collect();
    assert_eq!(unparented.len(), 1);
    assert_eq!(
        unparented[0].location.as_ref().unwrap().section.as_deref(),
        Some("Bare")
    );
}
