use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};

use crate::download::{DownloadJob, Downloader, sha1_file};
use crate::meta::{AssetIndex, AssetIndexInfo, Library, VersionInfo, VersionManifest};
use crate::paths::Paths;
use crate::rules::{native_classifier_for_arch, rules_allow};

/// How much of the asset store to mirror locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetMode {
    /// Only metadata (version JSON, client jar, libraries, resource pack).
    None,
    /// Metadata + the asset index, but no objects.
    Index,
    /// Everything, including sounds and language files (~500 MB).
    All,
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub version: Option<String>,
    pub assets: AssetMode,
    pub force: bool,
    pub extract_resources: bool,
    pub threads: usize,
    pub quiet: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            version: None,
            assets: AssetMode::Index,
            force: false,
            extract_resources: true,
            threads: 8,
            quiet: false,
        }
    }
}

/// A resolved Maven library.
#[derive(Debug, Clone)]
pub struct ResolvedLibrary {
    pub name: String,
    pub relative: String,
    pub path: PathBuf,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub native: bool,
}

/// Everything the client and launcher need for one installed version.
#[derive(Debug, Clone)]
pub struct InstalledVersion {
    pub id: String,
    pub version_json: PathBuf,
    pub client_jar: PathBuf,
    pub resources_dir: PathBuf,
    pub libraries: Vec<ResolvedLibrary>,
    pub natives_dir: PathBuf,
    pub asset_index: Option<PathBuf>,
    pub java_major: u32,
    pub main_class: String,
    pub info: VersionInfo,
    pub extracted_resources: usize,
}

impl InstalledVersion {
    pub fn load(paths: &Paths, id: &str) -> Result<Self> {
        let version_json = paths.version_json(id);
        if !version_json.exists() {
            bail!(
                "version {id} is not installed (missing {}). Run `mcrust setup` first.",
                version_json.display()
            );
        }
        let client_jar = paths.client_jar(id);
        if !client_jar.exists() {
            bail!("client.jar for {id} is missing. Run `mcrust setup --version {id}`.");
        }
        let bytes = fs::read(&version_json)?;
        let info: VersionInfo = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", version_json.display()))?;
        let libraries = resolve_libraries(&info, paths);
        Ok(Self {
            id: id.to_string(),
            version_json,
            client_jar,
            resources_dir: paths.resources_dir(id),
            libraries,
            natives_dir: paths.natives_java_dir(id),
            asset_index: info
                .asset_index
                .as_ref()
                .map(|index| paths.asset_index_path(&index.id)),
            java_major: info.required_java_major(),
            main_class: info.main_class.clone(),
            info,
            extracted_resources: 0,
        })
    }

    pub fn classpath(&self) -> String {
        let mut entries = Vec::with_capacity(self.libraries.len() + 1);
        entries.push(self.client_jar.clone());
        for library in &self.libraries {
            if !library.native {
                entries.push(library.path.clone());
            }
        }
        entries
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(if cfg!(windows) { ";" } else { ":" })
    }

    pub fn logging_config(&self, paths: &Paths) -> Option<PathBuf> {
        let client = self.info.logging.as_ref()?.client.as_ref()?;
        Some(
            paths
                .libraries_dir()
                .join("logging")
                .join(client.file.id.clone()?),
        )
    }
}

