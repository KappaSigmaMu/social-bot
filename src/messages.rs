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

pub fn member_info_message(info: &MemberInfo, include_handle: bool) -> String {
    let address = if include_handle {
        format_address_with_handle(&info.address, info.element_handle.as_deref())
    } else {
        info.address.clone()
    };
    let mut lines = vec![
        "**Member**".to_owned(),
        format!("· Address: `{address}`"),
        format!("· Status: {}", info.state),
        format!("· Strikes: {}", info.strikes),
    ];

    if let Some(roles) = format_roles(info.is_founder, info.is_defender) {
        lines.push(format!("· Roles: {roles}"));
    }

    lines.join("\n")
}

pub fn intake_countdown_message(blocks_remaining: u64, next_intake_at: u64) -> String {
    if blocks_remaining == 0 {
        return format!("Next intake is due now (at block {next_intake_at})");
    }

    let duration = Duration::from_secs(blocks_remaining * CandidatePeriod::SECONDS_PER_BLOCK);
    format!(
        "Next intake in {} (at block {next_intake_at})",
        format_readable_countdown(duration)
    )
}

pub fn format_readable_countdown(duration: Duration) -> String {
    let total = duration.as_secs();
    let days = total / 86_400;
    let hours = total % 86_400 / 3_600;
    let minutes = total % 3_600 / 60;
    let seconds = total % 60;

    format!(
        "{}, {}, {} and {}",
        format_countdown_component(days, "day"),
        format_countdown_component(hours, "hour"),
        format_countdown_component(minutes, "minute"),
        format_countdown_component(seconds, "second"),
    )
}

