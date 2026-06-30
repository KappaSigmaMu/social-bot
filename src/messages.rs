use crate::models::{Bid, Candidate, CandidatePeriod, CandidatePeriodKind, Defender, MemberInfo};
use std::time::Duration;

const KSM_DIVISOR: u128 = 1_000_000_000_000;

pub fn format_address_with_handle(address: &str, handle: Option<&str>) -> String {
    match handle {
        Some(handle) if handle != address => format!("{address} ({handle})"),
        _ => address.to_owned(),
    }
}

pub fn candidates_message(candidates: &[Candidate]) -> String {
    let mut message = String::from("**Candidates**\n");
    match candidates {
        [] => message.push_str("None."),
        candidates => {
            for candidate in candidates {
                message.push_str(&candidate_line(candidate));
            }
        }
    }
    message
}

pub fn period_message(
    candidate_period: &CandidatePeriod,
    defender_info: &Defender,
    candidates: &[Candidate],
    head: Option<&str>,
    candidate_skeptic: Option<&str>,
    new_period: bool,
) -> String {
    let mut message = String::new();
    if new_period {
        message.push_str("**New voting period started**\n\n");
    }

    match candidate_period.kind {
        CandidatePeriodKind::Voting => {
            message.push_str(&format!(
                "**Voting**\nCandidates submit ink proofs. Members vote on candidates.\n{} blocks remaining ({}).\n\n",
                candidate_period.voting_blocks_left,
                format_duration(candidate_period.voting_time_left()),
            ));
            message.push_str(&candidates_message(candidates));
            message.push('\n');
            message.push_str(&head_message(head));
        }
        CandidatePeriodKind::Claim => {
            message.push_str(&format!(
                "**Claim period**\nCandidates with a clear majority may claim membership.\n{} blocks remaining ({}).\n\n",
                candidate_period.claim_blocks_left,
                format_duration(candidate_period.claim_time_left()),
            ));
        }
    }

    message.push_str(&format!(
        "\n**Candidate skeptic:** {}\n\n",
        format_account(candidate_skeptic)
    ));

    if candidate_period.kind == CandidatePeriodKind::Voting && new_period {
        message.push_str("**New challenge period started**\n\n");
    }

    message.push_str(&format!(
        "**Challenge**\n{} blocks until end ({}).\n\n",
        candidate_period.challenge_blocks_left(),
        format_duration(candidate_period.challenge_time_left()),
    ));

    message.push_str(&defender_message(defender_info, !new_period));
    message.push_str(&format!(
        "\n**Defender skeptic:** {}",
        format_account(defender_info.skeptic.as_deref())
    ));
    message
}

pub fn defender_message(defender: &Defender, include_tally: bool) -> String {
    let mut message = format!(
        "**Defender:** {}",
        format_account(defender.address_or_handle.as_deref())
    );
    if include_tally {
        message.push_str(&format!(
            "\n· {} approvals · {} rejections",
            defender.tally.approvals, defender.tally.rejections
        ));
    }
    message
}

pub fn head_message(head: Option<&str>) -> String {
    format!("**Head:** {}", format_account(head))
}

pub fn member_info_message(info: &MemberInfo) -> String {
    format!(
        "**Member**\n· Address: `{}`\n· State: {}\n· Strikes: {}\n· Roles: {}",
        format_address_with_handle(&info.address, info.element_handle.as_deref()),
        info.state,
        info.strikes,
        format_roles(info.is_founder, info.is_defender),
    )
}

pub fn skeptics_message(defender_skeptic: Option<&str>, candidate_skeptic: Option<&str>) -> String {
    format!(
        "**Defender skeptic:** {}\n**Candidate skeptic:** {}",
        format_account(defender_skeptic),
        format_account(candidate_skeptic),
    )
}

pub fn candidate_not_found_message(address: &str) -> String {
    format!("No candidate matching `{address}`.")
}

pub fn new_bid_message(block_number: u64, bid: &Bid) -> String {
    format!(
        "**New bid** (block {block_number})\n· `{}` — {} KSM\n",
        bid.address_or_handle,
        format_ksm(bid.bid_plancks)
    )
}

pub fn unbid_message(block_number: u64, address_or_handle: &str) -> String {
    format!("**Withdrawn bid** (block {block_number})\n· `{address_or_handle}`\n")
}

fn candidate_line(candidate: &Candidate) -> String {
    format!(
        "· `{}` — {} KSM · {} approvals · {} rejections\n",
        candidate.address_or_handle,
        format_ksm(candidate.bid_plancks),
        candidate.tally.approvals,
        candidate.tally.rejections,
    )
}

fn format_account(account: Option<&str>) -> String {
    match account {
        Some(account) => format!("`{account}`"),
        None => "none".to_owned(),
    }
}