/// Installs (or repairs) a vanilla version. Idempotent and hash-verified.
pub fn install(paths: &Paths, options: &InstallOptions) -> Result<InstalledVersion> {
    let downloader = Downloader::new()?;
    let force = options.force;

    let manifest_path = paths.meta_dir().join("version_manifest_v2.json");
    let manifest: VersionManifest =
        downloader.fetch_json(crate::VERSION_MANIFEST_URL, &manifest_path, force)?;

    let requested = options
        .version
        .clone()
        .unwrap_or_else(|| manifest.latest.release.clone());
    let summary = manifest
        .versions
        .iter()
        .find(|candidate| candidate.id == requested)
        .with_context(|| format!("unknown Minecraft version {requested}"))?
        .clone();

    tracing::info!(version = %summary.id, kind = %summary.kind, "resolved version");

    let version_dir = paths.version_dir(&summary.id);
    fs::create_dir_all(&version_dir)?;
    let version_json = paths.version_json(&summary.id);
    let info: VersionInfo = downloader.fetch_json(&summary.url, &version_json, force)?;

    let client = info
        .client_download()
        .context("version metadata has no client download")?;
    let client_jar = paths.client_jar(&summary.id);

    if !options.quiet {
        let bar = progress_bar(client.size.unwrap_or(0), "client.jar");
        downloader
            .download(
                &DownloadJob::new(&client.url, &client_jar)
                    .with_hash(client.sha1.clone(), client.size),
                force,
                Some(&bar),
            )
            .with_context(|| format!("downloading client.jar for {}", summary.id))?;
        bar.finish_and_clear();
    } else {
        downloader.download(
            &DownloadJob::new(&client.url, &client_jar).with_hash(client.sha1.clone(), client.size),
            force,
            None,
        )?;
    }

    let libraries = resolve_libraries(&info, paths);
    let jobs: Vec<DownloadJob> = libraries
        .iter()
        .map(|library| {
            DownloadJob::new(&library.url, &library.path)
                .with_hash(library.sha1.clone(), library.size)
        })
        .collect();

    let total = libraries.iter().filter_map(|lib| lib.size).sum::<u64>();
    let bar = (!options.quiet).then(|| progress_bar(total, "libraries"));
    downloader
        .download_many(&jobs, options.threads, force, bar.as_ref())
        .context("downloading libraries")?;
    if let Some(bar) = bar {
        bar.finish_and_clear();
    }

    let natives_dir = paths.natives_java_dir(&summary.id);
    fs::create_dir_all(&natives_dir)?;
    for library in libraries.iter().filter(|lib| lib.native) {
        if let Err(error) = extract_natives(&library.path, &natives_dir) {
            tracing::warn!(library = %library.name, "could not extract natives: {error:#}");
        }
    }

    // Log4j config (not required to play, but needed to launch vanilla Java).
    if let Some(client_logging) = info
        .logging
        .as_ref()
        .and_then(|logging| logging.client.as_ref())
        && let Some(id) = &client_logging.file.id
    {
        let dest = paths.libraries_dir().join("logging").join(id);
        downloader.download(
            &DownloadJob::new(&client_logging.file.url, &dest)
                .with_hash(client_logging.file.sha1.clone(), client_logging.file.size),
            force,
            None,
        )?;
    }

    let mut asset_index_path = None;
    if options.assets != AssetMode::None
        && let Some(index) = &info.asset_index
    {
        let index_path = paths.asset_index_path(&index.id);
        download_index(&downloader, index, &index_path, force)?;
        asset_index_path = Some(index_path.clone());

        if options.assets == AssetMode::All {
            download_asset_objects(&downloader, paths, &index_path, options, force)?;
        }
    }

    let extracted_resources = if options.extract_resources {
        extract_client_resources(&client_jar, &paths.resources_dir(&summary.id), force)?
    } else {
        0
    };

    let resources_dir = paths.resources_dir(&summary.id);

    Ok(InstalledVersion {
        id: summary.id,
        version_json,
        client_jar,
        resources_dir,
        libraries,
        natives_dir,
        asset_index: asset_index_path,
        java_major: info.required_java_major(),
        main_class: info.main_class.clone(),
        info,
        extracted_resources,
    })
}

fn download_index(
    downloader: &Downloader,
    index: &AssetIndexInfo,
    dest: &Path,
    force: bool,
) -> Result<()> {
    downloader
        .download(
            &DownloadJob::new(&index.url, dest)
                .with_hash(Some(index.sha1.clone()), Some(index.size)),
            force,
            None,
        )
        .with_context(|| format!("downloading asset index {}", index.id))?;
    Ok(())
}

impl AssetMode {
    pub fn downloads_metadata(self) -> bool {
        self != Self::None
    }
}

/// Resolves the version manifest and downloads only the client jar.
///
/// Used by `mcrust decompile` and as a cheap repair path.
pub fn ensure_client_jar(
    paths: &Paths,
    requested: Option<&str>,
    force: bool,
) -> Result<(String, PathBuf)> {
    let downloader = Downloader::new()?;
    let manifest_path = paths.meta_dir().join("version_manifest_v2.json");
    let manifest: VersionManifest =
        downloader.fetch_json(crate::VERSION_MANIFEST_URL, &manifest_path, force)?;

    let id = requested
        .map(ToString::to_string)
        .unwrap_or_else(|| manifest.latest.release.clone());
    let summary = manifest
        .versions
        .iter()
        .find(|candidate| candidate.id == id)
        .with_context(|| format!("unknown Minecraft version {id}"))?;

    let version_json = paths.version_json(&id);
    let info: VersionInfo = downloader.fetch_json(&summary.url, &version_json, force)?;
    let client = info
        .client_download()
        .context("version metadata has no client download")?;
    let jar = paths.client_jar(&id);

    let bar = progress_bar(client.size.unwrap_or(0), "client.jar");
    downloader.download(
        &DownloadJob::new(&client.url, &jar).with_hash(client.sha1.clone(), client.size),
        force,
        Some(&bar),
    )?;
    bar.finish_and_clear();
    Ok((id, jar))
}

fn download_asset_objects(
    downloader: &Downloader,
    paths: &Paths,
    index_path: &Path,
    options: &InstallOptions,
    force: bool,
) -> Result<()> {
    let bytes = fs::read(index_path)?;
    let index: AssetIndex = serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {}", index_path.display()))?;

    let jobs: Vec<DownloadJob> = index
        .objects
        .values()
        .map(|object| {
            DownloadJob::new(
                format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    &object.hash[..2],
                    object.hash
                ),
                paths.asset_object_path(&object.hash),
            )
            .with_hash(Some(object.hash.clone()), Some(object.size))
        })
        .collect();

    let total = index
        .objects
        .values()
        .map(|object| object.size)
        .sum::<u64>();
    let bar = (!options.quiet).then(|| progress_bar(total, "assets"));
    tracing::info!(files = jobs.len(), "downloading asset objects");
    downloader
        .download_many(&jobs, options.threads.max(8), force, bar.as_ref())
        .context("downloading asset objects")?;
    if let Some(bar) = bar {
        bar.finish_and_clear();
    }
    Ok(())
}

