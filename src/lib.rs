//! `reqtrace` — deterministic requirements-coverage checker.
//!
//! Library crate; `main.rs` is a thin CLI shim over [`runner`].

pub mod checks;
pub mod config;
pub mod exceptions;
pub mod id;
pub mod inventory;
pub mod report;
pub mod runner;
pub mod scan;
