//! Core types and the `data.rada.gov.ua` API client for the legislation
//! database "Законодавство України" (zakon.rada.gov.ua).
//!
//! Verified API facts (live probes, October 2026):
//! - `User-Agent` must be exactly `OpenData` for anonymous JSON/TXT access.
//!   Anything else (browser UA, `OpenData/1.0`, lowercase) → 403.
//! - `data.rada.gov.ua` is HTTPS-only (port 80 → 301).
//! - Token endpoint `/api/token` returns a daily UUID token, but using it in
//!   requests requires prior IP registration, so we stick to `OpenData`.
//! - Limits: ≤60 req/min (5–7 s pause recommended), ≤100 000 req/day,
//!   ≤200 MB/day. Never poll `/api/token` or `/api/limits` per request.
//! - `doc.txt` dump and dictionary files are Windows-1251; all `/laws/*`
//!   HTML/JSON responses are UTF-8.

pub mod api;
pub mod github;
pub mod models;

pub use api::RadaClient;
pub use github::GitHub;
pub use models::*;

/// Base URL of the open-data API host.
pub const DATA_BASE: &str = "https://data.rada.gov.ua";

/// Base URL of the public site (used for human links in the UI).
pub const ZAKON_BASE: &str = "https://zakon.rada.gov.ua";

/// The only anonymous `User-Agent` accepted for JSON/TXT endpoints.
pub const UA_OPENDATA: &str = "OpenData";

/// Full path to the official API documentation mirror used during research.
pub const API_DOCS_REFERENCE: &str =
    "https://data.rada.gov.ua/open/main/api/page3";