fn progress_bar(total: u64, message: &str) -> ProgressBar {
    let bar = ProgressBar::new(total);
    let style = ProgressStyle::with_template(
        "{spinner:.green} {msg:16} [{bar:32.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("=>-");
    bar.set_style(style);
    bar.set_message(message.to_string());
    bar
}

/// Filters libraries for the current OS/arch and resolves their paths.
pub fn resolve_libraries(info: &VersionInfo, paths: &Paths) -> Vec<ResolvedLibrary> {
    let features = HashMap::new();
    let native_suffix = format!(":{}", native_classifier_for_arch());
    let mut resolved = Vec::new();

    for library in &info.libraries {
        if !rules_allow(&library.rules, &features) {
            continue;
        }

        let Some(downloads) = &library.downloads else {
            continue;
        };

        // New style: the classifier is part of the Maven name.
        let is_new_style_native = library.name.contains(":natives-");
        if is_new_style_native && !library.name.ends_with(&native_suffix) {
            continue;
        }

        if let Some(artifact) = &downloads.artifact
            && let Some(relative) = artifact
                .path
                .clone()
                .or_else(|| maven_path_from_name(&library.name))
        {
            resolved.push(ResolvedLibrary {
                name: library.name.clone(),
                path: paths.library_path(&relative),
                relative,
                url: artifact.url.clone(),
                sha1: artifact.sha1.clone(),
                size: artifact.size,
                native: is_new_style_native,
            });
            continue;
        }

        // Legacy style: `natives` maps an OS name to a classifier.
        if let Some(native) = pick_legacy_native(library)
            && let Some(relative) = native.path.or_else(|| maven_path_from_name(&library.name))
        {
            resolved.push(ResolvedLibrary {
                name: library.name.clone(),
                path: paths.library_path(&relative),
                relative,
                url: native.url,
                sha1: native.sha1,
                size: native.size,
                native: true,
            });
        }
    }

    resolved
}

fn pick_legacy_native(library: &Library) -> Option<crate::meta::DownloadInfo> {
    let downloads = library.downloads.as_ref()?;
    let natives = library.natives.as_ref()?;
    let key = natives.get(crate::rules::current_os_name())?;
    let mut info = downloads.classifiers.get(key)?.clone();
    if info.path.is_none() {
        info.path = maven_path_from_name(&library.name);
    }
    Some(info)
}

fn maven_path_from_name(name: &str) -> Option<String> {
    let mut parts = name.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next();
    let file = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    Some(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

fn extract_natives(jar: &Path, destination: &Path) -> Result<usize> {
    let file = File::open(jar).with_context(|| format!("opening {}", jar.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("reading native jar {}", jar.display()))?;
    let mut count = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let Ok(name) = entry.name() else {
            continue;
        };
        let name = name.to_string();
        if name.starts_with("META-INF/") {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".so") || lower.ends_with(".dll") || lower.ends_with(".dylib")) {
            continue;
        }
        let file_name = Path::new(&name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("native");
        let out = destination.join(file_name);
        if out.exists() {
            count += 1;
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        fs::write(&out, bytes).with_context(|| format!("writing {}", out.display()))?;
        count += 1;
    }
    Ok(count)
}

/// Extracts `assets/minecraft/**` from the client jar into a resource pack root.
pub fn extract_client_resources(jar: &Path, destination: &Path, force: bool) -> Result<usize> {
    let marker = destination.join(".mcrust-pack");
    let jar_hash = sha1_file(jar)?;
    if !force
        && marker.exists()
        && fs::read_to_string(&marker).is_ok_and(|value| value.trim() == jar_hash)
    {
        return Ok(0);
    }

    let file = File::open(jar).with_context(|| format!("opening {}", jar.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("reading client jar {}", jar.display()))?;
    fs::create_dir_all(destination)?;

    let mut count = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let Ok(name) = entry.name() else {
            continue;
        };
        let name = name.to_string();
        let Some(relative) = name.strip_prefix("assets/minecraft/") else {
            continue;
        };
        if relative.is_empty() || relative.split('/').any(|part| part == "..") {
            continue;
        }
        let out = destination.join("assets").join("minecraft").join(relative);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        fs::write(&out, bytes).with_context(|| format!("writing {}", out.display()))?;
        count += 1;
    }

    fs::write(&marker, jar_hash)?;
    tracing::info!(files = count, "extracted vanilla resource pack");
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path_from_name("org.lwjgl:lwjgl:3.4.3").as_deref(),
            Some("org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3.jar")
        );
        assert_eq!(
            maven_path_from_name("org.lwjgl:lwjgl:3.4.3:natives-linux").as_deref(),
            Some("org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-linux.jar")
        );
    }
}
