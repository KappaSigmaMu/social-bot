use anyhow::{Context, Result, anyhow};
use std::env;
use subxt::{OnlineClient, PolkadotConfig, dynamic};
use subxt_signer::sr25519::dev;

const DEFAULT_RPC_URL: &str = "ws://127.0.0.1:8000";
const DEFAULT_SIGNER: &str = "dave";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
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
    let api = if rpc_url.starts_with("ws://") || rpc_url.starts_with("http://") {
        OnlineClient::<PolkadotConfig>::from_insecure_url(&rpc_url).await
    } else {
        OnlineClient::<PolkadotConfig>::from_url(&rpc_url).await
    }
    .with_context(|| format!("connecting to Chopsticks RPC {rpc_url}"))?;

    let at_block = api.at_current_block().await?;
    let current_block = at_block.block_number();
    let tx_payload = dynamic::tx("Society", "unbid", Vec::<subxt::dynamic::Value>::new());

    let _events = at_block
        .transactions()
        .sign_and_submit_then_watch_default(&tx_payload, &signer)
        .await?
        .wait_for_finalized_success()
        .await?;

    println!("Submitted Society unbid at block {current_block} from {signer_label}");
    Ok(())
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
