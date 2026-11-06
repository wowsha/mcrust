use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// A directory laid out like a resource pack (`assets/minecraft/...`).
#[derive(Debug, Clone)]
pub struct ResourcePack {
    root: PathBuf,
}

impl ResourcePack {
    /// Opens a pack root. The directory must contain `assets/minecraft`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        if !root.join("assets").join("minecraft").is_dir() {
            bail!(
                "{} is not a resource pack (missing assets/minecraft). Run `mcrust setup`.",
                root.display()
            );
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads a pack-relative path such as `assets/minecraft/textures/block/stone.png`.
    pub fn read(&self, relative: &str) -> Result<Option<Vec<u8>>> {
        let Some(path) = self.resolve(relative) else {
            return Ok(None);
        };
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(
            fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
        ))
    }

    pub fn read_string(&self, relative: &str) -> Result<Option<String>> {
        Ok(self
            .read(relative)?
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.resolve(relative).is_some_and(|path| path.is_file())
    }

    /// Lists files below a pack-relative directory, as pack-relative paths.
    pub fn list(&self, relative_dir: &str) -> Result<Vec<String>> {
        let Some(dir) = self.resolve(relative_dir) else {
            return Ok(Vec::new());
        };
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let mut stack = vec![dir];
        while let Some(current) = stack.pop() {
            for entry in fs::read_dir(&current)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(relative) = path.strip_prefix(&self.root) {
                    out.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn texture_path(&self, name: &str) -> String {
        format!("assets/minecraft/textures/block/{name}.png")
    }

    pub fn blockstate_path(&self, name: &str) -> String {
        format!("assets/minecraft/blockstates/{name}.json")
    }

    pub fn model_path(&self, name: &str) -> String {
        format!("assets/minecraft/models/{name}.json")
    }

    fn resolve(&self, relative: &str) -> Option<PathBuf> {
        let relative = relative.replace('\\', "/");
        if relative.starts_with('/') {
            return None;
        }
        if relative
            .split('/')
            .any(|part| part == ".." || part.is_empty())
        {
            return None;
        }
        Some(self.root.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        let pack = ResourcePack {
            root: PathBuf::from("/tmp/pack"),
        };
        assert!(pack.resolve("../../etc/passwd").is_none());
        assert!(pack.resolve("/etc/passwd").is_none());
        assert!(pack.resolve("assets/minecraft/lang/en_us.json").is_some());
    }

    #[test]
    fn opens_an_extracted_pack() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("assets/minecraft/textures/block")).unwrap();
        let pack = ResourcePack::open(dir.path()).unwrap();
        assert_eq!(pack.root(), dir.path());
    }
}