fn format_countdown_component(value: u64, singular: &str) -> String {
    if value == 1 {
        format!("1 {singular}")
    } else {
        format!("{value} {singular}s")
    }
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

pub fn claim_started_message(
    candidate_period: &CandidatePeriod,
    candidates: &[Candidate],
) -> String {
    let mut message = format!(
        "**Voting ended — claim period started**\nCandidates with a clear majority may claim membership.\n{} blocks remaining ({}).\n\n",
        candidate_period.claim_blocks_left,
        format_duration(candidate_period.claim_time_left()),
    );
    message.push_str(&candidates_message(candidates));
    message
}

pub fn vouch_message(
    block_number: u64,
    candidate: &str,
    voucher: &str,
    offer_plancks: u128,
) -> String {
    format!(
        "**Vouch** (block {block_number}) — {voucher} vouches for {candidate} with {} KSM",
        format_ksm(offer_plancks)
    )
}

pub fn unvouch_message(block_number: u64, candidate: &str) -> String {
    format!("**Unvouch** (block {block_number}) — {candidate} is no longer vouched for")
}

pub fn auto_unbid_message(block_number: u64, candidate: &str) -> String {
    format!("**Auto unbid** (block {block_number}) — {candidate} dropped (excess bids)")
}

pub fn inducted_message(block_number: u64, primary: &str, candidates: &[String]) -> String {
    let mut message = format!(
        "**Inducted** (block {block_number}) — {} new member(s); new head: {primary}",
        candidates.len()
    );
    if !candidates.is_empty() {
        message.push_str("\n· ");
        message.push_str(&candidates.join("\n· "));
    }
    message
}

pub fn challenged_message(block_number: u64, member: &str) -> String {
    format!("**Challenged** (block {block_number}) — {member} challenged; defender vote is on")
}

pub fn candidate_suspended_message(block_number: u64, candidate: &str) -> String {
    format!("**Candidate suspended** (block {block_number}) — {candidate}")
}

pub fn member_suspended_message(block_number: u64, member: &str) -> String {
    format!("**Member suspended** (block {block_number}) — {member}")
}

pub fn suspended_member_judgement_message(block_number: u64, who: &str, judged: bool) -> String {
    let verdict = if judged { "forgiven" } else { "convicted" };
    format!("**Judgement** (block {block_number}) — {who} {verdict}")
}

pub fn elevated_message(block_number: u64, member: &str, rank: u64) -> String {
    format!("**Elevated** (block {block_number}) — {member} elevated to rank {rank}")
}

pub fn vote_message(block_number: u64, voter: &str, approve: bool, candidate: &str) -> String {
    let verdict = if approve { "approved" } else { "rejected" };
    format!("**Vote** (block {block_number}) — {voter} {verdict} candidate {candidate}")
}

pub fn defender_vote_message(
    block_number: u64,
    voter: &str,
    approve: bool,
    defender: &str,
) -> String {
    let verdict = if approve { "approved" } else { "rejected" };
    format!("**Defender vote** (block {block_number}) — {voter} {verdict} defender {defender}")
}

pub fn x_round_start_message(
    block_number: u64,
    candidate_count: usize,
    duration: Duration,
) -> String {
    format!(
        "New Kusama Society voting period started (block {block_number}). {candidate_count} candidate(s), ~{} to vote.",
        format_duration_short(duration)
    )
}

pub fn x_claim_start_message(block_number: u64) -> String {
    format!(
        "Kusama Society claim period started (block {block_number}). Candidates with a clear majority may claim membership."
    )
}

pub fn x_bid_message(block_number: u64, display: &str, bid_plancks: u128) -> String {
    format!(
        "New Kusama Society bid (block {block_number}): {display} — {} KSM",
        format_ksm(bid_plancks)
    )
}

pub fn x_unbid_message(block_number: u64, display: &str) -> String {
    format!("Kusama Society bid withdrawn (block {block_number}): {display}")
}

pub fn x_vouch_message(
    block_number: u64,
    voucher: &str,
    candidate: &str,
    offer_plancks: u128,
) -> String {
    format!(
        "New Kusama Society vouch (block {block_number}): {voucher} vouches for {candidate} — {} KSM",
        format_ksm(offer_plancks)
    )
}

pub fn x_inducted_message(block_number: u64, count: usize, primary_display: &str) -> String {
    format!(
        "Kusama Society inducted {count} new member(s) (block {block_number}). New head: {primary_display}"
    )
}

pub fn x_challenged_message(block_number: u64, display: &str) -> String {
    format!(
        "Kusama Society member challenged (block {block_number}): {display} — defender vote is on."
    )
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

fn format_roles(is_founder: bool, is_defender: bool) -> Option<String> {
    let mut roles = Vec::new();
    if is_founder {
        roles.push("founder");
    }
    if is_defender {
        roles.push("defender");
    }
    if roles.is_empty() {
        None
    } else {
        Some(roles.join(", "))
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

fn format_duration_short(duration: Duration) -> String {
    let total = duration.as_secs();
    let days = total / 86_400;
    let hours = total % 86_400 / 3_600;
    let minutes = total % 3_600 / 60;
    if days > 0 {
        if hours > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else {
        format!("{minutes}m")
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
    fn renders_readable_countdown() {
        assert_eq!(
            format_readable_countdown(Duration::from_secs(90_061)),
            "1 day, 1 hour, 1 minute and 1 second"
        );
        assert_eq!(
            format_readable_countdown(Duration::from_secs(0)),
            "0 days, 0 hours, 0 minutes and 0 seconds"
        );
    }

    #[test]
    fn renders_intake_countdown_message() {
        assert_eq!(
            intake_countdown_message(0, 1_234_567),
            "Next intake is due now (at block 1234567)"
        );
        assert_eq!(
            intake_countdown_message(10, 1_234_567),
            "Next intake in 0 days, 0 hours, 1 minute and 0 seconds (at block 1234567)"
        );
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
        let message = member_info_message(
            &MemberInfo {
                address: "addr".to_owned(),
                state: MemberState::Member,
                element_handle: Some("@member:matrix.org".to_owned()),
                strikes: 2,
                is_founder: true,
                is_defender: false,
            },
            true,
        );

        assert!(message.contains("**Member**"));
        assert!(message.contains("· Address: `addr (@member:matrix.org)`"));
        assert!(message.contains("· Status: member"));
        assert!(message.contains("· Roles: founder"));
    }

    #[test]
    fn omits_handle_for_me() {
        let message = member_info_message(
            &MemberInfo {
                address: "addr".to_owned(),
                state: MemberState::Member,
                element_handle: Some("@member:matrix.org".to_owned()),
                strikes: 0,
                is_founder: false,
                is_defender: false,
            },
            false,
        );

        assert!(message.contains("· Address: `addr`"));
        assert!(!message.contains("@member:matrix.org"));
    }

    #[test]
    fn omits_roles_when_none() {
        let message = member_info_message(
            &MemberInfo {
                address: "addr".to_owned(),
                state: MemberState::Member,
                element_handle: None,
                strikes: 0,
                is_founder: false,
                is_defender: false,
            },
            true,
        );

        assert!(message.contains("· Status: member"));
        assert!(!message.contains("Roles"));
    }

    #[test]
    fn renders_claim_started_message_with_candidate_tallies() {
        let period = CandidatePeriod::from_block(CandidatePeriod::VOTE_PERIOD_BLOCKS);
        let candidates = vec![
            Candidate {
                address_or_handle: "candidate-a".to_owned(),
                bid_plancks: 1_000_000_000_000,
                tally: Tally {
                    approvals: 2,
                    rejections: 0,
                },
            },
            Candidate {
                address_or_handle: "candidate-b".to_owned(),
                bid_plancks: 2_000_000_000_000,
                tally: Tally {
                    approvals: 0,
                    rejections: 3,
                },
            },
        ];
        let message = claim_started_message(&period, &candidates);

        assert!(message.contains("**Voting ended — claim period started**"));
        assert!(message.contains("**Candidates**"));
        assert!(message.contains("· `candidate-a` — 1 KSM · 2 approvals · 0 rejections"));
        assert!(message.contains("· `candidate-b` — 2 KSM · 0 approvals · 3 rejections"));
    }

    #[test]
    fn renders_thread_event_messages() {
        assert_eq!(
            vouch_message(1, "candidate-a", "voucher-b", 1_500_000_000_000),
            "**Vouch** (block 1) — voucher-b vouches for candidate-a with 1.5 KSM"
        );
        assert_eq!(
            unvouch_message(2, "candidate-a"),
            "**Unvouch** (block 2) — candidate-a is no longer vouched for"
        );
        assert_eq!(
            auto_unbid_message(3, "candidate-a"),
            "**Auto unbid** (block 3) — candidate-a dropped (excess bids)"
        );
        assert_eq!(
            inducted_message(
                4,
                "primary-a",
                &["candidate-a".to_owned(), "candidate-b".to_owned()]
            ),
            "**Inducted** (block 4) — 2 new member(s); new head: primary-a\n· candidate-a\n· candidate-b"
        );
        assert_eq!(
            challenged_message(5, "member-a"),
            "**Challenged** (block 5) — member-a challenged; defender vote is on"
        );
        assert_eq!(
            candidate_suspended_message(6, "candidate-a"),
            "**Candidate suspended** (block 6) — candidate-a"
        );
        assert_eq!(
            member_suspended_message(7, "member-a"),
            "**Member suspended** (block 7) — member-a"
        );
        assert_eq!(
            suspended_member_judgement_message(8, "who-a", true),
            "**Judgement** (block 8) — who-a forgiven"
        );
        assert_eq!(
            suspended_member_judgement_message(8, "who-a", false),
            "**Judgement** (block 8) — who-a convicted"
        );
        assert_eq!(
            elevated_message(9, "member-a", 2),
            "**Elevated** (block 9) — member-a elevated to rank 2"
        );
        assert_eq!(
            vote_message(10, "voter-a", true, "candidate-a"),
            "**Vote** (block 10) — voter-a approved candidate candidate-a"
        );
        assert_eq!(
            vote_message(10, "voter-a", false, "candidate-a"),
            "**Vote** (block 10) — voter-a rejected candidate candidate-a"
        );
        assert_eq!(
            defender_vote_message(11, "voter-a", true, "defender-a"),
            "**Defender vote** (block 11) — voter-a approved defender defender-a"
        );
    }

    #[test]
    fn x_messages_stay_within_twitter_limit() {
        let long_display = "Qm".repeat(60);
        let messages = vec![
            x_round_start_message(123_456, 3, Duration::from_secs(4 * 86_400)),
            x_claim_start_message(123_456),
            x_bid_message(123_456, &long_display, 350_000_000_000_000),
            x_unbid_message(123_456, &long_display),
            x_vouch_message(123_456, &long_display, "candidate", 350_000_000_000_000),
            x_inducted_message(123_456, 5, &long_display),
            x_challenged_message(123_456, &long_display),
        ];
        for message in messages {
            assert!(
                message.chars().count() <= 280,
                "X message exceeds 280 chars: {message}"
            );
        }
    }

    #[test]
    fn renders_x_messages() {
        assert_eq!(
            x_round_start_message(123_456, 3, Duration::from_secs(4 * 86_400 + 2 * 3_600)),
            "New Kusama Society voting period started (block 123456). 3 candidate(s), ~4d 2h to vote."
        );
        assert_eq!(
            x_claim_start_message(123_456),
            "Kusama Society claim period started (block 123456). Candidates with a clear majority may claim membership."
        );
        assert_eq!(
            x_bid_message(123_456, "Alice", 350_000_000_000_000),
            "New Kusama Society bid (block 123456): Alice — 350 KSM"
        );
        assert_eq!(
            x_unbid_message(123_456, "Alice"),
            "Kusama Society bid withdrawn (block 123456): Alice"
        );
        assert_eq!(
            x_vouch_message(123_456, "Bob", "Alice", 350_000_000_000_000),
            "New Kusama Society vouch (block 123456): Bob vouches for Alice — 350 KSM"
        );
        assert_eq!(
            x_inducted_message(123_456, 3, "Alice"),
            "Kusama Society inducted 3 new member(s) (block 123456). New head: Alice"
        );
        assert_eq!(
            x_challenged_message(123_456, "Alice"),
            "Kusama Society member challenged (block 123456): Alice — defender vote is on."
        );
    }
}
