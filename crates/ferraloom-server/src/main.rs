//! Ferraloom dedicated server entry point (Main#main).
use anyhow::{Context, Result, bail};
use ferraloom_game::block::UnportedBehavior;
use ferraloom_server::server::{MinecraftServer, ServerOptions};
#[tokio::main]
async fn main() -> Result<()> {
    let mut options = ServerOptions::new(std::env::current_dir()?);
    options.unported_behavior = UnportedBehavior::Warn;
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
            "--unported-behavior" => {
                options.unported_behavior = match args.next().as_deref() {
                    Some("strict") => UnportedBehavior::Strict,
                    Some("warn") => UnportedBehavior::Warn,
                    _ => bail!("--unported-behavior needs strict or warn"),
                };
            }
            "--regions" => {
                if args.next().as_deref() != Some("off") {
                    bail!("Only --regions off is implemented");
                }
            }
            "--help" | "-h" => {
                println!(
                    "ferraloom [nogui] [--universe DIR] [--cache DIR] [--host IP] [--port PORT] [--regions off] [--unported-behavior strict|warn]"
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
