//! Minimal GitHub REST helper: download the monthly index snapshot from a
//! public release asset. No auth needed (the repository is public); the
//! release is created by `publish-snapshot.ps1`.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub struct GitHub {
    http: reqwest::blocking::Client,
}

impl GitHub {
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent("rada-client")
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .expect("client");
        Self { http }
    }

    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .map_err(|e| anyhow!("github request: {e}"))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(anyhow!("github HTTP {status} for {url}"));
        }
        resp.json().map_err(|e| anyhow!("github JSON: {e}"))
    }

    /// Download URL of a snapshot asset from the latest release.
    ///
    /// `name`: exact asset name, or `None` → the first `*.sqlite.gz`
    /// asset of the latest release.
    pub fn latest_snapshot_url(
        &self,
        owner: &str,
        repo: &str,
        name: Option<&str>,
    ) -> Result<String> {
        let base = format!("https://api.github.com/repos/{owner}/{repo}");
        let pick = |assets: &[Asset]| -> Result<String> {
            match name {
                Some(n) => assets
                    .iter()
                    .find(|a| a.name == n)
                    .map(|a| a.browser_download_url.clone())
                    .ok_or_else(|| anyhow!("asset {n} not found in the release")),
                None => assets
                    .iter()
                    .find(|a| a.name.ends_with(".sqlite.gz"))
                    .map(|a| a.browser_download_url.clone())
                    .ok_or_else(|| anyhow!("no *.sqlite.gz asset in the release")),
            }
        };

        // The `latest` release is often an *app* release (no *.sqlite.gz
        // asset) because app releases postdate snapshot releases. Walk the
        // release list (newest first) and pick the newest one that carries
        // the requested asset.
        let rels: Vec<Release> = self
            .get_json(&format!("{base}/releases?per_page=100"))
            .unwrap_or_default();
        for rel in &rels {
            if let Ok(url) = pick(&rel.assets) {
                return Ok(url);
            }
        }
        if rels.is_empty() {
            // Fresh repo / unreachable API — the `latest` endpoint tells the
            // user more clearly what is missing.
            if let Ok(rel) = self.get_json::<Release>(&format!("{base}/releases/latest")) {
                return pick(&rel.assets);
            }
        }
        Err(anyhow!(
            "no *.sqlite.gz snapshot asset found in {owner}/{repo} releases \
             (run the snapshot publish script first)"
        ))
    }

    /// Download `url` to `out`; returns the file size in bytes.
    pub fn download(&self, url: &str, out: impl AsRef<Path>) -> Result<u64> {
        self.download_with_progress(url, out, |_| {})
    }

    /// Download `url` to `out`, calling `on_tick(bytes_so_far)` as chunks
    /// arrive (used by the GUI progress indicator). Streaming avoids the
    /// giant in-memory buffer the plain download uses.
    pub fn download_with_progress(
        &self,
        url: &str,
        out: impl AsRef<Path>,
        mut on_tick: impl FnMut(u64),
    ) -> Result<u64> {
        let out = out.as_ref();
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create dir {}", parent.display()))?;
            }
        }
        let mut resp = self
            .http
            .get(url)
            .send()
            .map_err(|e| anyhow!("download: {e}"))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(anyhow!("download HTTP {status} for {url}"));
        }
        let mut file = std::fs::File::create(out)
            .with_context(|| format!("create {}", out.display()))?;
        let mut total: u64 = 0;
        let mut buf = [0u8; 65536];
        loop {
            match std::io::Read::read(&mut resp, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    file.write_all(&buf[..n])
                        .with_context(|| format!("write {}", out.display()))?;
                    total += n as u64;
                    on_tick(total);
                }
                Err(e) => return Err(anyhow!("stream: {e}")),
            }
        }
        file.flush().ok();
        Ok(total)
    }
}

impl Default for GitHub {
    fn default() -> Self {
        Self::new()
    }
}
