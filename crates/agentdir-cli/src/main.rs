use std::io::Write;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use agentdir::error::AgentdirError;
use agentdir::types::{MappingDirection, MaterializeStrategy, SourcePath, VirtualPath};
use agentdir::workspace::Workspace;

#[derive(Parser)]
#[command(
    name = "agentdir",
    version,
    about = "Virtual filesystem for agent-optimized exploration"
)]
struct Cli {
    /// Workspace root directory (default: current directory)
    #[arg(short = 'w', long, global = true)]
    workspace: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new workspace
    Init {
        /// Path to create the workspace at
        path: PathBuf,
        /// Materialization strategy: reflink, symlink, hardlink, or virtual
        #[arg(long, default_value = "reflink", value_parser = parse_strategy)]
        strategy: String,
    },
    /// Map a source directory into the virtual tree
    Map {
        /// Source directory to map
        source: PathBuf,
        /// Virtual mount point (e.g., /docs)
        mount: String,
    },
    /// Apply a batch path mapping from a JSON file
    MapBatch {
        /// Path to JSON file: {"source_path": "virtual_path", ...}
        #[arg(long)]
        from_json: PathBuf,
    },
    /// Remove a source mapping from the virtual tree
    Unmap {
        /// Virtual mount point to remove
        mount: String,
    },
    /// Show workspace status
    Status,
    /// Show metadata for a virtual path
    Stat {
        /// Virtual path to inspect
        path: String,
    },
    /// Print file contents from source via virtual path
    Cat {
        /// Virtual path to read
        path: String,
    },
    /// Detect and apply source changes
    Refresh,
    /// Move an entry in the virtual namespace
    Mv {
        /// Source virtual path
        from: String,
        /// Destination virtual path
        to: String,
    },
    /// Copy an entry in the virtual namespace
    Cp {
        /// Source virtual path
        from: String,
        /// Destination virtual path
        to: String,
    },
    /// Create a virtual directory
    Mkdir {
        /// Virtual path to create
        path: String,
    },
    /// Remove a virtual directory
    Rmdir {
        /// Virtual path to remove
        path: String,
        /// Remove recursively
        #[arg(short, long)]
        recursive: bool,
    },
    /// Export source→virtual path mapping as JSON (use --reverse for virtual→source)
    ExportMapping {
        /// Output format
        #[arg(long, default_value = "json", value_parser = validate_format)]
        format: String,
        /// Reverse direction: emit virtual→source instead of source→virtual
        #[arg(long)]
        reverse: bool,
        /// Make source paths relative to this base directory
        #[arg(long)]
        relative_to: Option<PathBuf>,
    },
    /// Watch for source changes and auto-sync (runs in foreground)
    Watch {
        /// Polling interval in seconds
        #[arg(short, long, default_value = "60")]
        interval: u64,
    },
}

fn parse_strategy(s: &str) -> std::result::Result<String, String> {
    match s {
        "reflink" | "symlink" | "hardlink" | "virtual" => Ok(s.to_string()),
        other => Err(format!(
            "unknown strategy '{other}'; expected reflink, symlink, hardlink, or virtual"
        )),
    }
}

fn strategy_from_str(s: &str) -> MaterializeStrategy {
    match s {
        "symlink" => MaterializeStrategy::Symlink,
        "hardlink" => MaterializeStrategy::Hardlink,
        "virtual" => MaterializeStrategy::Virtual,
        _ => MaterializeStrategy::Reflink,
    }
}

fn validate_format(s: &str) -> std::result::Result<String, String> {
    match s {
        "json" => Ok(s.to_string()),
        other => Err(format!("unsupported format '{other}', only 'json' is supported")),
    }
}

