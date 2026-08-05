//! Annotations work in code comments too — the example config scans
//! `src/**/*.rs` alongside the design docs.

/// Adds `RateLimit-Remaining` / `RateLimit-Reset` headers to throttled
/// responses.
///
/// Covers: req~rate-limit-headers~1
pub fn rate_limit_headers(remaining: u32, reset_secs: u64) -> Vec<(String, String)> {
    vec![
        ("RateLimit-Remaining".into(), remaining.to_string()),
        ("RateLimit-Reset".into(), reset_secs.to_string()),
    ]
}
