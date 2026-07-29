use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use printlatch::{
    PRODUCT_NAME, VERSION,
    api::{AppState, router},
    auth,
    config::AppConfig,
    db::Database,
    printers,
    worker::{self, QueueSignal},
};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

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
    /// Open the local operator dashboard with a one-time pairing grant.
    Dashboard(DashboardArgs),
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
struct DashboardArgs {
    #[arg(long, env = "PRINTLATCH_PORT")]
    port: Option<u16>,
    /// Print the URL without opening the default browser.
    #[arg(long)]
    no_open: bool,
}

#[derive(Args)]
struct PairArgs {
    #[arg(long)]
    origin: String,
    #[arg(long, default_value = "Browser app")]
    name: String,
}

#[derive(Deserialize)]
struct InstanceIdentity {
    product: String,
    version: String,
    session: String,
    proof: String,
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
        Command::Dashboard(arguments) => dashboard(cli.data_dir, &arguments),
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

fn dashboard(data_dir: Option<PathBuf>, arguments: &DashboardArgs) -> Result<()> {
    let (config, db) = local_state(data_dir, arguments.port)?;
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.port);
    let instance_session = verify_dashboard_agent(&db, address)?;
    let origin = format!("http://127.0.0.1:{}", config.port);
    let grant =
        auth::new_dashboard_pairing_grant(&db, &origin, "PrintLatch dashboard", &instance_session)?;
    let url = format!("{origin}/app/#code={}", grant.code);
    println!("Dashboard URL: {url}");
    println!("Pairing expires at (Unix): {}", grant.expires_at);
    if !arguments.no_open {
        open_dashboard(&url)?;
        println!("Opened the dashboard in your default browser.");
    }
    Ok(())
}

fn verify_dashboard_agent(db: &Database, address: SocketAddr) -> Result<String> {
    let challenge = Uuid::new_v4().simple().to_string();
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_millis(750)).with_context(|| {
            format!(
                "PrintLatch is not reachable at http://{address}. Start the agent with `printlatch serve` and try again"
            )
        })?;
    stream.set_read_timeout(Some(Duration::from_millis(750)))?;
    stream.set_write_timeout(Some(Duration::from_millis(750)))?;
    let request = format!(
        "GET /health/instance?challenge={challenge} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .context("could not verify the PrintLatch agent")?;
    let mut response = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream
            .read(&mut chunk)
            .context("could not verify the PrintLatch agent")?;
        if read == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..read]);
        ensure!(
            response.len() <= 8 * 1024,
            "listener response was too large to be PrintLatch"
        );
    }
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("listener did not return a valid HTTP response")?;
    let headers = std::str::from_utf8(&response[..separator])
        .context("listener returned invalid HTTP headers")?;
    ensure!(
        headers.lines().next().is_some_and(|line| {
            line.starts_with("HTTP/1.1 200 ") || line.starts_with("HTTP/1.0 200 ")
        }),
        "listener is not the expected PrintLatch agent"
    );
    let identity: InstanceIdentity = serde_json::from_slice(&response[separator + 4..])
        .context("listener returned an invalid PrintLatch identity")?;
    ensure!(
        identity.product == PRODUCT_NAME && identity.version == VERSION,
        "listener is not the expected PrintLatch version"
    );
    ensure!(
        auth::valid_agent_session(&identity.session),
        "listener returned an invalid PrintLatch session"
    );
    let expected = auth::instance_proof(db, &challenge, &identity.session)?;
    if !auth::verify_instance_proof(&expected, &identity.proof) {
        bail!("listener could not prove it is this PrintLatch installation");
    }
    Ok(identity.session)
}

#[cfg(windows)]
fn open_dashboard(url: &str) -> Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .context("could not open the default browser")?;
    Ok(())
}

#[cfg(not(windows))]
fn open_dashboard(_url: &str) -> Result<()> {
    println!("Open the dashboard URL in a browser on this machine.");
    Ok(())
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
    let instance_session = auth::new_agent_session()?;
    let state = AppState {
        config: config.clone(),
        db: db.clone(),
        queue: queue.clone(),
        instance_session,
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

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, sync::Arc, thread};

    use super::*;

    #[test]
    fn rejects_a_listener_without_the_installation_proof() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("fake listener");
        let address = listener.local_addr().expect("listener address");
        let session = auth::new_agent_session().expect("agent session");
        let response_body = serde_json::json!({
            "product": PRODUCT_NAME,
            "version": VERSION,
            "session": session,
            "proof": "00".repeat(32),
        })
        .to_string();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("fake connection");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).expect("fake request");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("fake response");
        });
        let temp = tempfile::tempdir().expect("temporary data");
        let db = Database::open(temp.path().join("printlatch.sqlite3")).expect("database");
        let error = verify_dashboard_agent(&db, address).expect_err("spoofed listener must fail");
        assert!(
            error
                .to_string()
                .contains("could not prove it is this PrintLatch installation")
        );
        worker.join().expect("fake listener thread");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn accepts_the_running_agent_installation_and_session() {
        let temp = tempfile::tempdir().expect("temporary data");
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("agent listener");
        let address = listener.local_addr().expect("agent address");
        let config = AppConfig::resolve(Some(temp.path().to_path_buf()), Some(address.port()))
            .expect("configuration");
        config.ensure_directories().expect("directories");
        let db = Database::open(config.database_path()).expect("database");
        let session = auth::new_agent_session().expect("agent session");
        let state = AppState {
            config,
            db: db.clone(),
            queue: QueueSignal::new(),
            instance_session: session.clone(),
        };
        let server = tokio::spawn(async move {
            axum::serve(listener, router(state))
                .await
                .expect("agent server");
        });
        let verified = tokio::task::spawn_blocking({
            let db = Arc::new(db);
            move || verify_dashboard_agent(&db, address)
        })
        .await
        .expect("verification task")
        .expect("verified agent");
        assert_eq!(verified, session);
        server.abort();
    }
}