fn resolve_workspace(workspace_arg: Option<PathBuf>) -> PathBuf {
    workspace_arg.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn print_error(e: &AgentdirError) {
    eprintln!("Error: {e}");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let workspace_root = resolve_workspace(cli.workspace);

    if let Err(e) = run(cli.command, workspace_root).await {
        print_error(&e);
        std::process::exit(1);
    }
}

async fn run(command: Commands, workspace_root: PathBuf) -> agentdir::error::Result<()> {
    match command {
        Commands::Init { path, strategy } => {
            let strat = strategy_from_str(&strategy);
            let ws = Workspace::init_with_strategy(path.clone(), strat)?;
            println!("Initialized workspace at {} (strategy: {strategy})", path.display());
            println!("Manifest: {}", ws.manifest_path.display());
            Ok(())
        }

        Commands::Map { source, mount } => {
            let mut ws = Workspace::open(workspace_root)?;
            let source_path = SourcePath::new(source.canonicalize().map_err(AgentdirError::Io)?);
            let mount_path = VirtualPath::new(&mount)?;

            let summary = ws.map(source_path, mount_path).await?;
            println!(
                "Mapped: {} entries added ({} reflinked, {} copied, {} symlinked, {} hardlinked, {} dirs)",
                summary.entries_added, summary.reflinked, summary.copied,
                summary.symlinked, summary.hardlinked, summary.dirs_created
            );
            if summary.errors > 0 {
                eprintln!("Warning: {} entries failed to materialize", summary.errors);
            }
            Ok(())
        }

        Commands::MapBatch { from_json } => {
            let mut ws = Workspace::open(workspace_root)?;
            let json_content = std::fs::read_to_string(&from_json).map_err(AgentdirError::Io)?;
            let raw: std::collections::BTreeMap<String, String> =
                serde_json::from_str(&json_content)
                    .map_err(|e| AgentdirError::ManifestParse(e.to_string()))?;

            let mappings: Vec<(SourcePath, VirtualPath)> = raw
                .into_iter()
                .map(|(src, virt)| {
                    let sp = std::path::Path::new(&src)
                        .canonicalize()
                        .map(SourcePath::new)
                        .map_err(|e| {
                            AgentdirError::EntryNotFound(format!("source {src}: {e}"))
                        })?;
                    let vp = VirtualPath::new(&virt)?;
                    Ok((sp, vp))
                })
                .collect::<agentdir::error::Result<_>>()?;

            let summary = ws.map_batch(mappings).await?;
            println!(
                "Batch mapped: {} entries ({} reflinked, {} copied, {} symlinked, {} hardlinked, {} dirs)",
                summary.entries_added, summary.reflinked, summary.copied,
                summary.symlinked, summary.hardlinked, summary.dirs_created
            );
            if !summary.errors.is_empty() {
                eprintln!("Warning: {} entries failed", summary.errors.len());
            }
            Ok(())
        }

        Commands::Unmap { mount } => {
            let mut ws = Workspace::open(workspace_root)?;
            let mount_path = VirtualPath::new(&mount)?;
            let summary = ws.unmap(&mount_path)?;
            println!("Unmapped: {} entries removed", summary.entries_removed);
            Ok(())
        }

        Commands::Status => {
            let ws = Workspace::open(workspace_root)?;
            let status = ws.status();
            println!("Workspace: {}", status.materialized_root.display());
            println!("Entries: {}", status.total_entries);
            println!("Source roots: {}", status.source_roots);
            println!(
                "Last updated: {} (epoch secs)",
                status.last_updated_epoch_secs
            );
            Ok(())
        }

        Commands::Stat { path } => {
            let ws = Workspace::open(workspace_root)?;
            let vpath = VirtualPath::new(&path)?;
            let stat = ws.stat(&vpath)?;
            println!("Path: {}", stat.virtual_path);
            println!("Source: {}", stat.source_path);
            println!("Size: {} bytes", stat.size_bytes);
            println!("Mtime: {} ns", stat.mtime_ns);
            println!("Type: {:?}", stat.entry_type);
            println!("Materialized: {}", stat.materialized);
            Ok(())
        }

        Commands::Cat { path } => {
            let ws = Workspace::open(workspace_root)?;
            let vpath = VirtualPath::new(&path)?;
            let bytes = ws.read_bytes(&vpath).await?;
            std::io::stdout().write_all(&bytes)?;
            Ok(())
        }

        Commands::Refresh => {
            let mut ws = Workspace::open(workspace_root)?;
            let summary = ws.refresh().await?;
            println!(
                "Synced: +{} added, ~{} refreshed, -{} removed ({} errors)",
                summary.added,
                summary.refreshed,
                summary.removed,
                summary.errors.len()
            );
            Ok(())
        }

        Commands::Mv { from, to } => {
            let mut ws = Workspace::open(workspace_root)?;
            let from_path = VirtualPath::new(&from)?;
            let to_path = VirtualPath::new(&to)?;
            ws.mv(&from_path, &to_path)?;
            println!("Moved {from} -> {to}");
            Ok(())
        }

        Commands::Cp { from, to } => {
            let mut ws = Workspace::open(workspace_root)?;
            let from_path = VirtualPath::new(&from)?;
            let to_path = VirtualPath::new(&to)?;
            ws.cp(&from_path, &to_path)?;
            println!("Copied {from} -> {to}");
            Ok(())
        }

        Commands::Mkdir { path } => {
            let mut ws = Workspace::open(workspace_root)?;
            let vpath = VirtualPath::new(&path)?;
            ws.mkdir(&vpath)?;
            println!("Created directory {path}");
            Ok(())
        }

        Commands::Rmdir { path, recursive } => {
            let mut ws = Workspace::open(workspace_root)?;
            let vpath = VirtualPath::new(&path)?;
            ws.rmdir(&vpath, recursive)?;
            println!("Removed directory {path}");
            Ok(())
        }

        Commands::ExportMapping {
            format: _,
            reverse,
            relative_to,
        } => {
            let ws = Workspace::open(workspace_root)?;
            let direction = if reverse {
                MappingDirection::VirtualToSource
            } else {
                MappingDirection::SourceToVirtual
            };
            let mapping = ws.export_mapping(direction, relative_to.as_deref())?;
            let json = serde_json::to_string_pretty(&mapping)
                .map_err(|e| AgentdirError::ManifestWrite(e.to_string()))?;
            println!("{json}");
            Ok(())
        }

        Commands::Watch { interval } => run_watch(workspace_root, interval).await,
    }
}

async fn run_watch(workspace_root: PathBuf, interval: u64) -> agentdir::error::Result<()> {
    use agentdir::backend::SourceEvent;
    use agentdir::reconciler::Reconciler;
    use agentdir::watcher::FileWatcher;
    use std::time::Duration;
    use tokio::time;

    let mut ws = Workspace::open(workspace_root)?;
    let roots: Vec<_> = ws
        .catalog
        .source_roots()
        .iter()
        .map(|r| r.source_path.clone())
        .collect();

    println!(
        "Watching {} source roots. Press Ctrl+C to stop.",
        roots.len()
    );

    let watcher = FileWatcher::new(ws.backend.clone(), roots)
        .with_poll_interval(Duration::from_secs(interval));

    let (mut rx, _handle) = watcher.start().await?;

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                println!("\nShutting down...");
                ws.save()?;
                break;
            }
            Some(event) = rx.recv() => {
                let mut events = vec![event];
                let batch_deadline = time::sleep(Duration::from_millis(100));
                tokio::pin!(batch_deadline);

                loop {
                    tokio::select! {
                        _ = &mut batch_deadline => break,
                        Some(e) = rx.recv() => events.push(e),
                    }
                }

                let mut all_actions = Vec::new();
                let mut needs_full_reconcile = false;

                for ev in &events {
                    if matches!(ev, SourceEvent::RescanNeeded) {
                        needs_full_reconcile = true;
                    } else {
                        match Reconciler::from_event(&ws.catalog, ev) {
                            Ok(actions) => all_actions.extend(actions),
                            Err(e) => tracing::warn!("event processing error: {e}"),
                        }
                    }
                }

                if needs_full_reconcile {
                    let roots = ws.catalog.source_roots().to_vec();
                    match Reconciler::full_reconcile(&ws.catalog, ws.backend.as_ref(), &roots).await {
                        Ok(actions) => all_actions.extend(actions),
                        Err(e) => tracing::warn!("full reconcile error: {e}"),
                    }
                }

                if !all_actions.is_empty() {
                    match Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &all_actions) {
                        Ok(summary) => {
                            println!(
                                "Synced: +{} added, ~{} refreshed, -{} removed",
                                summary.added, summary.refreshed, summary.removed
                            );
                            if let Err(e) = ws.save() {
                                tracing::warn!("failed to save manifest: {e}");
                            }
                        }
                        Err(e) => tracing::warn!("apply actions error: {e}"),
                    }
                }
            }
        }
    }

    Ok(())
}
