use blake2::{Blake2b512, Digest};

const SS58_PREFIX: &[u8] = b"SS58PRE";

pub fn is_valid_matrix_handle(matrix_handle: &str) -> bool {
    regex::Regex::new(r"^@[^:]+:.*\..*$")
        .expect("valid matrix handle regex")
        .is_match(matrix_handle)
}

pub fn is_valid_address(address: &str) -> bool {
    decode_account_id(address).is_some()
}

pub fn decode_account_id(address: &str) -> Option<[u8; 32]> {
    let bytes = bs58::decode(address).into_vec().ok()?;
    if bytes.len() != 35 && bytes.len() != 36 && bytes.len() != 37 {
        return None;
    }

    let prefix_len = if bytes[0] & 0b0100_0000 == 0 { 1 } else { 2 };
    let checksum_len = bytes.len().checked_sub(prefix_len + 32)?;
    if !(2..=8).contains(&checksum_len) {
        return None;
    }

    let payload_len = prefix_len + 32;
    let payload = &bytes[..payload_len];
    let checksum = &bytes[payload_len..];
    let expected = ss58_checksum(payload);
    if checksum != &expected[..checksum_len] {
        return None;
    }

    let mut account = [0u8; 32];
    account.copy_from_slice(&payload[prefix_len..payload_len]);
    Some(account)
}

pub fn encode_account_id(account_id: &[u8; 32], network: u16) -> String {
    let mut payload = Vec::with_capacity(35);
    if network < 64 {
        payload.push(network as u8);
    } else {
        let first = ((network & 0b0000_0000_1111_1100) >> 2) as u8 | 0b0100_0000;
        let second = ((network >> 8) as u8) | ((network & 0b0000_0000_0000_0011) as u8) << 6;
        payload.extend([first, second]);
    }
    payload.extend(account_id);

    let checksum = ss58_checksum(&payload);
    payload.extend(&checksum[..2]);
    bs58::encode(payload).into_string()
}

fn ss58_checksum(payload: &[u8]) -> [u8; 64] {
    let mut hasher = Blake2b512::new();
    hasher.update(SS58_PREFIX);
    hasher.update(payload);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_matrix_handles_like_python_bot() {
        assert!(is_valid_matrix_handle("@testuser:matrix.org"));
        assert!(is_valid_matrix_handle("@good_address:matrix.org"));
        assert!(is_valid_matrix_handle("@test:dodgydomain."));
        assert!(!is_valid_matrix_handle("asdasdas dasf sdf "));
        assert!(!is_valid_matrix_handle("testuser@matrix.org"));
    }

    #[test]
    fn validates_known_kusama_addresses() {
        assert!(is_valid_address(
            "FUfBKr2pDxKrxmExGp4hjU6St4BDgffzKcyAqv6pruGnez1"
        ));
        assert!(!is_valid_address("asdasdasdasd"));
    }
}
