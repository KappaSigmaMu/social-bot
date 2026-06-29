use anyhow::{Context, Result, anyhow};
use std::env;
use subxt::dynamic::Value;
use subxt::{OnlineClient, PolkadotConfig, dynamic};
use subxt_signer::sr25519::dev;

const DEFAULT_RPC_URL: &str = "ws://127.0.0.1:8000";
const DEFAULT_BID_PLANCKS: u128 = 350_000_000_000_000;
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
    let bid_plancks = args
        .get(2)
        .map(|value| value.parse::<u128>())
        .transpose()
        .context("parsing bid value")?
        .or(match env::var("SOCIETY_BID_PLANCKS") {
            Ok(value) => Some(
                value
                    .parse::<u128>()
                    .context("parsing SOCIETY_BID_PLANCKS")?,
            ),
            Err(_) => None,
        })
        .unwrap_or(DEFAULT_BID_PLANCKS);

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
    let tx_payload = dynamic::tx("Society", "bid", vec![Value::u128(bid_plancks)]);

    let _events = at_block
        .transactions()
        .sign_and_submit_then_watch_default(&tx_payload, &signer)
        .await?
        .wait_for_finalized_success()
        .await?;

    println!(
        "Submitted Society bid at or after block {current_block} from {signer_label} for {} KSM ({bid_plancks} plancks)",
        format_ksm(bid_plancks)
    );
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

fn format_ksm(plancks: u128) -> String {
    const KSM_DIVISOR: u128 = 1_000_000_000_000;
    let whole = plancks / KSM_DIVISOR;
    let fraction = plancks % KSM_DIVISOR;
    if fraction == 0 {
        return whole.to_string();
    }
    let fraction = format!("{fraction:012}");
    format!("{whole}.{}", fraction.trim_end_matches('0'))
}
