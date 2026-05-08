use anyhow::{Context, Result};
use clap::Parser;
use quicproxy::bootstrap;
use quicproxy::config::Config;
use quicproxy::utils::elevate::{self, ElevateConfig};
use std::path::PathBuf;
use std::process;
use tracing::{debug, info};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "snmalloc")]
#[global_allocator]
static GLOBAL: snmalloc_rs::SnMalloc = snmalloc_rs::SnMalloc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Require administrator/root privileges
    #[arg(long)]
    elevate: bool,

    /// Do not show the console window when elevating privileges (Windows only)
    #[arg(long)]
    elevate_no_show_window: bool,
}

fn main() -> Result<()> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();

    let runtime = builder
        .build()
        .context("Failed to build tokio runtime")?;
    runtime.block_on(async_main())
}

async fn async_main() -> Result<()> {
    let args = Args::parse();

    if args.elevate {
        if !elevate::is_elevated() {
            eprintln!("Requesting administrator privileges...");

            let elevate_config = ElevateConfig {
                prompt_title: "QuicProxy".to_string(),
                prompt_message:
                    "QuicProxy requires administrator privileges to configure network interfaces."
                        .to_string(),
                show_window: !args.elevate_no_show_window,
                preserve_env_vars: vec![
                    "PATH".to_string(),
                    "HOME".to_string(),
                    "USER".to_string(),
                    "RUST_LOG".to_string(),
                    "RUST_BACKTRACE".to_string(),
                ],
                ..ElevateConfig::default()
            };

            // Reconstruct arguments
            let program_args = elevate::reconstruct_args();

            let executable = elevate::current_executable().context("Failed to get current executable path")?;
            if let Err(e) = elevate::elevate_command(
                executable.to_str().unwrap_or(""),
                &program_args,
                &elevate_config,
            ) {
                eprintln!("Failed to elevate privileges: {:#}", e);
                process::exit(1);
            }

            return Ok(());
        }
    }

    // Load config using the logic in config.rs
    let config = match Config::load(args.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading configuration:");
            eprintln!("  {:#}", e);
            process::exit(1);
        }
    };

    if elevate::is_elevated() {
        info!("Running with elevated privileges");
    } else {
        debug!("Running without elevated privileges");
    }

    // Run application
    if let Err(e) = bootstrap::run_with_signal(config, async {
        info!("Proxy started. Press Ctrl-C to stop.");
        let _ = tokio::signal::ctrl_c().await;
        Ok(())
    })
    .await
    {
        eprintln!("Application error: {:#}", e);
        process::exit(1);
    }

    Ok(())
}
