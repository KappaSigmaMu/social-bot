use crate::chain::{ChainData, Society};
use crate::messages::{candidates_message, period_message};
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn handle_command<C>(
    society: &Society<C>,
    prefix: &str,
    sender: &str,
    text: &str,
    server_timestamp_ms: Option<u64>,
) -> Result<Option<String>>
where
    C: ChainData,
{
    let Some(command_text) = text.strip_prefix(prefix) else {
        return Ok(None);
    };

    let mut parts = command_text.split_whitespace();
    let Some(command) = parts.next() else {
        return Ok(None);
    };

    let response = match command {
        "ping" => ping_response(server_timestamp_ms),
        "defender" => {
            let defender = society.get_defending().await?;
            match defender.address_or_handle {
                Some(address) => format!(
                    "The current defender is {address}. So far they have {} approvals and {} rejections.",
                    defender.tally.approvals, defender.tally.rejections
                ),
                None => "There is no defender".to_owned(),
            }
        }
        "info" => {
            let Some(address) = parts.next() else {
                return Ok(Some("Usage: `!info <address>`".to_owned()));
            };
            let info = society.get_member_info(address).await?;
            format!(
                "* **Address**: {}\n* **State**: {}\n* **Element_handle**: {}\n* **Strikes**: {}\n* **Is_founder**: {}\n* **Is_defender**: {}\n",
                info.address,
                info.state,
                info.element_handle.as_deref().unwrap_or("None"),
                info.strikes,
                info.is_founder,
                info.is_defender,
            )
        }
        "candidates" => {
            let candidates = society.get_candidates().await?;
            if let Some(address) = parts.next() {
                match candidates.iter().find(|candidate| candidate.address_or_handle == address) {
                    Some(candidate) => candidates_message(std::slice::from_ref(candidate)),
                    None => format!("No candidate with address `{address}`"),
                }
            } else {
                candidates_message(&candidates)
            }
        }
        "head" => match society.get_head_address().await? {
            Some(head) => format!("The current head is `{head}`"),
            None => "There is no head, something must have gone horribly wrong".to_owned(),
        },
        "set_address" => {
            let Some(address) = parts.next() else {
                return Ok(Some("Usage: `!set_address <address>`".to_owned()));
            };
            if society.set_matrix_handle(address, sender)? {
                format!("Set matrix handle {sender} for address `{address}`")
            } else {
                format!("Failed to set matrix handle {sender} for address `{address}`")
            }
        }
        "unset_address" => {
            if society.unset_matrix_handle(sender)? {
                format!("Unset address for {sender}")
            } else {
                format!("Failed to unset address for {sender}")
            }
        }
        "me" => match society.get_address_for_matrix_handle(sender)? {
            Some(address) => {
                let info = society.get_member_info(&address).await?;
                format!(
                    "* **Address**: {}\n* **State**: {}\n* **Element_handle**: {}\n* **Strikes**: {}\n* **Is_founder**: {}\n* **Is_defender**: {}\n",
                    info.address,
                    info.state,
                    info.element_handle.as_deref().unwrap_or("None"),
                    info.strikes,
                    info.is_founder,
                    info.is_defender,
                )
            }
            None => "You have not set your address yet. To do so, use `!set_address <address>`. Note that the !me command does not currently support addresses with an on-chain identity set.".to_owned(),
        },
        "period" => {
            let period = society.get_candidate_period().await?;
            let defender = society.get_defending().await?;
            let candidates = society.get_candidates().await?;
            let head = society.get_head_address().await?;
            let candidate_skeptic = society.get_candidate_skeptic().await?;
            period_message(
                &period,
                &defender,
                &candidates,
                head.as_deref(),
                candidate_skeptic.as_deref(),
                false,
            )
        }
        "skeptics" | "skeptic" => {
            let defender = society.get_defending().await?;
            let candidate_skeptic = society.get_candidate_skeptic().await?;
            let mut message = String::new();
            match defender.skeptic {
                Some(skeptic) => {
                    message.push_str(&format!("The current skeptic for the defender is {skeptic}\n\n"));
                }
                None => message.push_str("There is no skeptic for the current defender.\n\n"),
            }
            match candidate_skeptic {
                Some(skeptic) => {
                    message.push_str(&format!("The current skeptic for the candidates is {skeptic}"));
                }
                None => message.push_str("There is no skeptic for the current candidates."),
            }
            message
        }
        _ => return Ok(None),
    };

    Ok(Some(response))
}

fn ping_response(server_timestamp_ms: Option<u64>) -> String {
    let Some(server_timestamp_ms) = server_timestamp_ms else {
        return "Pong!".to_owned();
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let roundtrip = now.saturating_sub(server_timestamp_ms);
    format!("Pong! Took {roundtrip}ms")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::tests::FakeChain;
    use crate::models::{Candidate, Tally};
    use crate::store::OverrideStore;
    use tempfile::NamedTempFile;

    const MEMBER: &str = "FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1";

    fn test_society(chain: FakeChain) -> Society<FakeChain> {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        Society::new(chain, OverrideStore::open(path).unwrap())
    }

    #[tokio::test]
    async fn handles_set_and_me_commands() {
        let mut chain = FakeChain::default();
        chain.members.insert(MEMBER.to_owned());
        let society = test_society(chain);

        let response = handle_command(
            &society,
            "!",
            "@testuser:matrix.org",
            &format!("!set_address {MEMBER}"),
            None,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(response.contains("Set matrix handle"));

        let response = handle_command(&society, "!", "@testuser:matrix.org", "!me", None)
            .await
            .unwrap()
            .unwrap();
        assert!(response.contains("* **State**: member"));
    }

    #[tokio::test]
    async fn filters_single_candidate() {
        let mut chain = FakeChain::default();
        chain.candidates.push(Candidate {
            address_or_handle: MEMBER.to_owned(),
            bid_plancks: 2_000_000_000_000,
            tally: Tally {
                approvals: 7,
                rejections: 3,
            },
        });
        let society = test_society(chain);

        let response = handle_command(
            &society,
            "!",
            "@testuser:matrix.org",
            &format!("!candidates {MEMBER}"),
            None,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(response.contains("Approvals: 7"));
        assert!(response.contains("Bid: 2 KSM"));
    }
}
