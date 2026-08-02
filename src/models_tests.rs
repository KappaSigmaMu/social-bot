#[cfg(test)]
mod tests {
    use crate::models::{CandidatePeriod, CandidatePeriodKind, MemberState, SocietyEventKind};

    #[test]
    fn displays_member_states() {
        assert_eq!(MemberState::Member.to_string(), "member");
        assert_eq!(MemberState::Candidate.to_string(), "candidate");
        assert_eq!(MemberState::SuspendedMember.to_string(), "suspended member");
        assert_eq!(MemberState::NonMember.to_string(), "non-member");
    }

    #[test]
    fn event_kinds_round_trip_through_storage_names() {
        assert_eq!(SocietyEventKind::Bid.as_str(), "Bid");
        assert_eq!(SocietyEventKind::Unbid.as_str(), "Unbid");
        assert_eq!(SocietyEventKind::Vouch.as_str(), "Vouch");
        assert_eq!(SocietyEventKind::Unvouch.as_str(), "Unvouch");
        assert_eq!(SocietyEventKind::AutoUnbid.as_str(), "AutoUnbid");
        assert_eq!(SocietyEventKind::Inducted.as_str(), "Inducted");
        assert_eq!(SocietyEventKind::Challenged.as_str(), "Challenged");
        assert_eq!(
            SocietyEventKind::CandidateSuspended.as_str(),
            "CandidateSuspended"
        );
        assert_eq!(
            SocietyEventKind::MemberSuspended.as_str(),
            "MemberSuspended"
        );
        assert_eq!(
            SocietyEventKind::SuspendedMemberJudgement.as_str(),
            "SuspendedMemberJudgement"
        );
        assert_eq!(SocietyEventKind::Elevated.as_str(), "Elevated");
        assert_eq!(SocietyEventKind::Vote.as_str(), "Vote");
        assert_eq!(SocietyEventKind::DefenderVote.as_str(), "DefenderVote");
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
