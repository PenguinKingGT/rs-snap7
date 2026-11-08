//! Command-line S7 memory tools and optional local emulator/demo.

#[path = "s7/value.rs"]
mod value;
use clap::{Args, Parser, Subcommand};
use rs_snap7::{Client, ClientConfig, Error, Result, tags::Tag};
use std::{
    io::{self, Write},
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Read, write, inspect and emulate classic Siemens S7.
#[derive(Parser)]
#[command(name = "s7", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
/// Shared connection flags.
#[derive(Args)]
struct ConnectionArgs {
    /// PLC hostname or IP.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// TCP port.
    #[arg(long, default_value_t = 102)]
    port: u16,
    /// Rack, 0–7.
    #[arg(long, default_value_t = 0)]
    rack: u8,
    /// Slot, 0–31.
    #[arg(long, default_value_t = 1)]
    slot: u8,
    /// Per-operation timeout in milliseconds.
    #[arg(long, default_value_t = 5000)]
    timeout_ms: u64,
}
impl ConnectionArgs {
    /// Resolve a chosen host and validate settings before connecting.
    async fn connect(&self) -> Result<Client> {
        let endpoint = tokio::net::lookup_host((&*self.host, self.port))
            .await?
            .next()
            .ok_or(Error::InvalidInput("host resolved to no addresses"))?;
        let mut config = ClientConfig::new(endpoint, self.rack, self.slot)?;
        config.transport.timeout = Duration::from_millis(self.timeout_ms);
        Client::connect(config).await
    }
}
/// Supported commands; typed addresses use explicit colon/comma syntax.
#[derive(Subcommand)]
enum Command {
    /// Read one typed tag, printing a JSON value.
    Read {
        #[command(flatten)]
        connection: ConnectionArgs,
        /// Typed address, for example DB1:0:INT or DB1,R4.
        tag: String,
    },
    /// Validate and write one typed value. BOOL neighbors are preserved.
    Write {
        #[command(flatten)]
        connection: ConnectionArgs,
        /// Typed address.
        tag: String,
        /// Scalar text, or a JSON array for array tags.
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// Dump DB bytes as hex, or save raw bytes to a file.
    Dump {
        #[command(flatten)]
        connection: ConnectionArgs,
        #[arg(long)]
        db: u16,
        #[arg(long, default_value_t = 0)]
        start: u32,
        /// Byte length; omitted means discover the DB size.
        #[arg(long)]
        length: Option<usize>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Print CPU, module and negotiated connection information as JSON.
    Info {
        #[command(flatten)]
        connection: ConnectionArgs,
    },
    /// Start a local emulator with one registered DB.
    Server {
        #[arg(long, default_value = "127.0.0.1:1102")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 1)]
        db: u16,
        #[arg(long, default_value_t = 1024)]
        size: usize,
        /// Stop after this duration, or run until Ctrl-C when omitted.
        #[arg(long)]
        duration_ms: Option<u64>,
    },
    /// Identify PROFINET devices using a local interface IPv4.
    Discover {
        #[arg(long)]
        interface: Ipv4Addr,
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u64,
    },
    /// Serve live host metrics in DB1 and writable controls in DB2.
    #[cfg(feature = "demo")]
    Demo {
        #[arg(long, default_value = "127.0.0.1:1102")]
        bind: SocketAddr,
        #[arg(long)]
        duration_ms: Option<u64>,
    },
}
/// Install one process-wide stop flag; handlers only signal and never block.
fn stop_flag() -> Result<Arc<AtomicBool>> {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    ctrlc::set_handler(move || {
        flag.store(false, Ordering::Release);
    })
    .map_err(|error| Error::Io(io::Error::other(error)))?;
    Ok(running)
}
/// Flush readiness before entering a bounded-duration or Ctrl-C service loop.
fn run_service(
    endpoint: SocketAddr,
    duration_ms: Option<u64>,
    mut tick: impl FnMut() -> Result<()>,
) -> Result<()> {
    let running = stop_flag()?;
    println!("Listening on {endpoint}");
    io::stdout().flush()?;
    let started = Instant::now();
    while running.load(Ordering::Acquire)
        && duration_ms.is_none_or(|ms| started.elapsed() < Duration::from_millis(ms))
    {
        tick()?;
        let wait = duration_ms
            .map(|ms| Duration::from_millis(ms).saturating_sub(started.elapsed()))
            .unwrap_or(Duration::from_millis(250))
            .min(Duration::from_millis(250));
        std::thread::sleep(wait);
    }
    Ok(())
}
/// Execute one command, validating complete values before PLC connection.
async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Read { connection, tag } => {
            let tag: Tag = tag.parse()?;
            let client = connection.connect().await?;
            println!("{}", value::json(&client.read_tag(&tag).await?));
            client.disconnect().await
        }
        Command::Write {
            connection,
            tag,
            value,
        } => {
            let tag: Tag = tag.parse()?;
            let value = value::parse(&tag, &value)?;
            tag.encode(&value)?;
            let client = connection.connect().await?;
            client.write_tag(&tag, &value).await?;
            client.disconnect().await
        }
        Command::Dump {
            connection,
            db,
            start,
            length,
            output,
        } => {
            let client = connection.connect().await?;
            let bytes = match length {
                Some(length) => client.db_read(db, start, length).await?,
                None => {
                    let bytes = client.db_get(db, None).await?;
                    bytes
                        .get(start as usize..)
                        .ok_or(Error::InvalidInput("dump start exceeds DB"))?
                        .to_vec()
                }
            };
            if let Some(path) = output {
                std::fs::write(path, bytes)?;
            } else {
                for (line, bytes) in bytes.chunks(16).enumerate() {
                    print!("{:08x}:", start as usize + line * 16);
                    for byte in bytes {
                        print!(" {byte:02x}");
                    }
                    println!();
                }
            }
            client.disconnect().await
        }
        Command::Info { connection } => {
            let client = connection.connect().await?;
            let state = client.get_cpu_state().await?;
            let cpu = client.get_cpu_info().await?;
            println!(
                "{}",
                serde_json::json!({"state":format!("{state:?}"),"module":cpu.module_name,"serial":cpu.serial_number,"pdu_length":client.pdu_length().await?})
            );
            client.disconnect().await
        }
        Command::Server {
            bind,
            db,
            size,
            duration_ms,
        } => {
            if size == 0 || size > 0x200000 {
                return Err(Error::InvalidInput("server DB size out of range"));
            }
            blocking_job(move || {
            let server =
                rs_snap7::server::Server::start(rs_snap7::server::ServerConfig::new(bind))?;
            server.register_area(rs_snap7::Area::Db, db, vec![0; size])?;
            run_service(server.local_addr(), duration_ms, || Ok(()))
            }).await
        }
        Command::Discover {
            interface,
            timeout_ms,
        } => {
            blocking_job(move || {
            let config = rs_snap7::discovery::DiscoveryConfig {
                timeout: Duration::from_millis(timeout_ms),
                ..Default::default()
            };
            for device in rs_snap7::discovery::discover(interface, config)? {
                println!(
                    "{}",
                    serde_json::json!({"name":device.name,"mac":device.mac.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":"),"ip":device.ip.map(|v| v.to_string()),"family":device.family})
                );
            }
            Ok(())
            }).await
        }
        #[cfg(feature = "demo")]
        Command::Demo { bind, duration_ms } => {
            blocking_job(move || {
            let mut demo = rs_snap7::demo::Demo::start(rs_snap7::server::ServerConfig::new(bind))?;
            run_service(demo.server().local_addr(), duration_ms, || demo.sample())
            }).await
        }
    }
}
/// Report errors to stderr and return nonzero without a panic.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// Isolate synchronous service loops and discovery from the async executor.
async fn blocking_job<T: Send + 'static>(
    job: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|_| Error::Protocol("blocking command worker failed"))?
}