fn format_roles(is_founder: bool, is_defender: bool) -> String {
    let mut roles = Vec::new();
    if is_founder {
        roles.push("founder");
    }
    if is_defender {
        roles.push("defender");
    }
    if roles.is_empty() {
        "none".to_owned()
    } else {
        roles.join(", ")
    }
}

fn format_ksm(plancks: u128) -> String {
    let whole = plancks / KSM_DIVISOR;
    let fraction = plancks % KSM_DIVISOR;
    if fraction == 0 {
        whole.to_string()
    } else {
        let fraction = format!("{fraction:012}");
        format!("{whole}.{}", fraction.trim_end_matches('0'))
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    format!("{days}d {hours}h {minutes}m {seconds}s")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MemberState, Tally};

    #[test]
    fn renders_no_candidates() {
        assert_eq!(candidates_message(&[]), "**Candidates**\nNone.");
    }

    #[test]
    fn renders_candidate_bid_in_ksm() {
        let message = candidates_message(&[Candidate {
            address_or_handle: "@candidate:matrix.org".to_owned(),
            bid_plancks: 1_500_000_000_000,
            tally: Tally {
                approvals: 3,
                rejections: 1,
            },
        }]);

        assert!(message.contains("· `@candidate:matrix.org` — 1.5 KSM"));
        assert!(message.contains("3 approvals · 1 rejections"));
    }

    #[test]
    fn renders_new_bid_message() {
        let message = new_bid_message(
            123,
            &Bid {
                address_or_handle: "candidate-a".to_owned(),
                bid_plancks: 1_500_000_000_000,
            },
        );

        assert_eq!(
            message,
            "**New bid** (block 123)\n· `candidate-a` — 1.5 KSM\n"
        );
    }

    #[test]
    fn renders_unbid_message() {
        assert_eq!(
            unbid_message(123, "candidate-a"),
            "**Withdrawn bid** (block 123)\n· `candidate-a`\n"
        );
    }

    #[test]
    fn renders_multiple_candidates_with_header() {
        let candidates = vec![
            Candidate {
                address_or_handle: "candidate-a".to_owned(),
                bid_plancks: 1_000_000_000_000,
                tally: Tally {
                    approvals: 1,
                    rejections: 0,
                },
            },
            Candidate {
                address_or_handle: "candidate-b".to_owned(),
                bid_plancks: 2_000_000_000_000,
                tally: Tally {
                    approvals: 2,
                    rejections: 1,
                },
            },
        ];

        let message = candidates_message(&candidates);
        assert!(message.starts_with("**Candidates**\n"));
        assert!(message.contains("· `candidate-a`"));
        assert!(message.contains("· `candidate-b`"));
    }

    #[test]
    fn renders_voting_period_with_new_period_text() {
        let period = CandidatePeriod::from_block(1);
        let defender = Defender {
            address_or_handle: Some("@defender:matrix.org".to_owned()),
            skeptic: Some("@skeptic:matrix.org".to_owned()),
            tally: Tally {
                approvals: 10,
                rejections: 3,
            },
        };
        let message = period_message(
            &period,
            &defender,
            &[],
            Some("@head:matrix.org"),
            Some("@candidate-skeptic:matrix.org"),
            true,
        );

        assert!(message.contains("**New voting period started**"));
        assert!(message.contains("**New challenge period started**"));
        assert!(message.contains("**Head:** `@head:matrix.org`"));
        assert!(!message.contains("approvals"));
    }

    #[test]
    fn renders_claim_period_and_defender_tally() {
        let period = CandidatePeriod::from_block(CandidatePeriod::VOTE_PERIOD_BLOCKS + 10);
        let defender = Defender {
            address_or_handle: None,
            skeptic: None,
            tally: Tally {
                approvals: 8,
                rejections: 6,
            },
        };
        let message = period_message(&period, &defender, &[], None, None, false);

        assert!(message.contains("**Claim period**"));
        assert!(message.contains("**Defender:** none"));
        assert!(message.contains("8 approvals · 6 rejections"));
        assert!(message.contains("**Candidate skeptic:** none"));
    }

    #[test]
    fn format_address_with_handle_shows_handle_in_parentheses() {
        assert_eq!(
            format_address_with_handle("addr", Some("@user:matrix.org")),
            "addr (@user:matrix.org)"
        );
        assert_eq!(format_address_with_handle("addr", None), "addr");
        assert_eq!(format_address_with_handle("addr", Some("addr")), "addr");
    }

    #[test]
    fn renders_member_info_and_roles() {
        let message = member_info_message(&MemberInfo {
            address: "addr".to_owned(),
            state: MemberState::Member,
            element_handle: Some("@member:matrix.org".to_owned()),
            strikes: 2,
            is_founder: true,
            is_defender: false,
        });

        assert!(message.contains("**Member**"));
        assert!(message.contains("· Address: `addr (@member:matrix.org)`"));
        assert!(message.contains("· State: member"));
        assert!(message.contains("· Roles: founder"));
    }
}
