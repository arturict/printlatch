use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use printlatch::{
    VERSION,
    api::{AppState, router},
    auth,
    config::AppConfig,
    db::Database,
    printers,
    worker::{self, QueueSignal},
};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "printlatch", version = VERSION, about)]
struct Cli {
    #[arg(long, global = true, env = "PRINTLATCH_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the loopback-only print agent.
    Serve(ServeArgs),
    /// Create a five-minute browser pairing code bound to one origin.
    Pair(PairArgs),
    /// Manage local and browser client credentials.
    Clients {
        #[command(subcommand)]
        command: ClientCommand,
    },
    /// List the PDF capture target and Windows printers.
    Printers,
    /// Show local diagnostics without revealing tokens or document contents.
    Diagnose,
}

#[derive(Args)]
struct ServeArgs {
    #[arg(long, env = "PRINTLATCH_PORT")]
    port: Option<u16>,
    #[arg(long)]
    json_logs: bool,
}

#[derive(Args)]
struct PairArgs {
    #[arg(long)]
    origin: String,
    #[arg(long, default_value = "Browser app")]
    name: String,
}

#[derive(Subcommand)]
enum ClientCommand {
    /// Create a token for a local non-browser process.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = 30)]
        days: i64,
    },
    /// Rotate a client token and immediately invalidate the old token.
    Rotate {
        client_id: String,
        #[arg(long, default_value_t = 30)]
        days: i64,
    },
    /// Revoke a client.
    Revoke { client_id: String },
    /// List clients without token material.
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve(arguments) => serve(cli.data_dir, arguments).await,
        Command::Pair(arguments) => {
            let (config, db) = local_state(cli.data_dir, None)?;
            let grant = auth::new_pairing_grant(&db, &arguments.origin, &arguments.name)?;
            println!("Pairing code: {}", grant.code);
            println!("Origin: {}", grant.origin);
            println!("Expires at (Unix): {}", grant.expires_at);
            println!("Agent data: {}", config.data_dir.display());
            Ok(())
        }
        Command::Clients { command } => clients(cli.data_dir, command),
        Command::Printers => {
            for printer in printers::list_printers()? {
                println!(
                    "{}\t{}\t{}\t{}",
                    printer.id,
                    printer.name,
                    printer.kind,
                    if printer.tested {
                        "verified"
                    } else {
                        "discovered"
                    }
                );
            }
            Ok(())
        }
        Command::Diagnose => diagnose(cli.data_dir),
    }
}

async fn serve(data_dir: Option<PathBuf>, arguments: ServeArgs) -> Result<()> {
    init_tracing(arguments.json_logs)?;
    let (config, db) = local_state(data_dir, arguments.port)?;
    let interrupted = db.recover_interrupted_jobs()?;
    if interrupted > 0 {
        tracing::warn!(
            interrupted_jobs = interrupted,
            "interrupted print submissions require explicit retry"
        );
    }
    let queue = QueueSignal::new();
    let state = AppState {
        config: config.clone(),
        db: db.clone(),
        queue: queue.clone(),
    };
    tokio::spawn(worker::run(config.clone(), db, queue));
    let address = SocketAddr::from(([127, 0, 0, 1], config.port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("could not bind http://{address}"))?;
    tracing::info!(
        address = %address,
        data_dir = %config.data_dir.display(),
        telemetry = false,
        "PrintLatch agent ready"
    );
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("agent server stopped unexpectedly")
}

fn clients(data_dir: Option<PathBuf>, command: ClientCommand) -> Result<()> {
    let (_, db) = local_state(data_dir, None)?;
    match command {
        ClientCommand::Create { name, days } => {
            let token = auth::issue_local_token(&db, &name, days)?;
            println!("Client ID: {}", token.client_id);
            println!("Token (shown once): {}", token.token);
            println!("Expires at (Unix): {}", token.expires_at);
        }
        ClientCommand::Rotate { client_id, days } => {
            let token = auth::rotate_token(&db, &client_id, days)?;
            println!("Client ID: {}", token.client_id);
            println!("New token (shown once): {}", token.token);
            println!("Expires at (Unix): {}", token.expires_at);
        }
        ClientCommand::Revoke { client_id } => {
            db.revoke_client(&client_id)?;
            println!("Revoked client {client_id}");
        }
        ClientCommand::List => {
            for (client, expires_at, revoked) in db.list_clients()? {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    client.id,
                    client.name,
                    client.kind.as_str(),
                    client.origin.as_deref().unwrap_or("-"),
                    expires_at,
                    if revoked { "revoked" } else { "active" }
                );
            }
        }
    }
    Ok(())
}

fn diagnose(data_dir: Option<PathBuf>) -> Result<()> {
    let (config, db) = local_state(data_dir, None)?;
    println!("PrintLatch {VERSION}");
    println!("Data directory: {}", config.data_dir.display());
    println!("Database: {} (ok)", db.path().display());
    println!("Bind policy: 127.0.0.1 only");
    println!("Default port: {}", config.port);
    println!("Telemetry: disabled");
    println!("PDF limit: 10 MiB / 100 pages");
    let printers = printers::list_printers()?;
    println!("Available targets: {}", printers.len());
    for printer in printers {
        println!(
            "  {} [{}; {}]",
            printer.name,
            printer.kind,
            if printer.tested {
                "verified"
            } else {
                "discovered"
            }
        );
    }
    Ok(())
}

fn local_state(data_dir: Option<PathBuf>, port: Option<u16>) -> Result<(AppConfig, Database)> {
    let config = AppConfig::resolve(data_dir, port)?;
    config.ensure_directories()?;
    let db = Database::open(config.database_path())?;
    Ok((config, db))
}

fn init_tracing(json_logs: bool) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("printlatch=info,tower_http=warn"));
    if json_logs {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .with_current_span(false)
            .with_span_list(false)
            .try_init()
            .map_err(|error| anyhow::anyhow!("tracing is already initialized: {error}"))?;
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .try_init()
            .map_err(|error| anyhow::anyhow!("tracing is already initialized: {error}"))?;
    }
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(error = %error, "could not install shutdown handler");
    }
}
