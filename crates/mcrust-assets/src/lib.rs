//! Everything needed to obtain a vanilla Minecraft: Java Edition installation.
//!
//! This crate deliberately does **not** bundle any Mojang files. It talks to the
//! official metadata endpoints, verifies SHA-1 hashes, and keeps the downloaded
//! data in a per-user cache (`~/.mcrust` by default). The same pipeline is used
//! by the CLI and by the game client on first launch.

mod download;
mod install;
mod launch;
mod meta;
mod paths;
mod rules;

pub use download::{DownloadJob, Downloader};
pub use install::{
    AssetMode, InstallOptions, InstalledVersion, ensure_client_jar, extract_client_resources,
    install, resolve_libraries,
};
pub use launch::{LaunchOptions, launch_vanilla};
pub use meta::*;
pub use paths::Paths;
pub use rules::{Rule, current_os_name, rules_allow};

/// Official version manifest. This is the entry point for every install.
pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Default version used when the user does not pass `--version`.
pub const DEFAULT_VERSION: &str = "26.3";

/// Version of Vineflower used by `mcrust decompile`.
pub const VINEFLOWER_VERSION: &str = "1.12.0";
pub const VINEFLOWER_URL: &str =
    "https://github.com/Vineflower/vineflower/releases/download/1.12.0/vineflower-1.12.0-slim.jar";
