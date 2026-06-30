use crate::chain::{ChainData, Society};
use crate::messages::{
    candidate_not_found_message, candidates_message, defender_message, head_message,
    member_info_message, period_message, skeptics_message,
};
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
                Some(_) => defender_message(&defender, true),
                None => "**Defender:** none".to_owned(),
            }
        }
        "info" => {
            let Some(address) = parts.next() else {
                return Ok(Some("Usage: `!info <address>`".to_owned()));
            };
            let info = society.get_member_info(address).await?;
            member_info_message(&info, true)
        }
        "candidates" => {
            let candidates = society.get_candidates().await?;
            if let Some(address) = parts.next() {
                match candidates.iter().find(|candidate| candidate.address_or_handle == address) {
                    Some(candidate) => candidates_message(std::slice::from_ref(candidate)),
                    None => candidate_not_found_message(address),
                }
            } else {
                candidates_message(&candidates)
            }
        }
        "head" => head_message(society.get_head_display().await?.as_deref()),
        "set_address" => {
            let Some(address) = parts.next() else {
                return Ok(Some("Usage: `!set_address <address>`".to_owned()));
            };
            if society.set_matrix_handle(address, sender)? {
                format!("Linked `{address}` to {sender}.")
            } else {
                format!("Could not link `{address}` to {sender}.")
            }
        }
        "unset_address" => {
            if society.unset_matrix_handle(sender)? {
                format!("Removed address link for {sender}.")
            } else {
                format!("No address link to remove for {sender}.")
            }
        }
        "me" => match society.get_address_for_matrix_handle(sender)? {
            Some(address) => {
                let info = society.get_member_info(&address).await?;
                member_info_message(&info, false)
            }
            None => "No address linked yet. Use `!set_address <address>`.\nOn-chain identities are not supported by `!me`.".to_owned(),
        },
        "period" => {
            let period = society.get_candidate_period().await?;
            let defender = society.get_defending().await?;
            let candidates = society.get_candidates().await?;
            let head = society.get_head_display().await?;
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
            skeptics_message(
                defender.skeptic.as_deref(),
                candidate_skeptic.as_deref(),
            )
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
    format!("Pong! ({roundtrip} ms)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::tests::FakeChain;
    use crate::models::{Candidate, Tally};
    use crate::store::OverrideStore;
    use tempfile::NamedTempFile;

    const MEMBER: &str = "FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1";
    const CANDIDATE: &str = "G75yJUM2TveDikvysHHW5XhkP35gXqDAsgRLYQTh3gVDir9";
    const DEFENDER: &str = "DGE8ATd2NaitqX4jdvZNXFNMmY9Qui6swnfoheCiz7efWGG";

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
        assert!(response.contains("Linked"));

        let response = handle_command(&society, "!", "@testuser:matrix.org", "!me", None)
            .await
            .unwrap()
            .unwrap();
        assert!(response.contains("· Status: member"));
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
        assert!(response.contains("7 approvals"));
        assert!(response.contains("2 KSM"));
    }

    #[tokio::test]
    async fn ignores_non_commands_empty_commands_and_unknown_commands() {
        let society = test_society(FakeChain::default());

        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "hello", None)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!", None)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!unknown", None)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn handles_usage_and_failure_branches() {
        let society = test_society(FakeChain::default());

        assert_eq!(
            handle_command(&society, "!", "@testuser:matrix.org", "!info", None)
                .await
                .unwrap()
                .unwrap(),
            "Usage: `!info <address>`"
        );
        assert_eq!(
            handle_command(&society, "!", "@testuser:matrix.org", "!set_address", None)
                .await
                .unwrap()
                .unwrap(),
            "Usage: `!set_address <address>`"
        );
        assert!(
            handle_command(
                &society,
                "!",
                "@testuser:matrix.org",
                "!set_address not-an-address",
                None,
            )
            .await
            .unwrap()
            .unwrap()
            .contains("Could not link")
        );
        assert!(
            handle_command(
                &society,
                "!",
                "@testuser:matrix.org",
                "!unset_address",
                None
            )
            .await
            .unwrap()
            .unwrap()
            .contains("No address link to remove")
        );
        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!me", None)
                .await
                .unwrap()
                .unwrap()
                .contains("No address linked yet")
        );
    }

    #[tokio::test]
    async fn handles_chain_backed_read_commands() {
        let mut chain = FakeChain::default();
        chain.members.insert(MEMBER.to_owned());
        chain.candidates.push(Candidate {
            address_or_handle: CANDIDATE.to_owned(),
            bid_plancks: 3_250_000_000_000,
            tally: Tally {
                approvals: 9,
                rejections: 4,
            },
        });
        chain.defender = Some(DEFENDER.to_owned());
        chain.defender_skeptic = Some(MEMBER.to_owned());
        chain.candidate_skeptic = Some(CANDIDATE.to_owned());
        chain.head = Some(MEMBER.to_owned());
        chain.founder = Some(MEMBER.to_owned());
        chain.block_number = 1;
        chain.strikes.insert(MEMBER.to_owned(), 5);
        chain
            .identities
            .insert(MEMBER.to_owned(), "@member:matrix.org".to_owned());
        let society = test_society(chain);

        let ping = handle_command(&society, "!", "@testuser:matrix.org", "!ping", Some(0))
            .await
            .unwrap()
            .unwrap();
        assert!(ping.starts_with("Pong! ("));

        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!defender", None)
                .await
                .unwrap()
                .unwrap()
                .contains("1 approvals · 2 rejections")
        );
        assert_eq!(
            handle_command(&society, "!", "@testuser:matrix.org", "!head", None)
                .await
                .unwrap()
                .unwrap(),
            format!("**Head:** `{MEMBER} (@member:matrix.org)`")
        );
        assert!(
            handle_command(
                &society,
                "!",
                "@testuser:matrix.org",
                &format!("!info {MEMBER}"),
                None,
            )
            .await
            .unwrap()
            .unwrap()
            .contains("· Strikes: 5")
        );
        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!period", None)
                .await
                .unwrap()
                .unwrap()
                .contains("**Voting**")
        );
        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!skeptics", None)
                .await
                .unwrap()
                .unwrap()
                .contains("**Defender skeptic:**")
        );
        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!skeptic", None)
                .await
                .unwrap()
                .unwrap()
                .contains("**Candidate skeptic:**")
        );
    }

    #[tokio::test]
    async fn handles_absent_chain_values() {
        let society = test_society(FakeChain::default());

        assert_eq!(
            handle_command(&society, "!", "@testuser:matrix.org", "!defender", None)
                .await
                .unwrap()
                .unwrap(),
            "**Defender:** none"
        );
        assert_eq!(
            handle_command(&society, "!", "@testuser:matrix.org", "!head", None)
                .await
                .unwrap()
                .unwrap(),
            "**Head:** none"
        );
        assert!(
            handle_command(&society, "!", "@testuser:matrix.org", "!skeptics", None)
                .await
                .unwrap()
                .unwrap()
                .contains("none")
        );
        assert!(
            handle_command(
                &society,
                "!",
                "@testuser:matrix.org",
                &format!("!candidates {CANDIDATE}"),
                None,
            )
            .await
            .unwrap()
            .unwrap()
            .contains("No candidate matching")
        );
    }
}
