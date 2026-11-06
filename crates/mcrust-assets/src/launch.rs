use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::install::InstalledVersion;
use crate::meta::{Argument, ArgumentValue};
use crate::paths::Paths;
use crate::rules::rules_allow;

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub version: String,
    pub username: String,
    pub demo: bool,
    pub width: u32,
    pub height: u32,
    pub dry_run: bool,
    pub memory_mb: Option<u32>,
    pub extra_game_args: Vec<String>,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            version: crate::DEFAULT_VERSION.to_string(),
            username: "Player".to_string(),
            demo: false,
            width: 1280,
            height: 720,
            dry_run: false,
            memory_mb: None,
            extra_game_args: Vec::new(),
        }
    }
}

/// Launches the *vanilla Java client* using the files mcrust downloaded.
///
/// This is a reference tool: it lets you compare rendering and behaviour while
/// porting. It uses an offline/demo profile, so online servers that require a
/// Microsoft account will not work.
pub fn launch_vanilla(paths: &Paths, options: &LaunchOptions) -> Result<()> {
    let installed = InstalledVersion::load(paths, &options.version)?;
    let required_java = installed.java_major;
    let java = find_java(required_java)?;

    let features = HashMap::from([
        ("is_demo_user".to_string(), options.demo),
        ("has_custom_resolution".to_string(), true),
    ]);

    let natives = installed.natives_dir.clone();
    // The version metadata expects `${natives_directory}/java` to hold the
    // native libraries. `InstalledVersion::natives_dir` already points there.
    let natives_root = natives
        .parent()
        .map_or_else(|| natives.clone(), std::path::Path::to_path_buf);
    for dir in ["jna", "lwjgl", "netty"] {
        fs::create_dir_all(natives_root.join(dir)).ok();
    }

    let classpath = installed.classpath();
    let mut jvm_args = resolve_arguments(
        installed
            .info
            .arguments
            .as_ref()
            .map(|args| args.jvm.as_slice())
            .unwrap_or(&[]),
        &features,
    );
    let mut game_args = resolve_arguments(
        installed
            .info
            .arguments
            .as_ref()
            .map(|args| args.game.as_slice())
            .unwrap_or(&[]),
        &features,
    );

    let assets_root = paths.assets_dir();
    fs::create_dir_all(&assets_root)?;
    let game_dir = paths.data_dir();
    fs::create_dir_all(&game_dir)?;

    let access_token = "0";
    let uuid = offline_uuid(&options.username);
    let asset_index = installed
        .info
        .assets
        .clone()
        .unwrap_or_else(|| "legacy".to_string());

    let replacements = HashMap::from([
        ("natives_directory", natives.to_string_lossy().into_owned()),
        ("launcher_name", "mcrust".to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
        ("classpath", classpath),
        ("auth_player_name", options.username.clone()),
        ("version_name", installed.id.clone()),
        ("game_directory", game_dir.to_string_lossy().into_owned()),
        ("assets_root", assets_root.to_string_lossy().into_owned()),
        ("assets_index_name", asset_index),
        ("auth_uuid", uuid),
        ("auth_access_token", access_token.to_string()),
        ("clientid", "0".to_string()),
        ("auth_xuid", "0".to_string()),
        ("version_type", "release".to_string()),
        ("resolution_width", options.width.to_string()),
        ("resolution_height", options.height.to_string()),
    ]);

    substitute(&mut jvm_args, &replacements);
    substitute(&mut game_args, &replacements);

    if let Some(logging) = installed
        .info
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
        && let Some(path) = installed.logging_config(paths)
    {
        jvm_args.push(logging.argument.replace("${path}", &path.to_string_lossy()));
    }

    if let Some(memory) = options.memory_mb {
        jvm_args.retain(|arg| !arg.starts_with("-Xmx") && !arg.starts_with("-Xms"));
        jvm_args.push(format!("-Xmx{memory}M"));
        jvm_args.push(format!("-Xms{}M", memory.min(2048)));
    }

    game_args.extend(options.extra_game_args.iter().cloned());

    let mut command = Command::new(&java);
    command.args(&jvm_args);
    command.arg(&installed.main_class);
    command.args(&game_args);
    command.current_dir(&game_dir);

    if options.dry_run {
        println!(
            "{}",
            display_command(&java, &jvm_args, &installed.main_class, &game_args)
        );
        return Ok(());
    }

    tracing::info!(java = %java.display(), version = %installed.id, "starting vanilla Minecraft");
    let status = command
        .status()
        .with_context(|| format!("failed to start {}", java.display()))?;
    if !status.success() {
        bail!("Minecraft exited with {status}");
    }
    Ok(())
}

fn resolve_arguments(arguments: &[Argument], features: &HashMap<String, bool>) -> Vec<String> {
    let mut out = Vec::new();
    for argument in arguments {
        match argument {
            Argument::Plain(value) => out.push(value.clone()),
            Argument::Conditional { rules, value } => {
                if rules_allow(rules, features) {
                    match value {
                        ArgumentValue::One(one) => out.push(one.clone()),
                        ArgumentValue::Many(many) => out.extend(many.iter().cloned()),
                    }
                }
            }
        }
    }
    out
}

fn substitute(arguments: &mut [String], replacements: &HashMap<&str, String>) {
    for argument in arguments.iter_mut() {
        let mut value = argument.clone();
        for (key, replacement) in replacements {
            value = value.replace(&format!("${{{key}}}"), replacement);
        }
        *argument = value;
    }
}

fn display_command(java: &Path, jvm_args: &[String], main: &str, game_args: &[String]) -> String {
    let mut parts = vec![java.to_string_lossy().into_owned()];
    parts.extend(jvm_args.iter().map(|arg| quote(arg)));
    parts.push(main.to_string());
    parts.extend(game_args.iter().map(|arg| quote(arg)));
    parts.join(" ")
}

fn quote(value: &str) -> String {
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

fn offline_uuid(username: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(b"OfflinePlayer:");
    hasher.update(username.as_bytes());
    let digest = hasher.finalize();
    crate::download::hex(digest.as_slice())[..32].to_string()
}

fn find_java(required_major: u32) -> Result<PathBuf> {
    let candidates: Vec<PathBuf> = std::env::var_os("MCRUST_JAVA")
        .map(PathBuf::from)
        .into_iter()
        .chain(
            std::env::var_os("JAVA_HOME").map(|home| PathBuf::from(home).join("bin").join("java")),
        )
        .chain(std::iter::once(PathBuf::from("java")))
        .collect();

    for candidate in candidates {
        if let Ok(version) = java_major(&candidate)
            && version >= required_major
        {
            return Ok(candidate);
        }
    }

    bail!(
        "Minecraft {} needs Java {required_major}+. Install a JDK and set MCRUST_JAVA or JAVA_HOME.",
        crate::DEFAULT_VERSION
    )
}

/// Returns the major version of a `java` binary.
pub fn java_major(java: &PathBuf) -> Result<u32> {
    let output = Command::new(java)
        .arg("-version")
        .output()
        .with_context(|| format!("running {}", java.display()))?;
    let text = String::from_utf8_lossy(&output.stderr);
    parse_java_version(&text)
        .with_context(|| format!("could not parse the version of {}", java.display()))
}

fn parse_java_version(output: &str) -> Option<u32> {
    let marker = output.find("version \"")? + "version \"".len();
    let rest = &output[marker..];
    let version = rest.split('"').next()?;
    if let Some(legacy) = version.strip_prefix("1.") {
        return legacy.split('.').next()?.parse().ok();
    }
    version.split('.').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_java_version() {
        assert_eq!(
            parse_java_version("openjdk version \"25.0.1\" 2025-10-21"),
            Some(25)
        );
        assert_eq!(parse_java_version("java version \"1.8.0_402\""), Some(8));
    }

    #[test]
    fn substitutes_placeholders() {
        let mut args = vec!["-cp".to_string(), "${classpath}".to_string()];
        let map = HashMap::from([("classpath", "a:b".to_string())]);
        substitute(&mut args, &map);
        assert_eq!(args[1], "a:b");
    }
}
