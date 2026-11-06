use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use mcrust_assets::{
    AssetMode, DownloadJob, Downloader, InstallOptions, InstalledVersion, LaunchOptions, Paths,
    ensure_client_jar, install,
};
use mcrust_client::ClientOptions;

#[derive(Debug, Parser)]
#[command(
    name = "mcrust",
    version,
    about = "A from-scratch Minecraft: Java Edition port in Rust (Bevy + wgpu)",
    long_about = "Downloads vanilla Minecraft 26.3 from Mojang's official servers, extracts the \
                  resource pack and can either run the Rust client or the vanilla Java client for \
                  side-by-side comparison. No Mojang files are bundled with mcrust."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Download and verify a vanilla version (default: latest release).
    Setup {
        /// Minecraft version, e.g. 26.3.
        #[arg(long)]
        version: Option<String>,
        /// How much of the asset store to download.
        #[arg(long, value_enum, default_value_t = AssetChoice::Index)]
        assets: AssetChoice,
        /// Re-download and re-extract everything.
        #[arg(long)]
        force: bool,
        /// Do not extract assets/minecraft from client.jar.
        #[arg(long)]
        no_extract: bool,
        /// Parallel downloads.
        #[arg(long)]
        threads: Option<usize>,
    },
    /// Run the Rust client (installs on first launch).
    Play {
        #[arg(long)]
        version: Option<String>,
        #[arg(long, default_value_t = 8)]
        render_distance: i32,
        #[arg(long)]
        seed: Option<u64>,
        #[arg(long, default_value_t = 1280)]
        width: u32,
        #[arg(long, default_value_t = 720)]
        height: u32,
        /// Fail instead of installing automatically.
        #[arg(long)]
        no_setup: bool,
    },
    /// Launch the vanilla Java client with the downloaded files (comparison aid).
    Launch {
        #[arg(long)]
        version: Option<String>,
        #[arg(long, default_value = "Player")]
        username: String,
        #[arg(long)]
        demo: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        memory_mb: Option<u32>,
        #[arg(long, default_value_t = 1280)]
        width: u32,
        #[arg(long, default_value_t = 720)]
        height: u32,
    },
    /// Decompile the vanilla client for local reference (never committed).
    Decompile {
        #[arg(long)]
        version: Option<String>,
        /// Overwrite an existing decompilation.
        #[arg(long)]
        force: bool,
        /// Heap size for Vineflower.
        #[arg(long, default_value = "6G")]
        heap: String,
    },
    /// Show what is installed and where.
    Info {
        #[arg(long)]
        version: Option<String>,
    },
    /// Print the mcrust data directories.
    Paths,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AssetChoice {
    None,
    Index,
    All,
}

