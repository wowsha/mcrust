use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Resolves the on-disk layout used by mcrust.
///
/// ```text
/// ~/.mcrust/
///   meta/version_manifest_v2.json
///   versions/26.3/26.3.json
///   versions/26.3/client.jar
///   versions/26.3/resources/assets/minecraft/...   <- extracted resource pack
///   versions/26.3/natives/java/...
///   libraries/...
///   assets/indexes/34.json
///   assets/objects/ab/cdef...
///   vanilla/26.3/src/...                           <- decompiled reference (local only)
/// ```
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// `$MCRUST_HOME` if set, otherwise `~/.mcrust`.
    pub fn discover() -> Result<Self> {
        if let Some(root) = std::env::var_os("MCRUST_HOME") {
            if root.is_empty() {
                anyhow::bail!("MCRUST_HOME is set but empty");
            }
            return Ok(Self { root: root.into() });
        }
        let home = dirs::home_dir().context("could not determine the home directory")?;
        Ok(Self {
            root: home.join(".mcrust"),
        })
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn meta_dir(&self) -> PathBuf {
        self.root.join("meta")
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id)
    }

    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }

    pub fn client_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("client.jar")
    }

    /// Root of the extracted `assets/minecraft/...` tree, usable as a resource pack.
    pub fn resources_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("resources")
    }

    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }

    /// Directory where native libraries are loaded from (`java.library.path`).
    pub fn natives_java_dir(&self, id: &str) -> PathBuf {
        self.natives_dir(id).join("java")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn library_path(&self, relative: &str) -> PathBuf {
        self.libraries_dir().join(relative)
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn asset_indexes_dir(&self) -> PathBuf {
        self.assets_dir().join("indexes")
    }

    pub fn asset_index_path(&self, id: &str) -> PathBuf {
        self.asset_indexes_dir().join(format!("{id}.json"))
    }

    pub fn asset_objects_dir(&self) -> PathBuf {
        self.assets_dir().join("objects")
    }

    pub fn asset_object_path(&self, hash: &str) -> PathBuf {
        let prefix = hash.get(..2).unwrap_or("00");
        self.asset_objects_dir().join(prefix).join(hash)
    }

    /// Vanilla reference data (decompiled Java). Never shipped with the game.
    pub fn vanilla_dir(&self, id: &str) -> PathBuf {
        self.root.join("vanilla").join(id)
    }

    pub fn vanilla_src_dir(&self, id: &str) -> PathBuf {
        self.vanilla_dir(id).join("src")
    }

    pub fn tools_dir(&self) -> PathBuf {
        self.root.join("tools")
    }

    /// Writable directory for saves, options and screenshots.
    pub fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_stable() {
        let paths = Paths::at("/tmp/mcrust-test");
        assert_eq!(
            paths.client_jar("26.3"),
            Path::new("/tmp/mcrust-test/versions/26.3/client.jar")
        );
        assert_eq!(
            paths.resources_dir("26.3"),
            Path::new("/tmp/mcrust-test/versions/26.3/resources")
        );
        assert_eq!(
            paths.asset_object_path("abcdef"),
            Path::new("/tmp/mcrust-test/assets/objects/ab/abcdef")
        );
    }
}
