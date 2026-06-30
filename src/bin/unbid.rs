use anyhow::{Context, Result, anyhow};
use std::env;
use subxt::tx::DefaultParams;
use subxt::{OnlineClient, PolkadotConfig, dynamic};
use subxt_rpcs::RpcClient;
use subxt_rpcs::client::rpc_params;
use subxt_signer::sr25519::dev;

const DEFAULT_RPC_URL: &str = "ws://127.0.0.1:8000";
const DEFAULT_SIGNER: &str = "dave";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    let rpc_url = args
        .first()
        .cloned()
        .or_else(|| env::var("CHOPSTICKS_RPC_URL").ok())
        .unwrap_or_else(|| DEFAULT_RPC_URL.to_owned());
    let signer_name = args
        .get(1)
        .cloned()
        .or_else(|| env::var("CHOPSTICKS_BIDDER").ok())
        .unwrap_or_else(|| DEFAULT_SIGNER.to_owned());

    let signer = dev_account(&signer_name)?;
    let signer_label = signer_label(&signer_name);
    let api = connect_subxt(&rpc_url).await?;
    let rpc = connect_rpc(&rpc_url).await?;

    let at_block = api.at_current_block().await?;
    let current_block = at_block.block_number();
    let tx_payload = dynamic::tx("Society", "unbid", Vec::<subxt::dynamic::Value>::new());

    let tx = at_block
        .transactions()
        .create_signed(&tx_payload, &signer, DefaultParams::default_params())
        .await?;
    let tx_hash: String = rpc
        .request(
            "author_submitExtrinsic",
            rpc_params![format!("0x{}", hex::encode(tx.encoded()))],
        )
        .await
        .context("submitting extrinsic through legacy author_submitExtrinsic")?;

    println!(
        "Submitted Society unbid transaction from {signer_label} at block {current_block}; tx hash: {tx_hash}"
    );
    Ok(())
}

async fn connect_subxt(rpc_url: &str) -> Result<OnlineClient<PolkadotConfig>> {
    if rpc_url.starts_with("ws://") || rpc_url.starts_with("http://") {
        OnlineClient::<PolkadotConfig>::from_insecure_url(rpc_url).await
    } else {
        OnlineClient::<PolkadotConfig>::from_url(rpc_url).await
    }
    .with_context(|| format!("connecting to Chopsticks RPC {rpc_url}"))
}

async fn connect_rpc(rpc_url: &str) -> Result<RpcClient> {
    if rpc_url.starts_with("ws://") || rpc_url.starts_with("http://") {
        RpcClient::from_insecure_url(rpc_url).await
    } else {
        RpcClient::from_url(rpc_url).await
    }
    .with_context(|| format!("connecting legacy RPC client to {rpc_url}"))
}

fn dev_account(name: &str) -> Result<subxt_signer::sr25519::Keypair> {
    match name.to_ascii_lowercase().as_str() {
        "alice" => Ok(dev::alice()),
        "bob" => Ok(dev::bob()),
        "charlie" => Ok(dev::charlie()),
        "dave" => Ok(dev::dave()),
        "eve" => Ok(dev::eve()),
        "ferdie" => Ok(dev::ferdie()),
        other => Err(anyhow!(
            "unsupported dev signer '{other}'; expected one of: alice, bob, charlie, dave, eve, ferdie"
        )),
    }
}

fn signer_label(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "alice" => "Alice",
        "bob" => "Bob",
        "charlie" => "Charlie",
        "dave" => "Dave",
        "eve" => "Eve",
        "ferdie" => "Ferdie",
        _ => "Unknown",
    }
}

fn print_help() {
    println!(
        "Usage: cargo society:unbid [rpc-url] [signer]\n\nDefaults:\n  rpc-url: {DEFAULT_RPC_URL}\n  signer: {DEFAULT_SIGNER}\n\nEnvironment overrides:\n  CHOPSTICKS_RPC_URL\n  CHOPSTICKS_BIDDER"
    );
}
