#[cfg(test)]
mod tests {
    use crate::models::{
        CandidatePeriod, CandidatePeriodKind, MemberState, SeenSocietyEvents, SocietyEventId,
        SocietyEventKind,
    };

    #[test]
    fn deduplicates_society_events_by_stable_id() {
        let seen = SeenSocietyEvents::new();
        let id = SocietyEventId {
            block_hash: [1u8; 32],
            event_index: 7,
            kind: SocietyEventKind::Bid,
        };

        assert!(seen.mark_seen(&id));
        assert!(!seen.mark_seen(&id));
    }

    #[test]
    fn keeps_distinct_events_in_the_same_block() {
        let seen = SeenSocietyEvents::new();
        let block_hash = [2u8; 32];

        assert!(seen.mark_seen(&SocietyEventId {
            block_hash,
            event_index: 0,
            kind: SocietyEventKind::Bid,
        }));
        assert!(seen.mark_seen(&SocietyEventId {
            block_hash,
            event_index: 1,
            kind: SocietyEventKind::Unbid,
        }));
    }

    #[test]
    fn displays_member_states() {
        assert_eq!(MemberState::Member.to_string(), "member");
        assert_eq!(MemberState::Candidate.to_string(), "candidate");
        assert_eq!(MemberState::SuspendedMember.to_string(), "suspended member");
        assert_eq!(MemberState::NonMember.to_string(), "non-member");
    }

    #[test]
    fn calculates_voting_and_claim_periods() {
        let start = CandidatePeriod::from_block(0);
        assert_eq!(start.kind, CandidatePeriodKind::Voting);
        assert_eq!(start.voting_blocks_left, 72_000);
        assert_eq!(start.claim_blocks_left, 28_800);
        assert_eq!(start.challenge_blocks_left(), 100_800);
        assert_eq!(start.voting_time_left().as_secs(), 432_000);
        assert_eq!(start.claim_time_left().as_secs(), 172_800);
        assert_eq!(start.challenge_time_left().as_secs(), 604_800);

        let claim = CandidatePeriod::from_block(72_000);
        assert_eq!(claim.kind, CandidatePeriodKind::Claim);
        assert_eq!(claim.voting_blocks_left, 0);
        assert_eq!(claim.claim_blocks_left, 28_800);
    }
}
