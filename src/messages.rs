use crate::models::{Candidate, CandidatePeriod, CandidatePeriodKind, Defender};
use std::time::Duration;

const KSM_DIVISOR: u128 = 1_000_000_000_000;

pub fn candidates_message(candidates: &[Candidate]) -> String {
    match candidates {
        [] => "There are no candidates\n".to_owned(),
        [candidate] => candidate_line(candidate),
        candidates => {
            let mut message = "The current candidates are:\n".to_owned();
            for candidate in candidates {
                message.push_str(&candidate_line(candidate));
            }
            message
        }
    }
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
        message.push_str("A new candidate period has started.\n\n");
    }

    match candidate_period.kind {
        CandidatePeriodKind::Voting => {
            message.push_str(&format!(
                "We are currently in the voting period. Candidates should provide proof of ink. Members should vote on candidates. Blocks until end of voting period: {} ({})\n \n{}",
                candidate_period.voting_blocks_left,
                format_duration(candidate_period.voting_time_left()),
                candidates_message(candidates),
            ));
            message.push_str(&format!(
                "\nThe current head is {}.\n",
                head.unwrap_or("None")
            ));
        }
        CandidatePeriodKind::Claim => {
            message.push_str(&format!(
                "We are currently in the claim period. If you were a candidate in the previous period and received a clear majority of votes, you may now claim your membership. Blocks until end of claim period: {} ({})\n",
                candidate_period.claim_blocks_left,
                format_duration(candidate_period.claim_time_left()),
            ));
        }
    }

    message.push_str(&format!(
        "\nThe current skeptic for the candidates is {}.\n\n-----\n\n",
        candidate_skeptic.unwrap_or("None")
    ));

    if candidate_period.kind == CandidatePeriodKind::Voting && new_period {
        message.push_str("A new challenge period has also started.\n\n");
    }

    message.push_str(&format!(
        "There are currently {} blocks ({}) until the end of the challenge period.\n\n",
        candidate_period.challenge_blocks_left(),
        format_duration(candidate_period.challenge_time_left()),
    ));

    message.push_str(&format!(
        "The current defender is {}.\n",
        defender_info.address_or_handle.as_deref().unwrap_or("None")
    ));
    if !new_period {
        message.push_str(&format!(
            "  * Approvals: {}\n  * Rejections: {}\n",
            defender_info.tally.approvals, defender_info.tally.rejections
        ));
    }
    message.push_str(&format!(
        "\nThe current skeptic for the defender is {}.\n",
        defender_info.skeptic.as_deref().unwrap_or("None")
    ));
    message
}

fn candidate_line(candidate: &Candidate) -> String {
    format!(
        "* {}\n  * Bid: {} KSM\n  * Approvals: {}, Rejections: {}\n",
        candidate.address_or_handle,
        format_ksm(candidate.bid_plancks),
        candidate.tally.approvals,
        candidate.tally.rejections
    )
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
    format!("{days} days, {hours} hours, {minutes} minutes, {seconds} seconds")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Tally;

    #[test]
    fn renders_no_candidates() {
        assert_eq!(candidates_message(&[]), "There are no candidates\n");
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

        assert!(message.contains("Bid: 1.5 KSM"));
        assert!(message.contains("Approvals: 3, Rejections: 1"));
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
        assert!(message.starts_with("The current candidates are:"));
        assert!(message.contains("* candidate-a"));
        assert!(message.contains("* candidate-b"));
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

        assert!(message.contains("A new candidate period has started."));
        assert!(message.contains("A new challenge period has also started."));
        assert!(message.contains("The current head is @head:matrix.org."));
        assert!(!message.contains("Approvals: 10"));
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

        assert!(message.contains("We are currently in the claim period."));
        assert!(message.contains("The current defender is None."));
        assert!(message.contains("Approvals: 8"));
        assert!(message.contains("The current skeptic for the candidates is None."));
    }
}
