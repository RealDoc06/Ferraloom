//! Ferraloom dedicated server entry point (Main#main).
use anyhow::{Context, Result, bail};
use ferraloom_server::server::{MinecraftServer, ServerOptions};
#[tokio::main]
async fn main() -> Result<()> {
    let mut options = ServerOptions::new(std::env::current_dir()?);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "nogui" | "--nogui" => {}
            "--universe" => {
                options.root = args.next().context("--universe needs a directory")?.into()
            }
            "--port" => options.port = Some(args.next().context("--port needs a number")?.parse()?),
            "--host" => options.ip = Some(args.next().context("--host needs an address")?),
            "--cache" => options.cache = args.next().context("--cache needs a directory")?.into(),
            "--regions" => {
                if args.next().as_deref() != Some("off") {
                    bail!("Only --regions off is implemented");
                }
            }
            "--help" | "-h" => {
                println!(
                    "ferraloom [nogui] [--universe DIR] [--cache DIR] [--host IP] [--port PORT] [--regions off]"
                );
                return Ok(());
            }
            _ => bail!("Unknown option: {arg}"),
        }
    }
    let server = MinecraftServer::start(options).await?;
    println!("Listening on {}", server.address);
    let ingress = server.ingress.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = ingress
                .send(ferraloom_server::network::Ingress::Console("stop".into()))
                .await;
        }
    });
    server.wait().await
}
