//! Embedded single-page UI: one HTML file, inline CSS/JS, zero external
//! assets (portable-exe promise: nothing is fetched from the network).

pub const INDEX_HTML: &str = include_str!("ui/index.html");
