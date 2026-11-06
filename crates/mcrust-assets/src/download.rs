use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use indicatif::ProgressBar;
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};

/// One file to fetch. Hashes are verified when present.
#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

impl DownloadJob {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            sha1: None,
            size: None,
        }
    }

    pub fn with_hash(mut self, sha1: Option<String>, size: Option<u64>) -> Self {
        self.sha1 = sha1;
        self.size = size;
        self
    }
}

/// Small, retrying, hash-verifying HTTP client for Mojang's servers.
pub struct Downloader {
    agent: ureq::Agent,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new().expect("failed to construct the HTTP client")
    }
}

impl Downloader {
    pub fn new() -> Result<Self> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(concat!("mcrust/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Ok(Self { agent })
    }

    pub fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let mut last_error = None;
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(400 * attempt as u64));
            }
            match self.agent.get(url).call() {
                Ok(mut response) => match response.body_mut().read_to_vec() {
                    Ok(bytes) => return Ok(bytes),
                    Err(error) => last_error = Some(anyhow::Error::new(error)),
                },
                Err(ureq::Error::StatusCode(404)) => {
                    bail!("{url} returned HTTP 404");
                }
                Err(error) => last_error = Some(anyhow::Error::new(error)),
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("failed to download {url}")))
            .with_context(|| format!("downloading {url}"))
    }

    /// Fetches JSON, using `cache` when it exists and parses successfully.
    pub fn fetch_json<T: DeserializeOwned>(
        &self,
        url: &str,
        cache: &Path,
        force: bool,
    ) -> Result<T> {
        if !force && cache.exists() {
            let bytes = fs::read(cache)
                .with_context(|| format!("reading cached metadata {}", cache.display()))?;
            if let Ok(value) = serde_json::from_slice(&bytes) {
                return Ok(value);
            }
            tracing::warn!("cached {} is invalid, refetching", cache.display());
        }

        let bytes = self.fetch_bytes(url)?;
        write_atomic(cache, &bytes)?;
        serde_json::from_slice(&bytes).with_context(|| format!("parsing JSON from {url}"))
    }

    /// Downloads a single file, verifying its SHA-1. Existing valid files are reused.
    pub fn download(
        &self,
        job: &DownloadJob,
        force: bool,
        progress: Option<&ProgressBar>,
    ) -> Result<()> {
        if !force && job.dest.exists() {
            if let Some(expected) = &job.sha1 {
                if sha1_file(&job.dest).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected)) {
                    if let Some(size) = job.size
                        && let Some(pb) = progress
                    {
                        pb.inc(size);
                    }
                    return Ok(());
                }
                tracing::warn!("hash mismatch for {}, redownloading", job.dest.display());
            } else {
                return Ok(());
            }
        }

        if let Some(parent) = job.dest.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }

        let mut last_error = None;
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(500 * attempt as u64));
            }
            match self.download_once(job, progress) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    tracing::warn!("download attempt {} failed: {error:#}", attempt + 1);
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("download failed")))
    }

    fn download_once(&self, job: &DownloadJob, progress: Option<&ProgressBar>) -> Result<()> {
        let partial = partial_path(&job.dest);
        let mut response = self
            .agent
            .get(&job.url)
            .call()
            .with_context(|| format!("requesting {}", job.url))?;

        let mut reader = response.body_mut().as_reader();
        let file =
            File::create(&partial).with_context(|| format!("creating {}", partial.display()))?;
        let mut writer = BufWriter::with_capacity(256 * 1024, file);
        let mut hasher = Sha1::new();
        let mut buffer = [0u8; 128 * 1024];

        loop {
            let read = reader
                .read(&mut buffer)
                .with_context(|| format!("reading {}", job.url))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            writer
                .write_all(&buffer[..read])
                .with_context(|| format!("writing {}", partial.display()))?;
            if let Some(pb) = progress {
                pb.inc(read as u64);
            }
        }
        writer.flush()?;
        drop(writer);

        let actual = hex(hasher.finalize().as_slice());
        if let Some(expected) = &job.sha1
            && !actual.eq_ignore_ascii_case(expected)
        {
            let _ = fs::remove_file(&partial);
            bail!(
                "SHA-1 mismatch for {} (expected {expected}, got {actual})",
                job.url
            );
        }

        if job.dest.exists() {
            fs::remove_file(&job.dest).ok();
        }
        fs::rename(&partial, &job.dest)
            .with_context(|| format!("moving {} into place", job.dest.display()))?;
        Ok(())
    }

    /// Downloads many files on up to `threads` worker threads.
    pub fn download_many(
        &self,
        jobs: &[DownloadJob],
        threads: usize,
        force: bool,
        progress: Option<&ProgressBar>,
    ) -> Result<()> {
        if jobs.is_empty() {
            return Ok(());
        }
        let next = AtomicUsize::new(0);
        let failure: Mutex<Option<anyhow::Error>> = Mutex::new(None);
        let workers = threads.max(1).min(jobs.len());

        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        if failure.lock().expect("download failure lock").is_some() {
                            return;
                        }
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(index) else {
                            return;
                        };
                        if let Err(error) = self.download(job, force, progress) {
                            let mut slot = failure.lock().expect("download failure lock");
                            if slot.is_none() {
                                *slot = Some(error);
                            }
                            return;
                        }
                    }
                });
            }
        });

        if let Some(error) = failure.into_inner().expect("download failure lock") {
            return Err(error);
        }
        Ok(())
    }
}

fn partial_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    dest.with_file_name(format!("{name}.part"))
}

fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = partial_path(dest);
    fs::write(&partial, bytes).with_context(|| format!("writing {}", partial.display()))?;
    if dest.exists() {
        fs::remove_file(dest).ok();
    }
    fs::rename(&partial, dest).with_context(|| format!("moving {}", dest.display()))?;
    Ok(())
}

pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(hasher.finalize().as_slice()))
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hashes_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"hello world").unwrap();
        drop(file);
        assert_eq!(
            sha1_file(&path).unwrap(),
            "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed"
        );
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }
}