impl From<AssetChoice> for AssetMode {
    fn from(value: AssetChoice) -> Self {
        match value {
            AssetChoice::None => Self::None,
            AssetChoice::Index => Self::Index,
            AssetChoice::All => Self::All,
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,bevy=warn,wgpu=warn".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("\nerror: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let paths = Paths::discover()?;
    match cli.command.unwrap_or(Command::Play {
        version: None,
        render_distance: 8,
        seed: None,
        width: 1280,
        height: 720,
        no_setup: false,
    }) {
        Command::Setup {
            version,
            assets,
            force,
            no_extract,
            threads,
        } => setup(&paths, version, assets.into(), force, no_extract, threads),
        Command::Play {
            version,
            render_distance,
            seed,
            width,
            height,
            no_setup,
        } => play(
            &paths,
            version,
            render_distance,
            seed,
            width,
            height,
            no_setup,
        ),
        Command::Launch {
            version,
            username,
            demo,
            dry_run,
            memory_mb,
            width,
            height,
        } => launch(
            &paths, version, username, demo, dry_run, memory_mb, width, height,
        ),
        Command::Decompile {
            version,
            force,
            heap,
        } => decompile(&paths, version, force, &heap),
        Command::Info { version } => info(&paths, version),
        Command::Paths => {
            print_paths(&paths);
            Ok(())
        }
    }
}

fn setup(
    paths: &Paths,
    version: Option<String>,
    assets: AssetMode,
    force: bool,
    no_extract: bool,
    threads: Option<usize>,
) -> Result<()> {
    let installed = install(
        paths,
        &InstallOptions {
            version,
            assets,
            force,
            extract_resources: !no_extract,
            threads: threads.unwrap_or(8),
            quiet: false,
        },
    )?;
    print_installed(paths, &installed);
    println!("\nNext: `mcrust play` (Rust client) or `mcrust launch` (vanilla Java).");
    Ok(())
}

fn play(
    paths: &Paths,
    version: Option<String>,
    render_distance: i32,
    seed: Option<u64>,
    width: u32,
    height: u32,
    no_setup: bool,
) -> Result<()> {
    let mut options = ClientOptions {
        version: version.clone().unwrap_or_else(|| default_version(paths)),
        render_distance,
        width,
        height,
        ..ClientOptions::default()
    };
    if let Some(seed) = seed {
        options.seed = seed;
    }

    if !is_playable(paths, &options.version) {
        if no_setup {
            bail!(
                "Minecraft {} is not installed. Run `mcrust setup --version {}` first.",
                options.version,
                options.version
            );
        }
        println!(
            "First launch: downloading Minecraft {} and its assets…",
            options.version
        );
        let installed = install(
            paths,
            &InstallOptions {
                version: Some(options.version.clone()),
                ..InstallOptions::default()
            },
        )?;
        print_installed(paths, &installed);
    }

    mcrust_client::run(options)
}

fn launch(
    paths: &Paths,
    version: Option<String>,
    username: String,
    demo: bool,
    dry_run: bool,
    memory_mb: Option<u32>,
    width: u32,
    height: u32,
) -> Result<()> {
    let id = version.unwrap_or_else(|| default_version(paths));
    if !is_playable(paths, &id) {
        bail!("Minecraft {id} is not installed. Run `mcrust setup --version {id}` first.");
    }
    mcrust_assets::launch_vanilla(
        paths,
        &LaunchOptions {
            version: id,
            username,
            demo,
            width,
            height,
            dry_run,
            memory_mb,
            extra_game_args: Vec::new(),
        },
    )
}

fn decompile(paths: &Paths, version: Option<String>, force: bool, heap: &str) -> Result<()> {
    let (id, jar) = ensure_client_jar(paths, version.as_deref(), false)?;
    let output = paths.vanilla_src_dir(&id);

    if output.exists() && !force {
        let count = count_java_files(&output)?;
        if count > 0 {
            println!(
                "Already decompiled {id}: {count} Java files in {}",
                output.display()
            );
            println!("Use --force to decompile again.");
            return Ok(());
        }
    }

    let tool = paths.tools_dir().join("vineflower.jar");
    fs::create_dir_all(paths.tools_dir())?;
    if force || !tool.exists() {
        println!("Downloading Vineflower {VINEFLOWER_VERSION}…");
        let downloader = Downloader::new()?;
        downloader.download(
            &DownloadJob::new(mcrust_assets::VINEFLOWER_URL, &tool),
            force,
            None,
        )?;
    }

    let java = find_java()?;
    fs::create_dir_all(&output)?;
    println!(
        "Decompiling {} with {} (heap {heap}). This can take several minutes…",
        jar.display(),
        java.display()
    );

    let status = ProcessCommand::new(&java)
        .arg(format!("-Xmx{heap}"))
        .arg("-jar")
        .arg(&tool)
        .arg(&jar)
        .arg(&output)
        .status()
        .with_context(|| format!("running {}", java.display()))?;

    if !status.success() {
        bail!("Vineflower failed with {status}");
    }

    println!(
        "\nDecompiled {} Java files into {}",
        count_java_files(&output)?,
        output.display()
    );
    println!(
        "This directory is derived from Mojang's code: keep it local, never commit it \
         (it is already in .gitignore)."
    );
    Ok(())
}

fn info(paths: &Paths, version: Option<String>) -> Result<()> {
    print_paths(paths);
    let id = version.unwrap_or_else(|| default_version(paths));
    println!("\nversion: {id}");

    match InstalledVersion::load(paths, &id) {
        Ok(installed) => {
            println!("  client.jar:      {}", installed.client_jar.display());
            println!("  version json:    {}", installed.version_json.display());
            println!("  resource pack:   {}", installed.resources_dir.display());
            println!("  libraries:       {}", installed.libraries.len());
            println!("  natives:         {}", installed.natives_dir.display());
            println!(
                "  asset index:     {}",
                installed.asset_index.map_or_else(
                    || "not downloaded".to_string(),
                    |path| path.display().to_string()
                )
            );
            println!("  required java:   {}", installed.java_major);
            println!(
                "  playable:        {}",
                if is_playable(paths, &id) {
                    "yes"
                } else {
                    "no (run setup)"
                }
            );

            let vanilla = paths.vanilla_src_dir(&id);
            if vanilla.exists() {
                println!(
                    "  decompiled:      {} Java files (reference only)",
                    count_java_files(&vanilla).unwrap_or(0)
                );
            }
        }
        Err(error) => println!("  not installed: {error:#}"),
    }

    let registry = mcrust_vanilla::BlockRegistry::builtin();
    println!(
        "\nblock registry: {} blocks, {} textures",
        registry.len(),
        registry.texture_names().len()
    );
    println!(
        "world: {}..={} ({} blocks tall)",
        mcrust_world::WORLD_MIN_Y,
        mcrust_world::WORLD_MAX_Y,
        mcrust_world::WORLD_HEIGHT
    );
    Ok(())
}

fn print_paths(paths: &Paths) {
    println!("mcrust home:    {}", paths.root().display());
    println!("  versions:     {}", paths.versions_dir().display());
    println!("  libraries:    {}", paths.libraries_dir().display());
    println!("  assets:       {}", paths.assets_dir().display());
    println!(
        "  decompiled:   {}",
        paths.vanilla_dir("<version>").display()
    );
    println!("  saves:        {}", paths.data_dir().display());
}

fn print_installed(paths: &Paths, installed: &InstalledVersion) {
    println!("\nMinecraft {} is ready", installed.id);
    println!("  client.jar:  {}", installed.client_jar.display());
    println!("  resources:   {}", installed.resources_dir.display());
    println!("  libraries:   {}", installed.libraries.len());
    if installed.extracted_resources > 0 {
        println!("  extracted:   {} files", installed.extracted_resources);
    }
    if let Some(index) = &installed.asset_index {
        println!("  asset index: {}", index.display());
    }
    println!("  home:        {}", paths.root().display());
}

fn default_version(_paths: &Paths) -> String {
    std::env::var("MCRUST_VERSION").unwrap_or_else(|_| mcrust_assets::DEFAULT_VERSION.to_string())
}

fn is_playable(paths: &Paths, version: &str) -> bool {
    InstalledVersion::load(paths, version).is_ok()
        && paths
            .resources_dir(version)
            .join("assets/minecraft/textures/block/stone.png")
            .exists()
}

fn find_java() -> Result<PathBuf> {
    let candidates: Vec<PathBuf> = std::env::var_os("MCRUST_JAVA")
        .map(PathBuf::from)
        .into_iter()
        .chain(
            std::env::var_os("JAVA_HOME").map(|home| PathBuf::from(home).join("bin").join("java")),
        )
        .chain(std::iter::once(PathBuf::from("java")))
        .collect();

    for candidate in &candidates {
        if ProcessCommand::new(candidate)
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return Ok(candidate.clone());
        }
    }
    bail!("Java not found. Install a JDK 17+ (Minecraft 26.3 itself needs Java 25 to run).")
}

fn count_java_files(root: &Path) -> Result<usize> {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "java")
            {
                count += 1;
            }
        }
    }
    Ok(count)
}

const VINEFLOWER_VERSION: &str = mcrust_assets::VINEFLOWER_VERSION;
