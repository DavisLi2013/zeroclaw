use clap::Parser;
use std::path::PathBuf;
use zeroclaw::Config;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    project_root: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 42617)]
    port: u16,
    #[arg(long)]
    core_grpc: String,
    #[arg(long)]
    core_token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    zeroclaw::init_cli_tracing();
    let args = Args::parse();
    let mut config = Box::pin(Config::load_or_init()).await?;
    config.apply_env_overrides();
    zeroclaw::apply_runtime_project_root(&mut config, &args.project_root)?;
    config.gateway.core.endpoint = Some(args.core_grpc);
    config.gateway.core.bearer_token = args.core_token;
    zeroclaw_gateway::run_gateway(&args.host, args.port, config, None, None, None).await
}
