use clap::Parser;
use std::path::PathBuf;
use zeroclaw::Config;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    project_root: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 42618)]
    port: u16,
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
    apply_core_token(&mut config, args.core_token);
    zeroclaw_gateway::grpc::run_grpc_server(&args.host, args.port, config).await
}

fn apply_core_token(config: &mut Config, token: Option<String>) {
    let Some(token) = token.map(|token| token.trim().to_string()) else {
        return;
    };
    if token.is_empty() {
        return;
    }

    config.gateway.require_pairing = true;
    config.gateway.paired_tokens = vec![token];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_token_registers_grpc_bearer_token() {
        let mut config = Config::default();
        config.gateway.require_pairing = false;
        config.gateway.paired_tokens.clear();

        apply_core_token(&mut config, Some(" zc_core_test ".to_string()));

        assert!(config.gateway.require_pairing);
        assert_eq!(config.gateway.paired_tokens, vec!["zc_core_test"]);
    }

    #[test]
    fn missing_core_token_preserves_existing_pairing_config() {
        let mut config = Config::default();
        config.gateway.require_pairing = true;
        config.gateway.paired_tokens = vec!["zc_existing".to_string()];

        apply_core_token(&mut config, None);

        assert!(config.gateway.require_pairing);
        assert_eq!(config.gateway.paired_tokens, vec!["zc_existing"]);
    }
}
