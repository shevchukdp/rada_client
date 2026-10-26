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

        // `releases/latest` is a redirect to the most recent release.
        if let Ok(rel) = self.get_json::<Release>(&format!("{base}/releases/latest")) {
            return pick(&rel.assets);
        }
        // No releases at all (fresh repo) — report it clearly.
        Err(anyhow!(
            "no releases at {owner}/{repo} yet (run the snapshot publish script first)"
        ))
    }

    /// Download `url` to `out`; returns the file size in bytes.
    pub fn download(&self, url: &str, out: impl AsRef<Path>) -> Result<u64> {
        let out = out.as_ref();
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create dir {}", parent.display()))?;
            }
        }
        let resp = self
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
        let bytes = resp.bytes().map_err(|e| anyhow!("stream: {e}"))?;
        file.write_all(&bytes)?;
        Ok(bytes.len() as u64)
    }
}

impl Default for GitHub {
    fn default() -> Self {
        Self::new()
    }
}
